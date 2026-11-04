use super::*;
use zorai_shared::providers::{
    PROVIDER_ID_CUSTOM, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
};

async fn spawn_pre_compaction_memory_update_server(
    recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind pre-compaction memory update server");
    let addr = listener
        .local_addr()
        .expect("pre-compaction memory update server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            tokio::spawn(async move {
                let request =
                    read_http_request(&mut socket, "pre-compaction memory update request").await;
                let body = request_body(&request);
                recorded_bodies
                    .lock()
                    .expect("lock recorded pre-compaction request log")
                    .push_back(body.clone());

                let response = if body.contains("## Pre-Compaction Memory Flush") {
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_memory_flush_1\",\"function\":{\"name\":\"update_memory\",\"arguments\":\"{\\\"target\\\":\\\"memory\\\",\\\"mode\\\":\\\"append\\\",\\\"content\\\":\\\"- Durable correction from compaction\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                } else {
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write pre-compaction memory update response");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn first_turn_runner_bootstrap_includes_structured_memory_summary() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let outcome = engine
        .send_message_inner(
            None,
            "Bootstrap me with memory context",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should succeed");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant request log");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Persistent Memory File Paths")),
        "expected first-turn prompt bootstrap to carry the base memory layer"
    );
    assert!(
        !recorded
            .iter()
            .any(|body| body.contains("## Structured Memory Summary")),
        "the base memory layers are already inline; duplicating them as a summary wastes tokens"
    );
}

#[tokio::test]
async fn post_compaction_prompt_rebuild_refreshes_memory_summary_and_injection_state() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_pre_compaction_memory_update_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-memory-bootstrap-rebuild";

    let memory_paths = crate::agent::task_prompt::memory_paths_for_scope(
        root.path(),
        crate::agent::agent_identity::MAIN_AGENT_ID,
    );
    std::fs::write(
        &memory_paths.memory_path,
        "# Memory\n\n- Stable fact before compaction\n",
    )
    .expect("seed memory file");
    engine.refresh_memory_cache().await;

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Compaction memory rebuild".to_string(),
                messages: vec![
                    crate::agent::types::AgentMessage::user("First request", 1),
                    crate::agent::types::AgentMessage {
                        id: "assistant-1".to_string(),
                        role: MessageRole::Assistant,
                        content: "Observed earlier state".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        cost: None,
                        provider: None,
                        model: None,
                        api_transport: None,
                        response_id: None,
                        upstream_message: None,
                        provider_final_result: None,
                        author_agent_id: None,
                        author_agent_name: None,
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: 2,
                        feedback: None,
                    },
                    crate::agent::types::AgentMessage::user("Need a fresh request boundary", 3),
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Need a fresh request boundary",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should succeed");

    assert!(!outcome.interrupted_for_approval);

    let injection_state = engine
        .get_thread_memory_injection_state(thread_id)
        .await
        .expect("expected thread injection state after rebuild");
    assert!(
        injection_state.base_markdown_injected_at_ms.is_some(),
        "expected rebuild to refresh prompt memory injection state"
    );

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded pre-compaction request log");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Pre-Compaction Memory Flush")),
        "expected the rebuild path to execute the pre-compaction memory flush request"
    );
    assert!(
        !recorded
            .iter()
            .any(|body| body.contains("## Structured Memory Summary")),
        "post-compaction rebuild carries the base memory layers inline; no duplicate summary"
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("Durable correction from compaction")),
        "expected rebuilt prompt summary to reflect refreshed durable memory"
    );
}

#[tokio::test]
async fn send_message_request_includes_runtime_continuity_and_negative_knowledge() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-runtime-continuity-request";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Runtime continuity thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Investigate the failure",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(crate::agent::types::AgentTask {
            id: "task-runtime-continuity-request".to_string(),
            title: "Investigate failure".to_string(),
            description: "Inspect the failing path".to_string(),
            status: TaskStatus::InProgress,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-runtime-1".to_string()),
            goal_run_title: Some("Test goal".to_string()),
            goal_step_id: Some("step-runtime-1".to_string()),
            goal_step_title: Some("Investigate failure".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 1,
            max_retries: 2,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            completion_contract: None,
            tool_whitelist: None,
            tool_blacklist: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        });
    }

    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.push_back(crate::agent::types::GoalRun {
            id: "goal-runtime-1".to_string(),
            title: "Test goal".to_string(),
            goal: "Recover from repeated failure".to_string(),
            client_request_id: None,
            status: crate::agent::types::GoalRunStatus::Running,
            priority: crate::agent::types::TaskPriority::Normal,
            created_at: 1,
            updated_at: 1,
            started_at: Some(1),
            completed_at: None,
            thread_id: Some(thread_id.to_string()),
            root_thread_id: Some(thread_id.to_string()),
            supervision_thread_id: None,
            active_thread_id: Some(thread_id.to_string()),
            execution_thread_ids: vec![thread_id.to_string()],
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("Investigate failure".to_string()),
            current_step_kind: Some(crate::agent::types::GoalRunStepKind::Research),
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
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
            child_task_ids: vec!["task-runtime-continuity-request".to_string()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: Some("task-runtime-continuity-request".to_string()),
            pending_review_report: None,
            duration_ms: None,
            steps: vec![crate::agent::types::GoalRunStep {
                id: "step-runtime-1".to_string(),
                position: 0,
                title: "Investigate failure".to_string(),
                instructions: "Inspect the failing path".to_string(),
                kind: crate::agent::types::GoalRunStepKind::Research,
                success_criteria: "Know why it failed".to_string(),
                session_id: None,
                status: crate::agent::types::GoalRunStepStatus::InProgress,
                task_id: Some("task-runtime-continuity-request".to_string()),
                summary: None,
                error: None,
                started_at: Some(1),
                completed_at: None,
            }],
            events: Vec::new(),
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: Default::default(),
            authorship_tag: None,
        });
    }

    {
        let mut stores = engine.episodic_store.write().await;
        let store = stores
            .entry(crate::agent::agent_identity::MAIN_AGENT_ID.to_string())
            .or_default();
        store.counter_who.current_focus = Some("Tool: read_file".to_string());
        store.counter_who.correction_patterns = vec![crate::agent::episodic::CorrectionPattern {
            pattern: "Inspect workspace state before retrying".to_string(),
            correction_count: 2,
            last_correction_at: 3,
        }];
    }
    engine
        .add_negative_constraint(crate::agent::episodic::NegativeConstraint {
            id: "nk-runtime-test-1".to_string(),
            episode_id: None,
            constraint_type: crate::agent::episodic::ConstraintType::RuledOut,
            subject: "Investigate failure".to_string(),
            solution_class: Some("recovery".to_string()),
            description: "The old recovery path already failed twice.".to_string(),
            confidence: 0.95,
            state: crate::agent::episodic::ConstraintState::Dead,
            evidence_count: 2,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: vec!["investigate".to_string(), "failure".to_string()],
            valid_until: Some(crate::agent::now_millis() + 60_000),
            created_at: crate::agent::now_millis(),
        })
        .await
        .expect("seed negative knowledge for runtime prompt");

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate the failure",
            None,
            Some("task-runtime-continuity-request"),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Working Continuity")),
        "expected the execution prompt to include the continuity summary section",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("I am carrying this forward as")
                && body.contains(MAIN_AGENT_NAME)
                && body.contains("Test goal")
                && body.contains("Investigate failure")
                && body.contains("I am continuing the same workstream")),
        "expected the execution prompt to include active persona identity plus explicit goal, step, and task titles",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Ruled-Out Approaches (Negative Knowledge)")),
        "expected the execution prompt to include ruled-out approaches",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("The old recovery path already failed twice.")),
        "expected the execution prompt to include matching negative knowledge",
    );
}

#[tokio::test]
async fn send_message_request_includes_semantic_memory_palace_context_when_graph_exists() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-memory-palace-prompt";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Memory palace prompt thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Investigate authentication regression",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine.thread_structural_memories.write().await.insert(
        thread_id.to_string(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![],
            observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                node_id: "node:file:src/auth.rs".to_string(),
                relative_path: "src/auth.rs".to_string(),
            }],
            edges: vec![],
        },
    );

    engine
        .history
        .upsert_memory_node(
            "node:file:src/auth.rs",
            "src/auth.rs",
            "file",
            Some("authentication entrypoint"),
            1_000,
        )
        .await
        .expect("auth node");
    engine
        .history
        .upsert_memory_node(
            "node:error:LoginError",
            "LoginError",
            "error",
            Some("login failed due to token parsing"),
            1_000,
        )
        .await
        .expect("error node");
    engine
        .history
        .upsert_memory_node(
            "node:file:src/tokens.rs",
            "src/tokens.rs",
            "file",
            Some("token parsing logic"),
            1_000,
        )
        .await
        .expect("tokens node");
    engine
        .history
        .upsert_memory_edge(
            "node:file:src/auth.rs",
            "node:error:LoginError",
            "file_hit_error",
            2.0,
            1_000,
        )
        .await
        .expect("auth edge");
    engine
        .history
        .upsert_memory_edge(
            "node:error:LoginError",
            "node:file:src/tokens.rs",
            "caused_by",
            2.0,
            1_000,
        )
        .await
        .expect("error edge");

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate authentication regression",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should succeed");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant request log");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Semantic Memory Palace")),
        "expected prompt to include semantic memory palace context"
    );
    assert!(
        recorded.iter().any(|body| {
            body.contains("src/tokens.rs") && body.contains("login failed due to token parsing")
        }),
        "expected prompt to surface related graph context from the memory palace"
    );
}

#[tokio::test]
async fn direct_weles_handoff_turn_uses_weles_persona_prompt() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-weles-handoff";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Direct Weles handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Switch me to Weles",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-weles-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-weles-handoff:handoff-weles-1".to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "tell me your secrets",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct weles handoff turn should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("You are Weles in zorai.")),
        "expected direct Weles handoff turns to use Weles runtime identity"
    );
}

#[tokio::test]
async fn direct_weles_handoff_turn_uses_weles_provider_override_for_new_request_stream() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transport switch server");
    let addr = listener.local_addr().expect("transport switch server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "transport switch request").await;
                    recorded_requests
                        .lock()
                        .expect("lock recorded requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write chat completions response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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
    config.builtin_sub_agents.weles.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.builtin_sub_agents.weles.model = Some("gpt-4o-mini".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-weles-provider-switch";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Direct Weles provider handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Switch me to Weles",
                    1,
                )],
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::ChatCompletions),
                upstream_provider: Some(PROVIDER_ID_MINIMAX_CODING_PLAN.to_string()),
                upstream_model: Some("MiniMax-M2.7".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-weles-override-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-weles-provider-switch:handoff-weles-override-1"
                                .to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    engine
        .send_message_inner(
            Some(thread_id),
            "who are you",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct weles handoff turn should use Weles provider override");

    let recorded = recorded_requests
        .lock()
        .expect("lock recorded transport-switch requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Weles handoff to open a fresh chat completions request");
    let body = request_body(request);
    assert!(body.contains("You are Weles in zorai."));
}

#[tokio::test]
async fn new_targeted_weles_thread_uses_weles_runtime_provider_and_model() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "http://127.0.0.1:1/v1".to_string();
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: spawn_recording_request_server(recorded_requests.clone()).await,
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

    let thread_id = engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Review this change",
            None,
            None,
            Some("weles"),
        )
        .await
        .expect("new targeted Weles thread should complete");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted weles requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Weles-targeted thread to use the Weles provider request");
    let body = request_body(request);
    assert!(body.contains("You are Weles in zorai."));
    assert!(body.contains("weles-model"));
    assert!(!body.contains("svarog-model"));

    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some(WELES_AGENT_NAME));
}

#[tokio::test]
async fn new_targeted_custom_subagent_thread_persists_assistant_author_identity() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_request_server(recorded_requests).await;
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.sub_agents.push(SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "coder".to_string(),
        name: "coder".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "coder-model".to_string(),
        role: Some("coding specialist".to_string()),
        system_prompt: Some("Answer as coder.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Who are you?",
            None,
            None,
            Some("coder"),
        )
        .await
        .expect("new targeted custom subagent thread should complete");

    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("coder"));
    let assistant = thread
        .messages
        .iter()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("assistant reply should be persisted");
    assert_eq!(assistant.author_agent_id.as_deref(), Some("coder"));
    assert_eq!(assistant.author_agent_name.as_deref(), Some("coder"));
}

#[tokio::test]
async fn new_targeted_rarog_thread_uses_concierge_runtime_provider_and_model() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind new targeted rarog server");
    let addr = listener
        .local_addr()
        .expect("new targeted rarog server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request =
                        read_http_request(&mut socket, "new targeted rarog request").await;
                    recorded_requests
                        .lock()
                        .expect("lock targeted rarog requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write targeted rarog response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write targeted rarog 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "rarog-model".to_string(),
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
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("rarog-model".to_string());
    config.concierge.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Triage this issue",
            None,
            None,
            Some("rarog"),
        )
        .await
        .expect("new targeted Rarog thread should complete");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted rarog requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Rarog-targeted thread to use the concierge provider request");
    let body = request_body(request);
    let body_json: serde_json::Value =
        serde_json::from_str(&body).expect("recorded request body should be valid json");
    assert!(body.contains("You are the zorai concierge"));
    assert_eq!(
        body_json.get("model").and_then(|value| value.as_str()),
        Some("rarog-model")
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some(CONCIERGE_AGENT_NAME));
}

#[tokio::test]
async fn new_targeted_rarog_thread_prefers_concierge_model_override_over_stored_provider_model() {
    let recorded_requests = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind targeted rarog override server");
    let addr = listener
        .local_addr()
        .expect("targeted rarog override server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request =
                    read_http_request(&mut socket, "targeted rarog override request").await;
                recorded_requests
                    .lock()
                    .expect("lock targeted rarog override requests")
                    .push(request.clone());

                if request.starts_with("POST /v1/chat/completions") {
                    let response = concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write targeted rarog override response");
                } else {
                    socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                        .await
                        .expect("write targeted rarog override 404 response");
                }
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "MiniMax-M2.5".to_string(),
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
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("qwen3.6-plus".to_string());
    config.concierge.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Triage this issue",
            None,
            None,
            Some("rarog"),
        )
        .await
        .expect("new targeted Rarog thread should honor concierge model override");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted rarog override requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected targeted Rarog thread to hit concierge provider request");
    let body = request_body(request);
    let body_json: serde_json::Value =
        serde_json::from_str(&body).expect("recorded request body should be valid json");
    assert_eq!(
        body_json.get("model").and_then(|value| value.as_str()),
        Some("qwen3.6-plus"),
        "targeted Rarog request should carry concierge.model override"
    );
}

#[tokio::test]
async fn operator_send_rejects_budget_exceeded_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "thread-budget".to_string(),
        AgentThread {
            id: "thread-budget".to_string(),
            agent_name: Some("Dazhbog".to_string()),
            title: "Budget exceeded".to_string(),
            messages: vec![AgentMessage::user("Do the work", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );
    engine.tasks.lock().await.push_back(AgentTask {
        id: "task-budget".to_string(),
        title: "Budget exceeded child".to_string(),
        description: "Child exhausted its budget".to_string(),
        status: TaskStatus::BudgetExceeded,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 1,
        started_at: Some(1),
        completed_at: Some(2),
        error: Some("execution budget exceeded for this thread".to_string()),
        result: None,
        thread_id: Some("thread-budget".to_string()),
        source: "subagent".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: Some("task-parent".to_string()),
        parent_thread_id: Some("thread-parent".to_string()),
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("execution budget exceeded for this thread".to_string()),
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: Some("execution budget exceeded for this thread".to_string()),
        logs: Vec::new(),
        completion_contract: None,
        tool_whitelist: None,
        tool_blacklist: None,
        override_provider: None,
        override_model: None,
        override_api_transport: None,
        override_system_prompt: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        sub_agent_def_id: None,
    });
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let error = engine
        .send_message_with_session_and_surface(Some("thread-budget"), None, "continue", None, None)
        .await
        .expect_err("budget exceeded thread should reject operator send");

    assert!(
        error.to_string().contains("locked because task"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn successful_handoff_tool_call_restarts_same_turn_under_requested_agent() {
    let request_counter = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind handoff stop server");
    let addr = listener.local_addr().expect("handoff stop server addr");

    tokio::spawn({
        let request_counter = request_counter.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request_counter = request_counter.clone();
                tokio::spawn(async move {
                    let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                    let _request = read_http_request(&mut socket, "handoff stop request").await;
                    let response = if attempt == 0 {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_handoff_1\",\"function\":{\"name\":\"handoff_thread_agent\",\"arguments\":\"{\\\"action\\\":\\\"push_handoff\\\",\\\"target_agent_id\\\":\\\"rarog\\\",\\\"reason\\\":\\\"Operator explicitly requested Rarog\\\",\\\"summary\\\":\\\"Switch control to Rarog\\\",\\\"requested_by\\\":\\\"user\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"I'm Rarog.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    };
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write handoff stop response");
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-handoff-stops-turn";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Handoff stop thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("gimme Rarog", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .send_message_inner(
            Some(thread_id),
            "gimme Rarog",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("handoff should restart the same operator turn under Rarog");

    assert_eq!(request_counter.load(Ordering::SeqCst), 2);

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.content == "I'm Rarog."));
}

#[tokio::test]
async fn successful_handoff_restarts_same_turn_under_requested_agent_with_summary() {
    let request_counter = Arc::new(AtomicUsize::new(0));
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind handoff restart server");
    let addr = listener.local_addr().expect("handoff restart server addr");

    tokio::spawn({
        let request_counter = request_counter.clone();
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request_counter = request_counter.clone();
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                    let request = read_http_request(&mut socket, "handoff restart request").await;
                    recorded_requests
                        .lock()
                        .expect("lock recorded requests")
                        .push_back(request.clone());

                    let response = if attempt == 0 {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_handoff_1\",\"function\":{\"name\":\"handoff_thread_agent\",\"arguments\":\"{\\\"action\\\":\\\"push_handoff\\\",\\\"target_agent_id\\\":\\\"weles\\\",\\\"reason\\\":\\\"Operator explicitly requested Weles\\\",\\\"summary\\\":\\\"Switch control to Weles and continue helping from governance scope\\\",\\\"requested_by\\\":\\\"user\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"I'm Weles.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    };

                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write handoff restart response");
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "custom-main".to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "main-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;
    config.providers.insert(
        "custom-main".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "main-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-handoff-restart-same-turn";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Handoff restart thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("give me Weles", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .send_message_inner(
            Some(thread_id),
            "give me Weles",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("handoff should restart the same operator turn under Weles");

    assert_eq!(request_counter.load(Ordering::SeqCst), 2);

    let recorded = recorded_requests
        .lock()
        .expect("lock handoff restart requests");
    let second_request = recorded
        .get(1)
        .expect("second request should exist after handoff restart");
    let second_body = request_body(second_request);
    assert!(second_body.contains("You are Weles in zorai."));
    assert!(second_body.contains("weles-model"));
    assert!(second_body.contains("User requested you while talking to Svarog"));
    assert!(
        second_body.contains("Switch control to Weles and continue helping from governance scope")
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.content == "I'm Weles."));
}

#[tokio::test]
async fn direct_rarog_handoff_turn_uses_real_concierge_runtime_config() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind concierge runtime server");
    let addr = listener
        .local_addr()
        .expect("concierge runtime server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "concierge runtime request").await;
                    recorded_requests
                        .lock()
                        .expect("lock concierge requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write concierge response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("gpt-4o-mini".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-rarog-provider-switch";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::CONCIERGE_AGENT_NAME.to_string()),
                title: "Direct Rarog provider handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("gimme Rarog", 1)],
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::ChatCompletions),
                upstream_provider: Some(PROVIDER_ID_MINIMAX_CODING_PLAN.to_string()),
                upstream_model: Some("MiniMax-M2.7".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-rarog-override-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-rarog-provider-switch:handoff-rarog-override-1"
                                .to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    engine
        .send_message_inner(
            Some(thread_id),
            "who are you",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct rarog handoff turn should use concierge runtime");

    let recorded = recorded_requests
        .lock()
        .expect("lock concierge runtime requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected concierge handoff to open a fresh chat completions request");
    let body = request_body(request);
    assert!(body.contains("You are the zorai concierge"));
    assert!(body.contains("Rarog"));
}

#[tokio::test]
async fn transport_incompatibility_does_not_mutate_persisted_config_and_emits_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
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

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.persist_config().await;
    let mut events = engine.subscribe();

    let _error = match engine
        .send_message_inner(
            None, "hello", None, None, None, None, None, None, None, true,
        )
        .await
    {
        Ok(_) => panic!("transport incompatibility should fail the turn"),
        Err(error) => error,
    };

    let stored_config = engine.config.read().await.clone();
    assert_eq!(stored_config.api_transport, ApiTransport::Responses);
    assert_eq!(
        stored_config
            .providers
            .get("custom")
            .expect("provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("list persisted config items");
    let persisted = crate::agent::config::load_config_from_items(persisted_items)
        .expect("decode persisted config");
    assert_eq!(persisted.api_transport, ApiTransport::Responses);
    assert_eq!(
        persisted
            .providers
            .get("custom")
            .expect("persisted provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_notice = false;
    let mut saw_done = false;
    let mut seen_notice_kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            AgentEvent::WorkflowNotice {
                kind,
                message,
                details,
                ..
            } => {
                seen_notice_kinds.push(kind.clone());
                if kind == "transport-incompatible" || kind == "upstream-error" {
                    saw_notice = true;
                    assert!(
                        message.contains("incompatible")
                            || details
                                .as_deref()
                                .is_some_and(|d| d.contains("transport_incompatible"))
                    );
                    let details = details.expect("notice should include diagnostics");
                    assert!(details.contains("transport_incompatible"));
                    assert!(details.contains("Responses API not supported"));
                }
            }
            AgentEvent::Done { .. } => saw_done = true,
            _ => {}
        }
    }

    assert!(
        saw_notice,
        "expected operator-visible transport incompatibility notice, saw {:?}",
        seen_notice_kinds
    );
    assert!(
        saw_done,
        "expected turn completion event for surfaced error"
    );
    assert_eq!(request_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn auto_retry_wait_escalates_to_fresh_runner_after_repeated_waits() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let success_request_started = Arc::new(tokio::sync::Notify::new());
    let release_success_request = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transient_failures_then_blocking_success_server(
        request_counter.clone(),
        2,
        success_request_started.clone(),
        release_success_request.clone(),
    )
    .await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 5;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-auto-retry-loop";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Auto retry loop".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let initial_generation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if request_counter.load(Ordering::SeqCst) >= 1 {
                let streams = engine.stream_cancellations.lock().await;
                if let Some(entry) = streams.get(thread_id) {
                    break entry.generation;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("initial stream generation should be registered");

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        success_request_started.notified(),
    )
    .await
    .expect("fresh recovery request should start after repeated waits");

    let refreshed_generation = {
        let streams = engine.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .map(|entry| entry.generation)
            .expect("fresh recovery stream should replace the active stream entry")
    };
    assert!(
        refreshed_generation > initial_generation,
        "repeated auto-retry waits should replace the broken stream with a fresh runner"
    );

    release_success_request.notify_waiters();

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), task)
        .await
        .expect("fresh recovery loop should finish");
    let joined = result.expect("join send task");
    let outcome = joined.expect("fresh recovery loop should succeed");
    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(request_counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn structured_upstream_diagnostics_are_not_persisted_or_streamed_to_user() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let error = match engine
        .send_message_inner(
            None, "hello", None, None, None, None, None, None, None, true,
        )
        .await
    {
        Ok(_) => panic!("structured upstream failure should fail the turn"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "precondition: returned error still carries structured diagnostics envelope"
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_error_delta = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Delta { content, .. } = event {
            if content.starts_with("Error: ") {
                saw_error_delta = true;
                assert!(
                    !content.contains(UPSTREAM_DIAGNOSTICS_MARKER),
                    "error delta should not expose structured diagnostics"
                );
            }
        }
    }
    assert!(saw_error_delta, "expected streamed error delta");

    let threads = engine.threads.read().await;
    let thread = threads.values().next().expect("thread should be created");
    let assistant_error = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant && message.content.starts_with("Error: ")
        })
        .expect("assistant error should be persisted");
    assert!(
        !assistant_error
            .content
            .contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "persisted assistant error should not include structured diagnostics"
    );
}

#[tokio::test]
async fn retry_stream_now_resumes_active_waiting_stream() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let release_second_request = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transient_failure_then_blocking_server(
        request_counter.clone(),
        release_second_request.clone(),
    )
    .await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 10_000;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-retry-now-refreshes-stream";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Retry refresh".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let mut events = engine.subscribe();
    let mut send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let waiting_generation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::RetryStatus {
                    thread_id: event_thread_id,
                    phase,
                    ..
                }) if event_thread_id == thread_id && phase == "waiting" => {
                    let streams = engine.stream_cancellations.lock().await;
                    break streams
                        .get(thread_id)
                        .map(|entry| entry.generation)
                        .expect("waiting retry stream should be registered");
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
    })
    .await
    .expect("retry waiting status should appear");

    assert!(
        engine.retry_stream_now(thread_id).await,
        "retry-now should resume the active waiting stream"
    );

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while request_counter.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("retry-now should perform a second request");

    let resumed_generation = {
        let streams = engine.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .map(|entry| entry.generation)
            .expect("waiting retry stream should remain registered")
    };
    assert_eq!(
        resumed_generation, waiting_generation,
        "retry-now should resume the same active stream instead of replacing it"
    );

    release_second_request.notify_waiters();
    assert!(
        engine.stop_stream(thread_id).await,
        "resumed stream should remain cancellable until the operator stops it"
    );
    let _ = send_task.await;
}

#[tokio::test]
async fn anthropic_transport_retry_restarts_with_fresh_runner_state() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = spawn_anthropic_rebuild_sensitive_retry_server(recorded_bodies.clone()).await;
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 1;
    config.retry_delay_ms = 200;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-fresh-retry";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic fresh retry".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if recorded_bodies
                .lock()
                .expect("lock recorded anthropic bodies")
                .len()
                >= 1
            {
                let mut threads = engine.threads.write().await;
                let thread = threads.get_mut(thread_id).expect("thread should exist");
                let last_user = thread
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .expect("user message should exist");
                last_user.content = "hello again".to_string();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first anthropic request should be recorded");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), send_task)
        .await
        .expect("send task should complete")
        .expect("send task join should succeed")
        .expect("fresh runner retry should succeed");

    assert_eq!(outcome.thread_id, thread_id);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded anthropic bodies");
    assert!(
        recorded.len() >= 2,
        "expected at least two anthropic requests, saw {recorded:?}"
    );
    assert!(
        recorded[0].contains("\"hello\""),
        "expected the initial anthropic request to use the original thread content: {}",
        recorded[0]
    );
    assert!(
        recorded[1].contains("hello again"),
        "expected the retried anthropic request to rebuild from fresh thread state: {}",
        recorded[1]
    );
}

#[tokio::test]
async fn anthropic_outer_auto_retry_restarts_with_fresh_runner_state() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_anthropic_rebuild_sensitive_retry_server(recorded_bodies.clone()).await;
    config.model = "claude-test".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 200;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-outer-fresh-retry";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic outer fresh retry".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if recorded_bodies
                .lock()
                .expect("lock recorded anthropic bodies")
                .len()
                >= 1
            {
                let mut threads = engine.threads.write().await;
                let thread = threads.get_mut(thread_id).expect("thread should exist");
                let last_user = thread
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .expect("user message should exist");
                last_user.content = "hello again".to_string();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first anthropic request should be recorded");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), send_task)
        .await
        .expect("send task should complete")
        .expect("send task join should succeed")
        .expect("outer fresh runner retry should succeed");

    assert_eq!(outcome.thread_id, thread_id);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded anthropic bodies");
    assert!(
        recorded.len() >= 2,
        "expected at least two anthropic requests, saw {recorded:?}"
    );
    assert!(
        recorded[0].contains("\"hello\""),
        "expected the initial anthropic request to use the original thread content: {}",
        recorded[0]
    );
    assert!(
        recorded[1].contains("hello again"),
        "expected the outer auto-retry anthropic request to rebuild from fresh thread state: {}",
        recorded[1]
    );
}

#[tokio::test]
async fn concierge_recovery_fixable_request_invalid_starts_one_background_investigation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary: "provider rejected the daemon request as invalid: Invalid 'input[12].name': empty string".to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "Invalid 'input[12].name': empty string"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let first = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");
    let second = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("repeat recovery evaluation should succeed");

    assert!(first.started_investigation);
    assert!(first.retry_attempted);
    assert_eq!(
        first.signature.as_deref(),
        Some("request-invalid-empty-tool-name")
    );
    assert!(!second.started_investigation);
    assert!(!second.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-recovery")
    );
}

#[tokio::test]
async fn concierge_recovery_copilot_missing_tool_call_output_signature_retries() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary: "github-copilot rejected the daemon request as invalid: No tool call found for function call output with call_id call_function_9d83w4vhn3me_1.".to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "No tool call found for function call output with call_id call_function_9d83w4vhn3me_1."
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-missing-tool-call",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(disposition.started_investigation);
    assert!(disposition.retry_attempted);
    assert_eq!(
        disposition.signature.as_deref(),
        Some("request-invalid-stale-continuation")
    );

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-missing-tool-call")
    );
}

#[tokio::test]
async fn concierge_recovery_transport_signature_is_blocked_after_committed_output() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "transport_incompatible".to_string(),
        summary: "The selected provider/transport combination is incompatible: request body mismatch from stale thread state".to_string(),
        diagnostics: serde_json::json!({
            "details": "request body mismatch from stale thread state"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-visible-output",
            &structured,
            true,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(disposition.started_investigation);
    assert!(!disposition.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
}

#[tokio::test]
async fn strong_match_recommends_skill_before_non_discovery_tool_without_blocking() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures. Choose it when the task is to debug panic in rust service incidents.\n",
    )
    .expect("write skill");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = false;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let discovery = engine
        .discover_skill_recommendations_public("debug panic in rust service", None, 3, None)
        .await
        .expect("skill discovery should succeed");
    assert_eq!(discovery.confidence_tier, "strong");
    assert!(
        discovery
            .recommended_action
            .contains("read_skill systematic-debugging"),
        "expected discovery to require reading the matched skill first: {}",
        discovery.recommended_action
    );

    let thread_id = "thread-strong-skill-gate";
    let mut events = engine.subscribe();

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");
    assert!(!outcome.interrupted_for_approval);

    let (saw_gate_notice, saw_tool_result) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            let mut saw_gate_notice = false;
            let mut saw_tool_result = false;
            while !saw_gate_notice || !saw_tool_result {
                match events.recv().await {
                    Ok(AgentEvent::WorkflowNotice {
                        thread_id: event_thread_id,
                        kind,
                        message,
                        ..
                    }) if event_thread_id == thread_id && kind == "skill-gate" => {
                        if message.contains("read_skill systematic-debugging")
                            && message.contains("allowing the tool call to proceed")
                        {
                            saw_gate_notice = true;
                        }
                    }
                    Ok(AgentEvent::ToolResult {
                        thread_id: event_thread_id,
                        name,
                        content,
                        is_error,
                        ..
                    }) if event_thread_id == thread_id && name == "read_file" => {
                        saw_tool_result = true;
                        assert!(
                            !is_error,
                            "recommended skill reads should not block normal tool execution"
                        );
                        assert!(
                            content.contains("allowed through"),
                            "expected read_file to run successfully after the advisory notice: {content}"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => panic!("workflow event stream closed unexpectedly: {error}"),
                }
            }
            (saw_gate_notice, saw_tool_result)
        },
    )
    .await
    .expect("expected skill gate notice and read_file result");
    assert!(saw_gate_notice, "expected a skill gate workflow notice");
    assert!(
        saw_tool_result,
        "expected read_file to proceed after the advisory notice"
    );

    let metadata = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let persisted = engine
                .history
                .get_thread(thread_id)
                .await
                .expect("read persisted thread")
                .expect("thread should persist");
            let metadata = persisted.metadata_json.expect("thread metadata");
            if metadata.contains("\"recommended_skill\":\"systematic-debugging\"") {
                return metadata;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected persisted metadata to include the resolved skill recommendation");
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn weak_match_allows_progress_without_skip_rationale() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("debugging-playbook");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Debugging playbook\nUse this skill for generic debug investigations.\n",
    )
    .expect("write skill");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.80;
    config.skill_recommendation.weak_match_threshold = 0.30;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-weak-skill-gate";

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("initial send should complete");

    let first_read_attempt = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should exist")
            .messages
            .iter()
            .rev()
            .find(|message| {
                message.role == MessageRole::Tool
                    && message.tool_name.as_deref() == Some("read_file")
            })
            .cloned()
            .expect("initial read_file result should be recorded")
    };
    assert_eq!(first_read_attempt.tool_status.as_deref(), Some("done"));
    assert!(first_read_attempt.content.contains("allowed through"));

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let skip_message = thread.messages.iter().find(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some("justify_skill_skip")
    });
    assert!(
        skip_message.is_none(),
        "weak matches should not require justify_skill_skip before progress"
    );

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(!metadata.contains("\"skip_rationale\""));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn persisted_mesh_next_step_can_downgrade_legacy_strong_gate_to_advisory() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-persisted-mesh-next-step";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Persisted mesh gate".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "seed".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "read_skill systematic-debugging",
                        "mesh_next_step": "choose_or_bypass",
                        "mesh_requires_approval": false,
                        "read_skill_identifier": "variant-systematic-debugging-v1",
                        "skip_rationale": null,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "seed".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "hi",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist")
    };

    let read_result = thread
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Tool && message.tool_name.as_deref() == Some("read_file")
        })
        .expect("read_file result should be recorded");
    assert_eq!(read_result.tool_status.as_deref(), Some("done"));
    assert!(read_result.content.contains("allowed through"));

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"mesh_next_step\":\"choose_or_bypass\""));
    assert!(metadata.contains("\"mesh_requires_approval\":false"));
}

#[tokio::test]
async fn persisted_mesh_requires_approval_blocks_non_discovery_tool() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-persisted-mesh-approval-gate";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Persisted mesh approval gate".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "seed".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "request_approval systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": true,
                        "mesh_approval_id": "approval-required-1",
                        "read_skill_identifier": "variant-systematic-debugging-v1",
                        "skip_rationale": null,
                        "skill_read_completed": false,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "seed".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "hi",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist")
    };

    let read_result = thread
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Tool && message.tool_name.as_deref() == Some("read_file")
        })
        .expect("read_file result should be recorded");
    assert_eq!(read_result.tool_status.as_deref(), Some("error"));
    assert!(read_result.content.contains("obtain approval"));
}

#[tokio::test]
async fn reading_skill_does_not_clear_mesh_approval_requirement() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-mesh-approval-persists-after-read";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Mesh approval persists after read".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "request_approval systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": true,
                        "mesh_approval_id": "approval-required-2",
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "skill_read_completed": false,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    engine.hydrate().await.expect("hydrate");

    let state = engine
        .record_thread_skill_read_compliance(thread_id, "systematic-debugging")
        .await
        .expect("state should exist after hydrate");

    assert!(
        !state.compliant,
        "read_skill compliance alone must not clear approval-required state"
    );
    assert!(state.mesh_requires_approval);

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"mesh_requires_approval\":true"));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn substantive_follow_up_does_not_downgrade_hydrated_mesh_approval_requirement() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures. Choose it when the task is to debug panic in rust service incidents.\n",
    )
    .expect("write skill");
    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-hydrated-approval-persists-across-preflight";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Hydrated approval persists across preflight".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "request_approval systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": true,
                        "mesh_approval_id": "approval-required-3",
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "skill_read_completed": false,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = false;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in parser",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let finalized_state = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(state) = engine.get_thread_skill_discovery_state(thread_id).await {
                if !state.discovery_pending {
                    return state;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected async skill preflight to complete");

    assert_eq!(
        finalized_state.recommended_skill.as_deref(),
        Some("systematic-debugging")
    );

    let metadata = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let persisted = engine
                .history
                .get_thread(thread_id)
                .await
                .expect("read persisted thread")
                .expect("thread should persist");
            let metadata = persisted.metadata_json.expect("thread metadata");
            if metadata.contains("\"mesh_requires_approval\":true")
                && metadata.contains("\"compliant\":false")
            {
                return metadata;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected persisted metadata to keep approval-required state");
    assert!(metadata.contains("\"mesh_requires_approval\":true"));
    assert!(metadata.contains("\"compliant\":false"));

    let read_result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let maybe_message = {
                let threads = engine.threads.read().await;
                threads
                    .get(thread_id)
                    .and_then(|thread| {
                        thread.messages.iter().rev().find(|message| {
                            message.role == MessageRole::Tool
                                && message.tool_name.as_deref() == Some("read_file")
                        })
                    })
                    .cloned()
            };
            if let Some(message) = maybe_message {
                break message;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected read_file result to be recorded");
    assert_eq!(read_result.tool_status.as_deref(), Some("error"));
    assert!(read_result.content.contains("obtain approval"));
}

#[tokio::test]
async fn substantive_follow_up_preserves_hydrated_advisory_mesh_next_step() {
    let root = tempdir().unwrap();
    let skills_root = root.path().join("skills");
    let skill_dir = skills_root.join("debugging-playbook");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Debugging playbook\nUse this skill for generic debug investigations.\n",
    )
    .expect("write skill");
    let systematic_skill_dir = skills_root.join("systematic-debugging");
    fs::create_dir_all(&systematic_skill_dir).expect("create stronger skills directory");
    fs::write(
        systematic_skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures. Choose it when the task is to debug panic in rust service incidents.\n",
    )
    .expect("write stronger skill");
    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-hydrated-advisory-persists-across-preflight";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Hydrated advisory persists across preflight".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "debugging-playbook",
                        "recommended_action": "justify_skill_skip",
                        "mesh_next_step": "choose_or_bypass",
                        "mesh_requires_approval": false,
                        "mesh_approval_id": null,
                        "read_skill_identifier": "debugging-playbook",
                        "skip_rationale": null,
                        "skill_read_completed": false,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.80;
    config.skill_recommendation.weak_match_threshold = 0.30;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let finalized_state = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if let Some(state) = engine.get_thread_skill_discovery_state(thread_id).await {
                if !state.discovery_pending {
                    return state;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected async skill preflight to complete");

    assert!(
        finalized_state.recommended_skill.is_some(),
        "follow-up discovery should keep a concrete recommended skill"
    );
    assert_eq!(
        finalized_state.mesh_next_step,
        Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ChooseOrBypass)
    );

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"mesh_next_step\":\"choose_or_bypass\""));

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist")
    };
    let read_result = thread
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Tool && message.tool_name.as_deref() == Some("read_file")
        })
        .expect("read_file result should be recorded");
    assert_eq!(read_result.tool_status.as_deref(), Some("done"));
}

#[tokio::test]
async fn terse_follow_up_still_emits_hydrated_mesh_guidance_without_fresh_preflight() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-terse-follow-up-hydrated-mesh-guidance";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Hydrated terse follow-up guidance".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "request_approval systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": true,
                        "mesh_approval_id": "approval-required-4",
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "skill_read_completed": true,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "ok",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let mut saw_skill_preflight_notice = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::WorkflowNotice {
            thread_id: event_thread_id,
            kind,
            message,
            details,
        } = event
        {
            if event_thread_id == thread_id && kind == "skill-preflight" {
                saw_skill_preflight_notice = true;
                assert!(message.contains("request_approval systematic-debugging"));
                assert!(details
                    .unwrap_or_default()
                    .contains("\"mesh_requires_approval\":true"));
            }
        }
    }
    assert!(saw_skill_preflight_notice);
}

#[tokio::test]
async fn approval_id_survives_substantive_follow_up_and_can_be_resolved() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures. Choose it when the task is to debug panic in rust service incidents.\n",
    )
    .expect("write skill");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-approval-id-survives-follow-up";
    let approval_id = "approval-required-5";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Approval id survives follow-up".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "request_approval systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": true,
                        "mesh_approval_id": approval_id,
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "skill_read_completed": true,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![]).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 0;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in parser",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let preserved = engine
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("preserved state");
    assert_eq!(preserved.mesh_approval_id.as_deref(), Some(approval_id));

    let resolved = engine
        .record_thread_skill_approval_resolution(thread_id, approval_id)
        .await
        .expect("approval should resolve");
    assert!(resolved.compliant);
    assert!(!resolved.mesh_requires_approval);
}

#[tokio::test]
async fn hydrated_legacy_name_only_skill_identifier_becomes_compliant_after_read_skill() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures.\n",
    )
    .expect("write skill");

    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-legacy-name-only-read-skill";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Legacy name-only gate".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "read_skill systematic-debugging",
                        "read_skill_identifier": "systematic-debugging",
                        "skip_rationale": null,
                        "compliant": false,
                        "updated_at": 123
                    }
                })
                .to_string(),
            ),
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "seed-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "debug panic".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_skill".to_string(),
        serde_json::json!({ "skill": "systematic-debugging" }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.hydrate().await.expect("hydrate");

    let state = engine
        .record_thread_skill_read_compliance(thread_id, "systematic-debugging")
        .await
        .expect("legacy state should be present after hydrate");

    assert!(state.compliant);

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"compliant\":true"));
}

#[tokio::test]
async fn local_strong_match_still_runs_when_background_community_scout_enabled() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic failures in rust services.\n",
    )
    .expect("write skill");

    let registry_dir = root.path().join("registry");
    fs::create_dir_all(&registry_dir).expect("create registry directory");
    fs::write(
        registry_dir.join("index.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "updated_at": 42,
            "skills": [{
                "name": "community-debugging-expert",
                "description": "Advanced panic debugging workflow from the registry.",
                "version": "1.0.0",
                "publisher_id": "publisher-1",
                "publisher_verified": true,
                "success_rate": 0.91,
                "use_count": 18,
                "content_hash": "abc123",
                "zorai_version": "0.3.1",
                "maturity_at_publish": "proven",
                "tags": ["debug", "rust", "panic"],
                "published_at": 42
            }]
        }))
        .expect("serialize registry index"),
    )
    .expect("write registry index");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = true;
    config.extra.insert(
        "registry_url".to_string(),
        serde_json::Value::String("http://127.0.0.1:9".to_string()),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-community-scout-enabled";
    let mut events = engine.subscribe();

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let finalized_state = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(state) = engine.get_thread_skill_discovery_state(thread_id).await {
                if !state.discovery_pending {
                    return state;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected async skill preflight to complete");

    assert_eq!(
        finalized_state.recommended_skill.as_deref(),
        Some("systematic-debugging")
    );

    let metadata = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let persisted = engine
                .history
                .get_thread(thread_id)
                .await
                .expect("read persisted thread")
                .expect("thread should persist");
            let metadata = persisted.metadata_json.expect("thread metadata");
            if metadata.contains("\"recommended_skill\":\"systematic-debugging\"") {
                return metadata;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("expected persisted metadata to include the resolved skill recommendation");
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));

    let scout_notice = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    details,
                    ..
                }) if event_thread_id == thread_id && kind == "skill-community-scout" => {
                    return details.expect("scout notice details");
                }
                Ok(_) => continue,
                Err(error) => panic!("workflow event stream closed unexpectedly: {error}"),
            }
        }
    })
    .await
    .expect("expected community scout notice");

    assert!(
        scout_notice.contains("community-debugging-expert"),
        "expected scout notice to include candidate payload, got: {scout_notice}"
    );
    assert!(scout_notice.contains("\"community_preapprove_timeout_secs\":30"));
}

#[tokio::test]
async fn disabled_background_community_scout_does_not_search_registry() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic failures in rust services.\n",
    )
    .expect("write skill");

    let registry_dir = root.path().join("registry");
    fs::create_dir_all(&registry_dir).expect("create registry directory");
    fs::write(
        registry_dir.join("index.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "updated_at": 42,
            "skills": [{
                "name": "community-debugging-expert",
                "description": "Advanced panic debugging workflow from the registry.",
                "version": "1.0.0",
                "publisher_id": "publisher-1",
                "publisher_verified": true,
                "success_rate": 0.91,
                "use_count": 18,
                "content_hash": "abc123",
                "zorai_version": "0.3.1",
                "maturity_at_publish": "proven",
                "tags": ["debug", "rust", "panic"],
                "published_at": 42
            }]
        }))
        .expect("serialize registry index"),
    )
    .expect("write registry index");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = false;
    config.extra.insert(
        "registry_url".to_string(),
        serde_json::Value::String("http://127.0.0.1:9".to_string()),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-community-scout-disabled";
    let mut events = engine.subscribe();

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let saw_scout_notice = tokio::time::timeout(std::time::Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    ..
                }) if event_thread_id == thread_id && kind == "skill-community-scout" => {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        !saw_scout_notice,
        "community scout should stay disabled for this turn"
    );
}

#[tokio::test]
async fn send_message_does_not_wait_for_background_skill_discovery_completion() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic failures in rust services.\n",
    )
    .expect("write skill");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![]).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 0;
    config.skill_recommendation.background_community_search = false;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let release_discovery = Arc::new(tokio::sync::Notify::new());
    let runner_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    engine.set_skill_discovery_test_runner(
        crate::agent::skill_preflight::make_delayed_test_skill_discovery_runner(
            runner_started.clone(),
            release_discovery.clone(),
            crate::agent::skill_preflight::sample_test_skill_discovery_completion(
                "debug panic in rust service",
                "systematic-debugging",
            ),
        ),
    );

    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        engine.send_message_inner(
            None,
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        ),
    )
    .await
    .expect("send_message_inner should not block on skill discovery completion")
    .expect("send message should complete");

    assert!(
        runner_started.load(std::sync::atomic::Ordering::SeqCst),
        "background runner should have started"
    );

    let pending_state = engine
        .get_thread_skill_discovery_state(&outcome.thread_id)
        .await
        .expect("pending discovery state should be stored");
    assert!(pending_state.discovery_pending);
    assert_eq!(pending_state.recommended_action, "await_skill_discovery");

    release_discovery.notify_waiters();

    let completed_state = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if let Some(state) = engine
                .get_thread_skill_discovery_state(&outcome.thread_id)
                .await
            {
                if !state.discovery_pending {
                    return state;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("background discovery result should be applied");

    assert_eq!(
        completed_state.recommended_skill.as_deref(),
        Some("systematic-debugging")
    );
    assert!(!completed_state.discovery_pending);
}

#[tokio::test]
async fn concierge_chat_runs_only_safe_lookup_tools_and_answers_from_their_results() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind concierge tool server");
    let addr = listener.local_addr().expect("concierge tool server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "concierge tool request").await;
                    recorded_requests
                        .lock()
                        .expect("lock concierge tool requests")
                        .push_back(request.clone());
                    let response = if request_body(&request).contains("\"role\":\"tool\"") {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Found your video session.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_history_1\",\"function\":{\"name\":\"search_history\",\"arguments\":\"{\\\"query\\\":\\\"video\\\"}\"}},{\"index\":1,\"id\":\"call_bash_1\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"command\\\":\\\"echo hi\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    };
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write concierge tool response");
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "concierge-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "concierge-model".to_string(),
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
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("concierge-model".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine
        .send_concierge_message_on_thread(
            crate::agent::concierge::CONCIERGE_THREAD_ID,
            "Search history for: video",
            None,
            false,
            false,
        )
        .await
        .expect("concierge reply should complete");

    let recorded = recorded_requests
        .lock()
        .expect("lock concierge tool requests")
        .iter()
        .filter(|request| request.contains("POST /v1/chat/completions"))
        .map(|request| request_body(request))
        .collect::<Vec<_>>();
    assert_eq!(recorded.len(), 2, "one tool round, then the answer");

    let first: serde_json::Value = serde_json::from_str(&recorded[0]).expect("first body json");
    let offered = first["tools"]
        .as_array()
        .expect("concierge must be offered tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect::<Vec<_>>();
    assert!(offered.contains(&"search_history"), "{offered:?}");
    assert!(offered.contains(&"session_search"), "{offered:?}");
    for unsafe_tool in [
        "bash",
        "python",
        "execute_command",
        "write_file",
        "message_agent",
    ] {
        assert!(
            !offered.contains(&unsafe_tool),
            "{unsafe_tool} offered: {offered:?}"
        );
    }

    let second: serde_json::Value = serde_json::from_str(&recorded[1]).expect("second body json");
    let tool_results = second["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter(|message| message["role"] == "tool")
        .map(|message| {
            (
                message["tool_call_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                message["content"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(tool_results.len(), 2, "{tool_results:?}");
    assert_eq!(tool_results[0].0, "call_history_1");
    assert!(
        !tool_results[0].1.contains("not available"),
        "{tool_results:?}"
    );
    assert_eq!(tool_results[1].0, "call_bash_1");
    assert_eq!(
        tool_results[1].1,
        "Tool 'bash' is not available to the concierge."
    );

    let threads = engine.threads.read().await;
    let thread = threads
        .get(crate::agent::concierge::CONCIERGE_THREAD_ID)
        .expect("concierge thread");
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Found your video session.")
    );
}
