use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::Duration;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

async fn spawn_recording_openai_server(recorded_bodies: Arc<StdMutex<VecDeque<String>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind recording openai server");
    let addr = listener.local_addr().expect("recording server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request from test client");
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push_back(body);

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: text/event-stream\r\n",
                    "cache-control: no-cache\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "data: {\"choices\":[{\"delta\":{\"content\":\"merged fact\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3},\"content\":\"\"}\n\n",
                    "data: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::with_capacity(65536);
    let mut temp = [0u8; 4096];
    let headers_end = loop {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            return Ok(String::new());
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, ':');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    while buffer.len().saturating_sub(headers_end) < content_length {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
    }

    let available = buffer.len().saturating_sub(headers_end).min(content_length);
    Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
}

#[test]
fn idle_returns_true_when_all_conditions_met() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        Some(1000),
        1000 + DEFAULT_IDLE_THRESHOLD_MS,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_task() {
    assert!(!is_idle_for_consolidation(
        1,
        0,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_goal_run() {
    assert!(!is_idle_for_consolidation(
        0,
        1,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_stream() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        1,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_recent_presence() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        0,
        Some(10_000),
        10_001,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_true_when_no_presence_recorded() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        None,
        1000,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[tokio::test]
async fn maybe_run_consolidation_if_idle_blocks_when_goal_run_is_awaiting_approval() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.consolidation.enabled = true;
    config.consolidation.idle_threshold_secs = 0;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal = GoalRun {
        id: "goal-awaiting-approval".to_string(),
        title: "goal awaiting approval".to_string(),
        goal: "wait for operator approval".to_string(),
        client_request_id: None,
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        created_at: 0,
        updated_at: 0,
        started_at: None,
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
        max_replans: 3,
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
        awaiting_approval_id: Some("approval-1".to_string()),
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
    };
    engine.goal_runs.lock().await.push_back(goal);

    let result = engine
        .maybe_run_consolidation_if_idle(Duration::from_millis(5))
        .await;
    assert!(
        result.is_none(),
        "dream/consolidation should stay paused while a goal run is awaiting approval"
    );
}

#[tokio::test]
async fn maybe_run_consolidation_if_idle_persists_dream_note_when_strategy_learning_occurs() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.consolidation.enabled = true;
    config.consolidation.idle_threshold_secs = 0;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let weles_scope = crate::agent::agent_identity::WELES_AGENT_ID;
    crate::agent::ensure_memory_files_for_scope(engine.data_dir.as_path(), weles_scope)
        .await
        .expect("seed memory files for weles scope");
    crate::agent::ensure_memory_files_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .await
    .expect("seed memory files for main scope");
    let weles_memory_path =
        crate::agent::task_prompt::memory_paths_for_scope(engine.data_dir.as_path(), weles_scope)
            .memory_path;
    let main_memory_path = crate::agent::task_prompt::memory_paths_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .memory_path;
    tokio::fs::write(&weles_memory_path, "# Memory\n")
        .await
        .expect("shrink weles memory file for deterministic idle-learning test");
    tokio::fs::write(&main_memory_path, "# Memory\n")
        .await
        .expect("shrink main memory file for deterministic idle-learning test");
    let now = now_millis();
    let metrics_json = serde_json::json!({
        "total_duration_ms": 45_000,
        "step_count": 2,
        "success_rate": 0.5,
        "operator_revisions": 1,
        "exit_code": 1,
    })
    .to_string();

    for idx in 0..3u64 {
        let started_at = now.saturating_sub(1_000 + idx);
        let completed_at = started_at + 100;
        engine
            .history
            .insert_execution_trace(
                &format!("dream-trace-{idx}"),
                None,
                None,
                Some(&format!("task-{idx}")),
                "coding",
                "success",
                Some(0.6),
                "[\"bash_command\",\"read_file\"]",
                &metrics_json,
                45_000,
                120,
                weles_scope,
                started_at,
                completed_at,
                completed_at,
            )
            .await
            .expect("seed execution trace");
    }

    let result = engine
        .maybe_run_consolidation_if_idle(Duration::from_millis(50))
        .await
        .expect("expected idle consolidation to run");
    assert!(
        result.forge_hints_generated > 0,
        "expected existing idle strategy learning surface to generate hints"
    );
    assert!(
        result.forge_hints_auto_applied > 0,
        "expected idle consolidation to auto-apply at least one forge hint so dream persistence has source material"
    );

    let weles_content = tokio::fs::read_to_string(&weles_memory_path)
        .await
        .expect("read weles memory file after idle consolidation");
    assert!(
        weles_content.contains("[dream]"),
        "dream state should persist an auditable [dream] note in Weles scope when idle strategy learning occurs; content was: {weles_content}"
    );
    let main_content = tokio::fs::read_to_string(&main_memory_path)
        .await
        .expect("read main memory file after idle consolidation");
    assert!(
        !main_content.contains("[dream]"),
        "dream state should not persist daemon learning into main-agent memory; content was: {main_content}"
    );
}

#[tokio::test]
async fn maybe_run_consolidation_if_idle_records_forge_dream_hint_provenance() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    config.consolidation.enabled = true;
    config.consolidation.idle_threshold_secs = 0;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let weles_scope = crate::agent::agent_identity::WELES_AGENT_ID;
    crate::agent::ensure_memory_files_for_scope(engine.data_dir.as_path(), weles_scope)
        .await
        .expect("seed memory files for weles scope");
    crate::agent::ensure_memory_files_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .await
    .expect("seed memory files for main scope");
    let weles_memory_path =
        crate::agent::task_prompt::memory_paths_for_scope(engine.data_dir.as_path(), weles_scope)
            .memory_path;
    let main_memory_path = crate::agent::task_prompt::memory_paths_for_scope(
        engine.data_dir.as_path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    )
    .memory_path;
    tokio::fs::write(&weles_memory_path, "# Memory\n")
        .await
        .expect("shrink weles memory file for deterministic idle-learning test");
    tokio::fs::write(&main_memory_path, "# Memory\n")
        .await
        .expect("shrink main memory file for deterministic idle-learning test");
    let now = now_millis();
    let metrics_json = serde_json::json!({
        "total_duration_ms": 45_000,
        "step_count": 2,
        "success_rate": 0.5,
        "operator_revisions": 1,
        "exit_code": 1,
    })
    .to_string();

    for idx in 0..3u64 {
        let started_at = now.saturating_sub(1_000 + idx);
        let completed_at = started_at + 100;
        engine
            .history
            .insert_execution_trace(
                &format!("dream-trace-{idx}"),
                None,
                None,
                Some(&format!("task-{idx}")),
                "coding",
                "success",
                Some(0.6),
                "[\"bash_command\",\"read_file\"]",
                &metrics_json,
                45_000,
                120,
                weles_scope,
                started_at,
                completed_at,
                completed_at,
            )
            .await
            .expect("seed execution trace");
    }

    let result = engine
        .maybe_run_consolidation_if_idle(Duration::from_millis(50))
        .await
        .expect("expected idle consolidation to run");
    assert!(result.forge_hints_auto_applied > 0);

    let report = engine
        .history
        .provenance_report(32)
        .expect("provenance report after idle consolidation");
    let carryover = crate::agent::provenance::adaptive_carryover_provenance_summary(&report, 8);
    assert_eq!(carryover["inspection_tool"].as_str(), Some("show_dreams"));
    assert!(carryover["persisted_event_count"]
        .as_u64()
        .is_some_and(|value| value >= 1));
    assert!(carryover["forge_hint_event_count"]
        .as_u64()
        .is_some_and(|value| value >= 1));
    let recent = carryover["recent_events"]
        .as_array()
        .expect("recent adaptive carryover provenance events array");
    assert!(recent.iter().any(|entry| {
        entry["event_type"].as_str()
            == Some(crate::agent::provenance::PROVENANCE_EVENT_FORGE_DREAM_HINTS_PERSISTED)
            && entry["source_kind"].as_str() == Some("forge")
            && entry["sequence"].as_u64().is_some()
            && entry["provenance_ref"]
                .as_str()
                .is_some_and(|value| value.starts_with("provenance:"))
    }));
}

#[tokio::test]
async fn send_refinement_llm_call_uses_weles_provider_when_running_under_weles_scope() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "http://127.0.0.1:1/v1".to_string();
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            presence_penalty: None,
            base_url: spawn_recording_openai_server(recorded_bodies.clone()).await,
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some("custom-weles".to_string());
    config.builtin_sub_agents.weles.model = Some("weles-model".to_string());
    config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let config_snapshot = engine.get_config().await;

    let response = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
        async {
            engine
                .send_refinement_llm_call(&config_snapshot, "Merge these facts.")
                .await
        },
    )
    .await
    .expect("weles-scoped refinement call should succeed");

    assert!(
        response.trim().contains("merged fact"),
        "expected refinement response to include the recorded mock content, got: {response}"
    );

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded refinement requests");
    let body = recorded
        .front()
        .expect("expected refinement request to hit Weles provider");
    assert!(
        body.contains("\"model\":\"weles-model\""),
        "expected refinement request to use Weles model, body was: {body}"
    );
    assert!(
        !body.contains("svarog-model"),
        "refinement request should not fall back to main-agent model, body was: {body}"
    );
}

#[test]
fn decay_returns_half_at_half_life() {
    let now = 1_000_000_000u64;
    let half_life_ms = (DEFAULT_HALF_LIFE_HOURS * 3_600_000.0) as u64;
    let last_confirmed = now - half_life_ms;
    let confidence = compute_decay_confidence(last_confirmed, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 0.5).abs() < 0.01,
        "expected ~0.5, got {confidence}"
    );
}

#[test]
fn decay_returns_near_one_for_just_confirmed() {
    let now = 1_000_000_000u64;
    let confidence = compute_decay_confidence(now, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 1.0).abs() < 0.001,
        "expected ~1.0, got {confidence}"
    );
}

#[test]
fn decay_returns_zero_for_zero_timestamp() {
    let confidence = compute_decay_confidence(0, 1_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_returns_zero_for_nonpositive_half_life() {
    let confidence = compute_decay_confidence(500_000, 1_000_000, 0.0);
    assert_eq!(confidence, 0.0);
    let confidence = compute_decay_confidence(500_000, 1_000_000, -5.0);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_clamps_to_valid_range() {
    let c1 = compute_decay_confidence(1, 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c1));

    let c2 = compute_decay_confidence(1, u64::MAX / 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c2));
}

#[test]
fn decay_handles_very_large_age_without_panic() {
    let confidence = compute_decay_confidence(0, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);

    let confidence = compute_decay_confidence(1, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&confidence));
    assert!(confidence < 0.001);
}

#[tokio::test]
async fn dream_state_cycle_persists_cycles_evaluations_and_show_dreams_payload() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let task = engine
        .enqueue_task(
            "Review authentication diff".to_string(),
            "Compare the auth patch, review regressions, and summarize what changed.".to_string(),
            "normal",
            Some("grep auth src/auth.rs".to_string()),
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-dream-state".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    {
        let mut tasks = engine.tasks.lock().await;
        let stored = tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
            .expect("task should exist");
        stored.status = TaskStatus::Completed;
        stored.started_at = Some(now_millis().saturating_sub(30_000));
        stored.completed_at = Some(now_millis());
        stored.retry_count = 2;
    }

    engine.run_dream_state_cycle_if_idle(15 * 60 * 1000).await;

    let cycles = engine
        .history
        .list_dream_cycles(4)
        .await
        .expect("dream cycles should persist");
    assert!(!cycles.is_empty(), "dream cycle should be recorded");
    let cycle_id = cycles[0].id.expect("cycle id should be present");
    let evaluations = engine
        .history
        .list_counterfactual_evaluations(cycle_id)
        .await
        .expect("counterfactual evaluations should persist");
    assert!(
        !evaluations.is_empty(),
        "dream cycle should record counterfactual evaluations"
    );

    let payload = engine
        .show_dreams_payload(5)
        .await
        .expect("dream payload should build");
    assert!(
        payload["cycle_count"].as_u64().unwrap_or_default() >= 1,
        "show_dreams should surface recorded cycles"
    );
    assert!(
        payload["hints"]
            .as_array()
            .is_some_and(|items| !items.is_empty()),
        "show_dreams should surface persisted dream hints"
    );
    let carryover = &payload["carryover"];
    assert_eq!(carryover["inspection_tool"].as_str(), Some("show_dreams"));
    assert!(carryover["hint_count"]
        .as_u64()
        .is_some_and(|value| value >= 1));
    assert!(carryover["summary"]
        .as_str()
        .is_some_and(|value| value.contains("visible [dream] hint")));
    let recent = carryover["recent_events"]
        .as_array()
        .expect("show_dreams should surface recent carryover provenance events");
    assert!(recent.iter().any(|entry| {
        entry["event_type"].as_str()
            == Some(crate::agent::provenance::PROVENANCE_EVENT_DREAM_HINTS_PERSISTED)
            && entry["source_kind"].as_str() == Some("dream_state")
            && entry["provenance_ref"]
                .as_str()
                .is_some_and(|value| value.starts_with("provenance:"))
    }));
}
