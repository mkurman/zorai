use super::*;

#[tokio::test]
async fn execute_tool_routes_weles_runtime_review_over_internal_dm_thread() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "block",
            "reasons": ["reviewed over internal dm"],
            "audit_id": "audit-internal-dm"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-5.5".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-runtime-review";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Operator thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let parent_task = engine
        .enqueue_task(
            "Goal verifier".to_string(),
            "Submit a structured verdict without being stranded by synchronous governance"
                .to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some("goal-weles-runtime-review".to_string()),
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let tool_call = ToolCall::with_default_weles_review(
        "tool-weles-internal-dm".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": "curl -s https://example.com",
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(&parent_task.id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    // Shell tools route a `Block` verdict through the pending-approval
    // flow rather than a hard `is_error: true`: the result carries a
    // `pending_approval` so the operator can override. The shape we
    // verify is "blocked pending approval", not "hard error".
    assert!(
        !result.is_error,
        "shell block surfaces as pending approval, not hard error: {}",
        result.content
    );
    assert!(
        result.pending_approval.is_some(),
        "shell block should attach a pending approval handle for the operator"
    );
    let review = result
        .weles_review
        .expect("guarded result should include WELES review metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Block);
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("reviewed over internal dm")));

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let threads = engine.threads.read().await;
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("WELES review should create an internal DM thread");
    assert_eq!(
        dm_thread.title,
        crate::agent::agent_identity::internal_dm_thread_title(
            crate::agent::agent_identity::MAIN_AGENT_ID,
            crate::agent::agent_identity::WELES_AGENT_ID,
        )
    );
    assert!(
        dm_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::Assistant
                && message.content.contains("audit-internal-dm")
        }),
        "WELES assistant response should be stored on the internal DM thread"
    );
    assert_eq!(
        threads
            .get(thread_id)
            .expect("operator thread should remain present")
            .messages
            .len(),
        1,
        "operator thread should not receive the internal WELES exchange"
    );
    drop(threads);

    let tasks = engine.list_tasks().await;
    assert!(
        tasks
            .iter()
            .all(|task| task.parent_task_id.as_deref() != Some(parent_task.id.as_str())),
        "governance review is out-of-band and must not become a child of the goal task"
    );
    let child_tasks = tasks
        .into_iter()
        .filter(|task| {
            task.sub_agent_def_id.as_deref()
                == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
                && task.parent_thread_id.as_deref() == Some(thread_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        child_tasks.len(),
        1,
        "one governance review task should be recorded"
    );
    assert!(
        child_tasks
            .iter()
            .all(|task| task.status.is_terminal()),
        "synchronous governance children must be terminal before tool execution returns: {child_tasks:?}"
    );
}

#[tokio::test]
async fn execute_tool_weles_scope_blocks_shell_python_without_recursive_spawn_from_message_loop() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["should never be used for recursive review"],
            "audit_id": "audit-should-not-recurse"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-5.5".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-loop";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "WELES loop thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let task = super::spawn_weles_internal_subagent(
            &engine,
            thread_id,
            None,
            "governance",
            "bash_command",
            &serde_json::json!({"command": "git clone https://github.com/mkurman/claude-scientific-skills.git ~/skills/claude-scientific-skills 2>&1"}),
            SecurityLevel::Highest,
            &["shell command requests network access".to_string()],
        )
        .await
        .expect("daemon-owned WELES governance spawn should succeed");
    let before_tasks = engine.list_tasks().await.len();

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Run the governance review",
            None,
            Some(&task.id),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("WELES runtime send should complete without recursion crash");

    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(engine.list_tasks().await.len(), before_tasks);
}

#[tokio::test]
async fn synchronous_weles_governance_task_is_never_dispatched_by_the_scheduler() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        "Acknowledged.".to_string(),
    )
    .await;
    config.model = "gpt-5.5".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-dispatch";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Operator thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let task = super::spawn_weles_internal_subagent(
        &engine,
        thread_id,
        None,
        "governance",
        "bash_command",
        &serde_json::json!({"command": "curl -s https://example.com"}),
        SecurityLevel::Moderate,
        &["network access".to_string()],
    )
    .await
    .expect("daemon-owned WELES governance spawn should succeed");
    assert_eq!(
        task.status,
        crate::agent::types::TaskStatus::InProgress,
        "governance reviews run synchronously and must not wait in the scheduler queue"
    );
    engine
        .clone()
        .dispatch_ready_tasks()
        .await
        .expect("scheduler pass should succeed");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let stored = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|entry| entry.id == task.id)
        .expect("task should remain persisted");
    assert_eq!(stored.status, crate::agent::types::TaskStatus::InProgress);
    assert!(
        stored.lane_id.is_none(),
        "the scheduler must not run a governance review that the tool call is already running: {:?}",
        stored.logs
    );
    assert!(
        recorded_bodies
            .lock()
            .expect("lock stub assistant body log")
            .is_empty(),
        "no second WELES model call may be made for a synchronous governance review"
    );
}

#[tokio::test]
async fn workspace_review_weles_task_uses_dedicated_review_thread() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url =
        spawn_stub_assistant_server_for_tool_executor(recorded_bodies, "Reviewed.".to_string())
            .await;
    config.model = "gpt-5.5".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let parent_thread_id = "workspace-thread-delivery";
    engine.threads.write().await.insert(
        parent_thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: parent_thread_id.to_string(),
            agent_name: Some("Svarog".to_string()),
            title: "Delivered workspace task".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("Delivered", 1)],
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

    let task_id = "task_workspace_review_weles";
    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: task_id.to_string(),
            title: "Review workspace task".to_string(),
            description: "Review completion of workspace task wtask-1.".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "workspace_review".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: Some(parent_thread_id.to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
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
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: Some(
                crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string(),
            ),
        });

    engine
        .clone()
        .dispatch_ready_tasks()
        .await
        .expect("scheduler should dispatch the queued workspace review task");

    timeout(Duration::from_secs(5), async {
        loop {
            let tasks = engine.list_tasks().await;
            let stored = tasks
                .iter()
                .find(|entry| entry.id == task_id)
                .expect("task should remain persisted");
            if stored.status == crate::agent::types::TaskStatus::Completed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("workspace review task should complete");

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let tasks = engine.list_tasks().await;
    let stored = tasks
        .iter()
        .find(|entry| entry.id == task_id)
        .expect("task should remain persisted");
    let review_thread_id = stored
        .thread_id
        .as_deref()
        .expect("workspace review task should bind to a review thread");

    assert_ne!(review_thread_id, dm_thread_id);
    assert_ne!(review_thread_id, parent_thread_id);
    let threads = engine.threads.read().await;
    let review_thread = threads
        .get(review_thread_id)
        .expect("dedicated review thread should exist");
    assert_eq!(
        review_thread.agent_name.as_deref(),
        Some(crate::agent::agent_identity::WELES_AGENT_NAME),
        "dedicated review thread should be owned by WELES"
    );
    assert!(
        review_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::Assistant
                && message.content.contains("Reviewed.")
        }),
        "dedicated review thread should contain the WELES review reply"
    );
}

#[tokio::test]
async fn workspace_review_weles_task_rehomes_internal_dm_thread_to_dedicated_review_thread() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        "Dedicated review.".to_string(),
    )
    .await;
    config.model = "gpt-5.5".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    engine.threads.write().await.insert(
        dm_thread_id.clone(),
        crate::agent::types::AgentThread {
            id: dm_thread_id.clone(),
            agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
            title: crate::agent::agent_identity::internal_dm_thread_title(
                crate::agent::agent_identity::MAIN_AGENT_ID,
                crate::agent::agent_identity::WELES_AGENT_ID,
            ),
            messages: vec![crate::agent::types::AgentMessage::user(
                "Existing internal DM content",
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

    let task_id = "task_workspace_review_weles_rehome";
    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: task_id.to_string(),
            title: "Review workspace task".to_string(),
            description: "Review completion of workspace task wtask-2.".to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(dm_thread_id.clone()),
            source: "workspace_review".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: Some("workspace-thread-delivery".to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
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
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_api_transport: None,
            override_system_prompt: None,
            sub_agent_def_id: Some(
                crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string(),
            ),
        });

    engine
        .clone()
        .dispatch_ready_tasks()
        .await
        .expect("scheduler should dispatch the queued workspace review task");

    timeout(Duration::from_secs(5), async {
        loop {
            let stored = engine
                .list_tasks()
                .await
                .into_iter()
                .find(|entry| entry.id == task_id)
                .expect("task should remain persisted");
            if stored.status == crate::agent::types::TaskStatus::Completed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("workspace review task should complete");

    let stored = engine
        .list_tasks()
        .await
        .into_iter()
        .find(|entry| entry.id == task_id)
        .expect("task should remain persisted");
    let review_thread_id = stored
        .thread_id
        .as_deref()
        .expect("workspace review task should bind to a review thread");
    assert_ne!(review_thread_id, dm_thread_id);

    let threads = engine.threads.read().await;
    assert_eq!(
        threads
            .get(&dm_thread_id)
            .expect("internal DM thread should still exist")
            .messages
            .len(),
        1,
        "workspace review dispatch should not append to the internal DM thread"
    );
    let review_thread = threads
        .get(review_thread_id)
        .expect("dedicated review thread should exist");
    assert_eq!(
        review_thread.agent_name.as_deref(),
        Some(crate::agent::agent_identity::WELES_AGENT_NAME)
    );
    assert!(
        review_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::Assistant
                && message.content.contains("Dedicated review.")
        }),
        "dedicated review thread should contain the new WELES review reply"
    );
}
