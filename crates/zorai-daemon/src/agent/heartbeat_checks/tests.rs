use super::*;
use crate::session_manager::SessionManager;
use std::collections::VecDeque;
use std::sync::Arc;
use tempfile::tempdir;

fn make_todo(id: &str, content: &str, status: TodoStatus, updated_at: u64) -> TodoItem {
    TodoItem {
        id: id.to_string(),
        content: content.to_string(),
        status,
        position: 0,
        step_index: None,
        created_at: updated_at,
        updated_at,
    }
}

fn make_goal_run(id: &str, title: &str, status: GoalRunStatus, updated_at: u64) -> GoalRun {
    GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        goal: title.to_string(),
        client_request_id: None,
        status,
        priority: TaskPriority::Normal,
        created_at: updated_at,
        updated_at,
        started_at: Some(updated_at),
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 2,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        pending_review_report: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: Default::default(),
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        supervision_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

/// Helper to build a minimal AgentEngine for testing check functions.
/// Only populates the fields needed by check functions.
async fn make_test_engine(
    todos: HashMap<String, Vec<TodoItem>>,
    goal_runs: VecDeque<GoalRun>,
    gateway_threads: HashMap<String, String>,
) -> Arc<AgentEngine> {
    use crate::agent::circuit_breaker::CircuitBreakerRegistry;
    use crate::agent::concierge::ConciergeEngine;

    let config = AgentConfig::default();
    let mlflow_tracing = crate::agent::mlflow_tracing::MlflowTracingRuntime::new(
        &std::env::temp_dir(),
        &config.mlflow_tracing,
    );
    let (event_tx, _) = broadcast::channel(16);
    let (watcher_refresh_tx, watcher_refresh_rx) = mpsc::unbounded_channel();
    let http_client = reqwest::Client::new();
    let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
        std::iter::empty::<String>(),
    ));
    let config = Arc::new(RwLock::new(config));
    let concierge = Arc::new(ConciergeEngine::new(
        config.clone(),
        event_tx.clone(),
        http_client.clone(),
        circuit_breakers.clone(),
    ));

    let data_dir =
        std::env::temp_dir().join(format!("zorai-test-heartbeat-checks-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&data_dir);
    let (skill_discovery_result_tx, _skill_discovery_result_rx) = mpsc::unbounded_channel();
    let (auto_thread_title_jobs, _auto_thread_title_rx) = mpsc::unbounded_channel();
    let (prompt_queue_wake_tx, _prompt_queue_wake_rx) = mpsc::unbounded_channel();
    let (continuation_flush_tx, _continuation_flush_rx) = mpsc::unbounded_channel();
    let (internal_dm_jobs_tx, _internal_dm_jobs_rx) = mpsc::unbounded_channel();

    let history = crate::history::HistoryStore::new_test_store(&data_dir)
        .await
        .expect("test history store");
    let sm = crate::session_manager::SessionManager::new_with_history(
        Arc::new(
            crate::history::HistoryStore::new_test_store(&data_dir)
                .await
                .expect("test history store for session manager"),
        ),
        1024,
    );

    Arc::new(AgentEngine {
        started_at_ms: now_millis(),
        config,
        http_client,
        concierge,
        session_manager: sm,
        history,
        mlflow_tracing,
        threads: RwLock::new(HashMap::new()),
        thread_message_hydration_pending: RwLock::new(HashSet::new()),
        thread_message_hydration_lock: Mutex::new(()),
        semantic_document_index_sync_lock: Mutex::new(()),
        semantic_vector_index_lock: Mutex::new(()),
        thread_message_hydration_test_delay: Mutex::new(None),
        thread_handoff_states: RwLock::new(HashMap::new()),
        thread_participants: RwLock::new(HashMap::new()),
        thread_participant_suggestions: RwLock::new(HashMap::new()),
        deferred_visible_thread_continuations: Mutex::new(HashMap::new()),
        active_visible_thread_continuation_flushes: std::sync::Mutex::new(HashSet::new()),
        active_thread_participant_suggestion_drains: Mutex::new(HashSet::new()),
        thread_client_surfaces: RwLock::new(HashMap::new()),
        thread_execution_profiles: RwLock::new(HashMap::new()),
        thread_identity_metadata: RwLock::new(HashMap::new()),
        thread_skill_discovery_states: RwLock::new(HashMap::new()),
        thread_memory_injection_states: RwLock::new(HashMap::new()),
        thread_structural_memories: RwLock::new(HashMap::new()),
        thread_todos: RwLock::new(todos),
        thread_work_contexts: RwLock::new(HashMap::new()),
        resonance_context_cache: RwLock::new(HashMap::new()),
        tasks: Mutex::new(VecDeque::new()),
        goal_runs: Mutex::new(goal_runs),
        goal_step_completion_marker_retries: Mutex::new(HashMap::new()),
        goal_run_client_surfaces: RwLock::new(HashMap::new()),
        inflight_goal_runs: Mutex::new(HashSet::new()),
        heartbeat_items: RwLock::new(Vec::new()),
        event_tx,
        internal_event_tx: crate::agent::internal_event::new_internal_event_channel(),
        memory: RwLock::new(HashMap::new()),
        recent_policy_decisions: RwLock::new(HashMap::new()),
        retry_guards: RwLock::new(HashMap::new()),
        operator_model: RwLock::new(OperatorModel::default()),
        meta_cognitive_self_model: RwLock::new(super::metacognitive::types::SelfModel::default()),
        anticipatory: RwLock::new(AnticipatoryRuntime::default()),
        collaboration: RwLock::new(HashMap::new()),
        tool_synthesis_gap_notices: RwLock::new(HashSet::new()),
        data_dir,
        workspace_root: None,
        gateway_process: Mutex::new(None),
        gateway_init_lock: Mutex::new(()),
        gateway_state: Mutex::new(None),
        gateway_init_test_delay: Mutex::new(None),
        gateway_ipc_sender: Mutex::new(None),
        gateway_pending_send_results: Mutex::new(HashMap::new()),
        gateway_restart_attempts: Mutex::new(0),
        gateway_restart_not_before_ms: Mutex::new(None),
        gateway_discord_channels: RwLock::new(Vec::new()),
        gateway_slack_channels: RwLock::new(Vec::new()),
        gateway_threads: RwLock::new(gateway_threads),
        gateway_route_modes: RwLock::new(HashMap::new()),
        gateway_seen_ids: Mutex::new(Vec::new()),
        gateway_inflight_channels: Mutex::new(HashSet::new()),
        gateway_pending_followups: Mutex::new(HashMap::new()),
        gateway_injected_messages: Mutex::new(VecDeque::new()),
        webhook_listener_addr: RwLock::new(None),
        whatsapp_link: Arc::new(super::whatsapp_link::WhatsAppLinkRuntime::new()),
        external_runners: RwLock::new(HashMap::new()),
        subagent_runtime: RwLock::new(HashMap::new()),
        agent_terminal_leases: Mutex::new(HashMap::new()),
        trusted_weles_tasks: RwLock::new(HashSet::new()),
        weles_health: RwLock::new(WelesHealthStatus {
            state: WelesHealthState::Healthy,
            reason: None,
            checked_at: 0,
        }),
        stream_cancellations: Mutex::new(HashMap::new()),
        stream_providers: Mutex::new(HashMap::new()),
        operator_stopped_streams: Mutex::new(HashSet::new()),
        stream_generation: AtomicU64::new(1),
        stalled_turn_candidates: Mutex::new(HashMap::new()),
        operation_wakeups: Mutex::new(HashMap::new()),
        timer_wakeups: Mutex::new(HashMap::new()),
        active_operator_sessions: RwLock::new(HashMap::new()),
        pending_operator_approvals: RwLock::new(HashMap::new()),
        pending_approval_commands: RwLock::new(HashMap::new()),
        critique_approval_continuations: Mutex::new(HashMap::new()),
        policy_escalation_session_grants: RwLock::new(HashSet::new()),
        task_approval_rules: RwLock::new(Vec::new()),
        pending_operator_questions: Mutex::new(HashMap::new()),
        operator_profile_sessions: RwLock::new(HashMap::new()),
        honcho_sync: Mutex::new(HonchoSyncState::default()),
        repo_watchers: Mutex::new(HashMap::new()),
        watcher_refresh_tx,
        watcher_refresh_rx: Mutex::new(Some(watcher_refresh_rx)),
        skill_discovery_result_tx,
        auto_thread_title_jobs,
        prompt_queue_wake_tx,
        continuation_flush_tx,
        internal_dm_jobs_tx,
        skill_discovery_test_runner: std::sync::OnceLock::new(),
        force_mesh_discovery_degraded_for_tests: std::sync::atomic::AtomicBool::new(false),
        aline_startup_reconcile_started: std::sync::atomic::AtomicBool::new(false),
        aline_startup_test_completion: std::sync::OnceLock::new(),
        aline_startup_test_runner: std::sync::OnceLock::new(),
        aline_startup_test_availability: std::sync::OnceLock::new(),
        aline_startup_test_repo_roots: Mutex::new(Vec::new()),
        aline_startup_last_summary: Mutex::new(None),
        circuit_breakers,
        provider_availability_signals: RwLock::new(HashMap::new()),
        config_notify: tokio::sync::Notify::new(),
        config_runtime_projection: Mutex::new(ConfigRuntimeProjection::default()),
        learned_check_weights: RwLock::new(HashMap::new()),
        heuristic_store: RwLock::new(super::learning::heuristics::HeuristicStore::default()),
        pattern_store: RwLock::new(super::learning::patterns::PatternStore::default()),
        disclosure_queue: RwLock::new(super::capability_tier::DisclosureQueue::default()),
        plugin_manager: std::sync::OnceLock::new(),
        episodic_store: RwLock::new(HashMap::new()),
        awareness: RwLock::new(super::awareness::AwarenessMonitor::new()),
        calibration_tracker: RwLock::new(
            super::uncertainty::calibration::CalibrationTracker::default(),
        ),
        handoff_broker: RwLock::new(super::handoff::HandoffBroker::default()),
        specialist_embedding_cache: tokio::sync::Mutex::new(
            super::handoff::SpecialistEmbeddingCache::default(),
        ),
        divergent_sessions: RwLock::new(HashMap::new()),
        debate_sessions: RwLock::new(HashMap::new()),
        cost_trackers: Mutex::new(HashMap::new()),
        goal_run_health_monitors: RwLock::new(HashMap::new()),
    })
}

#[tokio::test]
async fn heartbeat_checks_stale_todos_detects_old_pending() {
    let now = now_millis();
    let old = now - (25 * 3600 * 1000);

    let mut todos = HashMap::new();
    todos.insert(
        "thread-1".to_string(),
        vec![
            make_todo("todo-1", "Fix bug", TodoStatus::Pending, old),
            make_todo("todo-2", "Write docs", TodoStatus::InProgress, old),
            make_todo("todo-3", "Ship it", TodoStatus::Completed, old),
        ],
    );

    let engine = make_test_engine(todos, VecDeque::new(), HashMap::new()).await;
    let result = engine.check_stale_todos(24).await;

    assert_eq!(result.check_type, HeartbeatCheckType::StaleTodos);
    assert_eq!(result.items_found, 2);
    assert_eq!(result.details.len(), 2);
}

#[tokio::test]
async fn heartbeat_checks_stale_todos_ignore_heartbeat_synthesis_threads() {
    let now = now_millis();
    let old = now - (25 * 3600 * 1000);
    let heartbeat_thread_id = "thread-heartbeat";
    let mut todos = HashMap::new();
    todos.insert(
        heartbeat_thread_id.to_string(),
        vec![make_todo(
            "todo-heartbeat",
            "Decide whether to act on paused goal runs",
            TodoStatus::Pending,
            old,
        )],
    );
    todos.insert(
        "thread-user".to_string(),
        vec![make_todo(
            "todo-user",
            "Fix actual user task",
            TodoStatus::Pending,
            old,
        )],
    );

    let engine = make_test_engine(todos, VecDeque::new(), HashMap::new()).await;
    engine.threads.write().await.insert(
        heartbeat_thread_id.to_string(),
        AgentThread {
            id: heartbeat_thread_id.to_string(),
            agent_name: Some("Weles".to_string()),
            title: "HEARTBEAT SYNTHESIS\nScheduled check".to_string(),
            messages: Vec::new(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: old,
            updated_at: old,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    let result = engine.check_stale_todos(24).await;

    assert_eq!(result.items_found, 1);
    assert_eq!(result.details[0].id, "todo-user");
}

#[tokio::test]
async fn heartbeat_checks_stale_todos_none_stale() {
    let now = now_millis();
    let recent = now - (1 * 3600 * 1000);

    let mut todos = HashMap::new();
    todos.insert(
        "thread-1".to_string(),
        vec![make_todo("todo-1", "New task", TodoStatus::Pending, recent)],
    );

    let engine = make_test_engine(todos, VecDeque::new(), HashMap::new()).await;
    let result = engine.check_stale_todos(24).await;

    assert_eq!(result.items_found, 0);
    assert!(result.summary.contains("No stale TODOs"));
}

#[tokio::test]
async fn heartbeat_checks_stuck_goals_detects_old_running() {
    let now = now_millis();
    let old = now - (3 * 3600 * 1000);

    let goal_runs = VecDeque::from(vec![
        make_goal_run("goal-1", "Deploy feature", GoalRunStatus::Running, old),
        make_goal_run(
            "goal-2",
            "Complete migration",
            GoalRunStatus::Completed,
            old,
        ),
    ]);

    let engine = make_test_engine(HashMap::new(), goal_runs, HashMap::new()).await;
    let result = engine.check_stuck_goal_runs(2).await;

    assert_eq!(result.check_type, HeartbeatCheckType::StuckGoalRuns);
    assert_eq!(result.items_found, 1);
    assert_eq!(result.details[0].id, "goal-1");
}

#[tokio::test]
async fn heartbeat_checks_stuck_goals_use_persisted_goal_runs_after_live_queue_clear() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let now = now_millis();
    let old = now - (3 * 3600 * 1000);

    engine.goal_runs.lock().await.push_back(make_goal_run(
        "goal-persisted-stuck",
        "Persisted stuck goal",
        GoalRunStatus::Running,
        old,
    ));
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    let result = engine.check_stuck_goal_runs(2).await;

    assert_eq!(result.check_type, HeartbeatCheckType::StuckGoalRuns);
    assert_eq!(result.items_found, 1);
    assert_eq!(result.details[0].id, "goal-persisted-stuck");
}

#[tokio::test]
async fn heartbeat_checks_stuck_goals_none_stuck() {
    let now = now_millis();
    let recent = now - (30 * 60 * 1000);

    let goal_runs = VecDeque::from(vec![make_goal_run(
        "goal-1",
        "Active goal",
        GoalRunStatus::Running,
        recent,
    )]);

    let engine = make_test_engine(HashMap::new(), goal_runs, HashMap::new()).await;
    let result = engine.check_stuck_goal_runs(2).await;

    assert_eq!(result.items_found, 0);
    assert!(result.summary.contains("No stuck"));
}

#[tokio::test]
async fn heartbeat_checks_unreplied_empty_gateway() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    let result = engine.check_unreplied_messages(1).await;

    assert_eq!(
        result.check_type,
        HeartbeatCheckType::UnrepliedGatewayMessages
    );
    assert_eq!(result.items_found, 0);
    assert!(result.summary.contains("No unreplied gateway"));
}

#[tokio::test]
async fn heartbeat_checks_ignore_channels_with_newer_response_timestamps() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    let mut gateway_state = crate::agent::gateway::GatewayState::new(
        AgentConfig::default().gateway,
        reqwest::Client::new(),
    );
    let now = now_millis();
    gateway_state
        .last_incoming_at
        .insert("Slack:C123".to_string(), now.saturating_sub(3_600_000));
    gateway_state
        .last_response_at
        .insert("Slack:C123".to_string(), now.saturating_sub(60_000));
    *engine.gateway_state.lock().await = Some(gateway_state);

    let result = engine.check_unreplied_messages(1).await;
    assert_eq!(result.items_found, 0);
    assert!(result.summary.contains("No unreplied gateway"));
}

#[tokio::test]
async fn heartbeat_checks_read_gateway_health_from_ipc_updates() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    let mut gateway_state = crate::agent::gateway::GatewayState::new(
        AgentConfig::default().gateway,
        reqwest::Client::new(),
    );
    crate::agent::gateway_loop::apply_health_snapshot(
        &mut gateway_state,
        &zorai_protocol::GatewayHealthState {
            platform: "slack".to_string(),
            status: zorai_protocol::GatewayConnectionStatus::Error,
            last_success_at_ms: Some(100),
            last_error_at_ms: Some(200),
            consecutive_failure_count: 3,
            last_error: Some("api timeout".to_string()),
            current_backoff_secs: 30,
        },
    );
    *engine.gateway_state.lock().await = Some(gateway_state);

    let snapshots = engine.gateway_health_snapshots().await;
    let slack = snapshots
        .iter()
        .find(|snapshot| snapshot.platform == "slack")
        .expect("slack snapshot should exist");
    assert_eq!(slack.status, zorai_protocol::GatewayConnectionStatus::Error);
    assert_eq!(slack.consecutive_failure_count, 3);
    assert_eq!(slack.last_error.as_deref(), Some("api timeout"));
    assert_eq!(slack.current_backoff_secs, 30);
}

#[tokio::test]
async fn heartbeat_checks_repo_changes_graceful_on_no_data_dir() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    let result = engine.check_repo_changes().await;

    assert_eq!(result.check_type, HeartbeatCheckType::RepoChanges);
}

#[tokio::test]
async fn heartbeat_checks_independent() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;

    let stale = engine.check_stale_todos(24).await;
    let stuck = engine.check_stuck_goal_runs(2).await;
    let unreplied = engine.check_unreplied_messages(1).await;
    let repo = engine.check_repo_changes().await;

    assert_eq!(stale.check_type, HeartbeatCheckType::StaleTodos);
    assert_eq!(stuck.check_type, HeartbeatCheckType::StuckGoalRuns);
    assert_eq!(
        unreplied.check_type,
        HeartbeatCheckType::UnrepliedGatewayMessages
    );
    assert_eq!(repo.check_type, HeartbeatCheckType::RepoChanges);
}

fn oauth_plugin_manifest(name: &str, token_url: &str) -> String {
    serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "schema_version": 1,
        "auth": {
            "type": "oauth2",
            "authorization_url": "https://example.com/oauth/authorize",
            "token_url": token_url,
            "scopes": ["scope.read"],
            "pkce": true
        }
    })
    .to_string()
}

async fn attach_plugin_manager_with_oauth_plugin(
    engine: &Arc<AgentEngine>,
    plugin_name: &str,
    manifest_json: &str,
    expired: bool,
    with_refresh_token: bool,
) {
    let root = tempfile::TempDir::new().expect("tempdir should succeed");
    let plugins_dir = root.path().join("plugins");
    let plugin_dir = plugins_dir.join(plugin_name);
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir should be created");
    std::fs::write(plugin_dir.join("plugin.json"), manifest_json).expect("manifest should write");

    let history = Arc::new(
        crate::history::HistoryStore::new_test_store(root.path())
            .await
            .expect("test history store should initialize"),
    );
    let plugin_manager = Arc::new(crate::plugin::PluginManager::new(
        history.clone(),
        plugins_dir,
    ));
    let (loaded, skipped) = plugin_manager.load_all_from_disk().await;
    assert_eq!(loaded, 1);
    assert_eq!(skipped, 0);

    let expires_at = if expired {
        (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339()
    } else {
        (chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339()
    };
    let created_at = chrono::Utc::now().to_rfc3339();
    history
        .conn
        .call({
            let plugin_name = plugin_name.to_string();
            let expires_at = expires_at.clone();
            let created_at = created_at.clone();
            move |conn| {
                conn.execute(
                    "INSERT INTO plugin_credentials (plugin_name, credential_type, encrypted_value, expires_at, created_at, updated_at)
                     VALUES (?1, 'access_token', ?2, ?3, ?4, ?4)",
                    rusqlite::params![plugin_name, vec![1_u8, 2, 3], expires_at, created_at],
                )?;
                Ok(())
            }
        })
        .await
        .expect("access token should insert");

    if with_refresh_token {
        let created_at = chrono::Utc::now().to_rfc3339();
        history
            .conn
            .call({
                let plugin_name = plugin_name.to_string();
                move |conn| {
                    conn.execute(
                        "INSERT INTO plugin_credentials (plugin_name, credential_type, encrypted_value, expires_at, created_at, updated_at)
                         VALUES (?1, 'refresh_token', ?2, NULL, ?3, ?3)",
                        rusqlite::params![plugin_name, vec![9_u8, 9, 9], created_at],
                    )?;
                    Ok(())
                }
            })
            .await
            .expect("refresh token should insert");
    }

    engine
        .plugin_manager
        .set(plugin_manager)
        .unwrap_or_else(|_| panic!("plugin manager should set once"));
}

#[tokio::test]
async fn heartbeat_checks_plugin_auth_detects_plugins_that_need_reconnect() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    attach_plugin_manager_with_oauth_plugin(
        &engine,
        "needs-reconnect-plugin",
        &oauth_plugin_manifest("needs-reconnect-plugin", "https://example.com/oauth/token"),
        true,
        false,
    )
    .await;

    let result = engine.check_plugin_auth().await;

    assert_eq!(result.check_type, HeartbeatCheckType::PluginAuth);
    assert_eq!(result.items_found, 1);
    assert!(result.summary.contains("need reconnect"));
    assert!(result.details[0].context.contains("Reconnect"));
}

#[tokio::test]
async fn heartbeat_checks_plugin_auth_reports_failed_auto_refresh_attempts() {
    let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
    attach_plugin_manager_with_oauth_plugin(
        &engine,
        "refreshable-plugin",
        &oauth_plugin_manifest("refreshable-plugin", "https://example.com/oauth/token"),
        true,
        true,
    )
    .await;

    let result = engine.check_plugin_auth().await;

    assert_eq!(result.check_type, HeartbeatCheckType::PluginAuth);
    assert_eq!(result.items_found, 1);
    assert!(result.summary.contains("refresh failed"));
    assert!(result.details[0].context.contains("auto-refresh"));
}
