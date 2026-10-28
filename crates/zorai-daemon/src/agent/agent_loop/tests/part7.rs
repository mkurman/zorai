use super::*;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

#[tokio::test]
async fn degraded_weles_shell_block_emits_operator_approval_request() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "bash_command".to_string(),
        serde_json::json!({
            "command": "git clone https://example.com/repo.git /tmp/repo",
            "allow_network": true,
            "sandbox_enabled": false,
            "security_level": "lowest"
        })
        .to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.extra.insert(
        "weles_review_available".to_string(),
        serde_json::Value::Bool(false),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.event_tx.subscribe();
    let thread_id = "thread-degraded-weles-approval";

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "clone the repo",
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

    assert!(
        outcome.interrupted_for_approval,
        "shell fallback should interrupt the loop for operator approval"
    );

    let mut approval_id = None;
    while let Ok(event) = events.try_recv() {
        let value = serde_json::to_value(&event).expect("agent event should serialize");
        if value.get("type").and_then(|value| value.as_str()) == Some("approval_required") {
            approval_id = value
                .get("approval_id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            break;
        }
    }

    let approval_id =
        approval_id.expect("degraded WELES shell block should emit approval_required event");
    assert!(
        engine
            .pending_approval_commands
            .read()
            .await
            .contains_key(&approval_id),
        "approval should be registered so the operator can resolve it"
    );
}

#[tokio::test]
async fn metacognitive_sunk_cost_block_injects_reflection_and_skips_tool_execution() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("blocked.txt");
    fs::write(&readable_path, "should never be read\n").expect("write blocked file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![
        (
            "bash_command".to_string(),
            serde_json::json!({ "command": "false" }).to_string(),
        ),
        (
            "bash_command".to_string(),
            serde_json::json!({ "command": "false" }).to_string(),
        ),
        (
            "bash_command".to_string(),
            serde_json::json!({ "command": "false" }).to_string(),
        ),
    ])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 3;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-metacognitive-sunk-cost-block";

    engine
        .send_message_inner(
            Some(thread_id),
            "keep trying the shell command until it works",
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

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");

    let blocked_tool_result = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("bash_command")
                && message
                    .content
                    .contains("Tool call blocked by meta-cognitive regulator before execution")
        })
        .cloned()
        .expect("expected blocked tool result for sunk-cost intervention");
    assert_eq!(blocked_tool_result.tool_status.as_deref(), Some("error"));

    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message
                .content
                .contains("Meta-cognitive intervention: tool call blocked before execution.")
    }));

    let bash_tool_messages = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("bash_command")
        })
        .count();
    assert_eq!(
        bash_tool_messages, 3,
        "expected two real failures plus one metacognitive block, not an executed fourth call"
    );
    drop(threads);

    let model = engine.meta_cognitive_self_model.read().await;
    let sunk_cost = model
        .biases
        .iter()
        .find(|bias| bias.name == "sunk_cost")
        .expect("sunk_cost bias should exist");
    assert!(
        sunk_cost.occurrence_count >= 1,
        "sunk-cost intervention should reinforce bias occurrence count"
    );
}

#[tokio::test]
async fn metacognitive_confirmation_warning_is_advisory_before_tool_runs() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("confirm.txt");
    fs::write(&readable_path, "match confirmed\n").expect("write confirm file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![
        (
            "read_file".to_string(),
            serde_json::json!({ "path": readable_path, "offset": 0, "limit": 1 }).to_string(),
        ),
        (
            "read_file".to_string(),
            serde_json::json!({ "path": readable_path, "offset": 0, "limit": 1 }).to_string(),
        ),
        (
            "search_files".to_string(),
            serde_json::json!({
                "path": root.path(),
                "pattern": "match",
                "file_pattern": "*.txt",
                "max_results": 5,
                "timeout_seconds": 30
            })
            .to_string(),
        ),
    ])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 3;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-metacognitive-confirmation-warning";

    engine
        .send_message_inner(
            Some(thread_id),
            "verify that the file still matches what we expect",
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

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");

    // Two acceptable forms: the singular "warning before tool execution"
    // prefix produced for a lone intervention, OR the "grouped advisory
    // notices" prefix produced when `append_metacognitive_system_message`
    // compacts back-to-back warnings into a single system message. Both
    // satisfy the intent that an advisory landed before the warned tool
    // runs.
    let first_system_idx = thread
        .messages
        .iter()
        .position(|message| {
            message.role == MessageRole::System
                && (message
                    .content
                    .contains("Meta-cognitive intervention: warning before tool execution.")
                    || message
                        .content
                        .contains("Meta-cognitive intervention: grouped advisory notices."))
        })
        .expect("expected metacognitive warning system message");
    let first_search_idx = thread
        .messages
        .iter()
        .position(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("search_files")
        })
        .expect("expected eventual search_files tool execution");
    assert!(
        first_system_idx < first_search_idx,
        "reflection warning should be injected before the warned tool executes"
    );

    let search_files_results = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("search_files")
                && message.tool_status.as_deref() == Some("done")
        })
        .count();
    assert_eq!(search_files_results, 1);
    assert!(
        !thread.messages.iter().any(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("search_files")
                && message.tool_status.as_deref() == Some("error")
                && message.content.contains("meta-cognitive regulator")
        }),
        "warning-level interventions should not defer the tool with a synthetic failure"
    );
    drop(threads);

    let model = engine.meta_cognitive_self_model.read().await;
    assert!(
        model.calibration_offset < 0.0,
        "warning-level confirmation bias should feed a conservative calibration adjustment"
    );
}

#[tokio::test]
async fn metacognitive_calibration_records_uncertain_band_as_negative_predicted_success() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .apply_meta_cognitive_calibration_adjustment(
            -0.05,
            crate::agent::explanation::ConfidenceBand::Uncertain,
        )
        .await;

    let tracker = engine.calibration_tracker.read().await;
    let stats = tracker.calibration_stats();
    let (count, accuracy) = stats
        .get("LOW")
        .expect("uncertain band should record a LOW calibration observation");
    assert_eq!(*count, 1);
    assert_eq!(
        *accuracy, 0.0,
        "uncertain-band metacognitive penalties should record a negative predicted-success observation"
    );
}

#[tokio::test]
async fn metacognitive_learning_updates_persist_across_rehydrate() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .reinforce_meta_cognitive_bias_occurrence("confirmation")
        .await;
    engine
        .apply_meta_cognitive_calibration_adjustment(
            -0.08,
            crate::agent::explanation::ConfidenceBand::Likely,
        )
        .await;

    let live_model = engine.meta_cognitive_self_model.read().await.clone();
    let live_confirmation = live_model
        .biases
        .iter()
        .find(|bias| bias.name == "confirmation")
        .expect("confirmation bias should exist")
        .occurrence_count;
    assert!(live_confirmation >= 1);
    assert!(live_model.calibration_offset < 0.0);

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let loaded_model = rehydrated.meta_cognitive_self_model.read().await.clone();
    let loaded_confirmation = loaded_model
        .biases
        .iter()
        .find(|bias| bias.name == "confirmation")
        .expect("confirmation bias should exist after hydrate")
        .occurrence_count;

    assert!(
        loaded_model.calibration_offset < 0.0,
        "metacognitive calibration adjustments should persist across rehydrate"
    );
    assert!(
        loaded_confirmation >= 1,
        "bias occurrence reinforcement should persist across rehydrate"
    );
}

#[tokio::test]
async fn metacognitive_workflow_profiles_learn_from_live_outcomes_and_persist() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("workflow-learning.txt");
    fs::write(
        &readable_path,
        "workflow learning target
",
    )
    .expect("write workflow file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![
        (
            "read_file".to_string(),
            serde_json::json!({ "path": readable_path, "offset": 0, "limit": 1 }).to_string(),
        ),
        (
            "search_files".to_string(),
            serde_json::json!({
                "path": root.path(),
                "pattern": "workflow learning",
                "file_pattern": "*.txt",
                "max_results": 5,
                "timeout_seconds": 30
            })
            .to_string(),
        ),
    ])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-metacognitive-workflow-profile-learning";

    let before_model = engine.meta_cognitive_self_model.read().await.clone();
    assert!(
        !before_model.workflow_profiles.iter().any(|profile| {
            profile.name == "read_file__search_files"
                && profile.typical_tools
                    == vec!["read_file".to_string(), "search_files".to_string()]
        }),
        "sequence-specific workflow profile should not exist before learning"
    );

    engine
        .send_message_inner(
            Some(thread_id),
            "inspect the workflow file and search for the learning string",
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

    let live_model = engine.meta_cognitive_self_model.read().await.clone();
    let learned_profile = live_model
        .workflow_profiles
        .iter()
        .find(|profile| {
            profile.name == "read_file__search_files"
                && profile.typical_tools
                    == vec!["read_file".to_string(), "search_files".to_string()]
        })
        .cloned()
        .expect("sequence-specific workflow profile should be learned from live outcomes");

    assert_eq!(learned_profile.avg_steps, 2);
    assert!(
        (learned_profile.avg_success_rate - 1.0).abs() < f64::EPSILON,
        "successful workflow should learn a perfect initial success rate"
    );

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let loaded_model = rehydrated.meta_cognitive_self_model.read().await.clone();
    assert!(loaded_model.workflow_profiles.iter().any(|profile| {
        profile.name == learned_profile.name
            && profile.avg_steps == learned_profile.avg_steps
            && (profile.avg_success_rate - learned_profile.avg_success_rate).abs() < f64::EPSILON
            && profile.typical_tools == learned_profile.typical_tools
    }));
}

#[tokio::test]
async fn metacognitive_warning_repeated_tool_calls_remain_advisory() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![
        (
            tool_names::UPDATE_TODO.to_string(),
            serde_json::json!({
                "items": [{
                    "content": "Create step completion marker for goal step 1",
                    "status": "completed",
                    "step_index": 1
                }]
            })
            .to_string(),
        ),
        (
            tool_names::UPDATE_TODO.to_string(),
            serde_json::json!({
                "items": [{
                    "content": "Create step completion marker for goal step 1",
                    "status": "completed",
                    "step_index": 1
                }]
            })
            .to_string(),
        ),
        (
            tool_names::UPDATE_TODO.to_string(),
            serde_json::json!({
                "items": [{
                    "content": "Create step completion marker for goal step 1",
                    "status": "completed",
                    "step_index": 1
                }]
            })
            .to_string(),
        ),
    ])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 4;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-metacognitive-warning-repeat-escalation";
    let task_id = "task-metacognitive-warning-repeat-escalation";

    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: task_id.to_string(),
            title: "Verify goal step".to_string(),
            description: "Verify goal step".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 10,
            created_at: 1,
            started_at: Some(2),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: Some("session-warning-repeat".to_string()),
            goal_run_id: Some("goal-warning-repeat".to_string()),
            goal_run_title: Some("Warning repeat".to_string()),
            goal_step_id: Some("step-warning-repeat".to_string()),
            goal_step_title: Some("Verify goal step".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 2,
            max_retries: 3,
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

    engine
        .send_message_inner(
            Some(thread_id),
            "I am certain the verification is complete; update the todo as completed now.",
            None,
            Some(task_id),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");

    let metacognitive_tool_failures = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some(tool_names::UPDATE_TODO)
                && message.tool_status.as_deref() == Some("error")
                && message.content.contains("meta-cognitive regulator")
        })
        .count();
    assert!(
        metacognitive_tool_failures == 0,
        "warning-level interventions should not create synthetic tool failures"
    );

    let warning_system_messages = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Meta-cognitive intervention: warning before tool execution.")
        })
        .count();
    assert!(
        warning_system_messages >= 1,
        "repeated high-confidence tool calls should still emit advisory warnings"
    );

    assert!(!thread.messages.iter().any(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some(tool_names::UPDATE_TODO)
            && message
                .content
                .contains("Tool call blocked by meta-cognitive regulator before execution")
    }));

    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some(tool_names::UPDATE_TODO)
            && message.tool_status.as_deref() == Some("done")
    }));
}

#[tokio::test]
async fn repeated_get_operation_status_polling_is_suppressed_as_busy_wait() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let record = crate::server::operation_registry()
        .accept_operation("scripted_poll", Some("tool-test:scripted-poll".to_string()));
    crate::server::operation_registry().mark_started(&record.operation_id);

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(
        (0..4)
            .map(|_| {
                (
                    "get_operation_status".to_string(),
                    serde_json::json!({
                        "operation_id": record.operation_id,
                    })
                    .to_string(),
                )
            })
            .collect(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 5;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-operation-status-repeat-polling";

    engine
        .send_message_inner(
            Some(thread_id),
            "Poll the background operation until it changes state.",
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

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let operation_status_results = thread
        .messages
        .iter()
        .filter(|message| {
            message.role == MessageRole::Tool
                && message.tool_name.as_deref() == Some("get_operation_status")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        operation_status_results.len(),
        4,
        "the agent still issued four identical status calls"
    );
    assert_eq!(
        operation_status_results
            .iter()
            .filter(|message| message.tool_status.as_deref() == Some("done")
                && message.content.contains(&record.operation_id)
                && message.content.contains("\"kind\":\"scripted_poll\""))
            .count(),
        1,
        "only the first snapshot should execute: {:?}",
        operation_status_results
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        operation_status_results
            .iter()
            .filter(|message| message
                .content
                .contains("Repeated get_operation_status polling suppressed"))
            .count()
            >= 3,
        "later identical polls should be suppressed: {:?}",
        operation_status_results
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn neutral_investigative_sequence_does_not_false_positive_confirmation_bias() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("neutral.txt");
    fs::write(&readable_path, "neutral sequence\n").expect("write neutral file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![
        (
            "read_file".to_string(),
            serde_json::json!({ "path": readable_path, "offset": 0, "limit": 1 }).to_string(),
        ),
        (
            "search_files".to_string(),
            serde_json::json!({
                "path": root.path(),
                "pattern": "neutral",
                "file_pattern": "*.txt",
                "max_results": 5,
                "timeout_seconds": 30
            })
            .to_string(),
        ),
    ])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-metacognitive-confirmation-neutral";

    engine
        .send_message_inner(
            Some(thread_id),
            "inspect the file and then search for the string",
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

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");

    assert!(
        !thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Meta-cognitive intervention: warning before tool execution.")
                && message.content.contains("confirmation")
        }),
        "neutral evidence gathering should not false-positive as confirmation bias"
    );

    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some("search_files")
            && message.tool_status.as_deref() == Some("done")
    }));
}

#[tokio::test]
async fn ask_parent_interrupts_child_turn_so_dispatch_cannot_complete_it() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "ask_parent".to_string(),
        serde_json::json!({ "question": "Which schema?" }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let parent = engine
        .enqueue_task(
            "Parent".to_string(),
            "coordinate".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent-ask".to_string()),
            None,
        )
        .await;
    let child = engine
        .enqueue_task(
            "Child".to_string(),
            "ask then continue".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent-ask".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    {
        let mut tasks = engine.tasks.lock().await;
        let task = tasks
            .iter_mut()
            .find(|task| task.id == child.id)
            .expect("child exists");
        task.thread_id = Some("thread-child-ask".to_string());
        task.status = TaskStatus::InProgress;
    }
    engine.persist_tasks().await;

    let outcome = engine
        .send_task_message(
            &child.id,
            Some("thread-child-ask"),
            None,
            None,
            "ask the parent which schema to use",
        )
        .await
        .expect("child turn should return after ask_parent");

    assert!(
        outcome.interrupted_for_approval,
        "ask_parent must end the child turn so a later parent answer can inject a follow-up"
    );

    let updated = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == child.id)
        .cloned()
        .expect("child exists");
    assert_ne!(
        updated.status,
        TaskStatus::Completed,
        "dispatch success must not complete the child after ask_parent"
    );
    assert!(
        crate::agent::tool_executor::task_is_awaiting_parent(&updated),
        "child must stay blocked on the parent until answer_child runs"
    );
}

async fn spawn_tool_call_then_stalled_server(readable_path: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stalled tool call server");
    let addr = listener
        .local_addr()
        .expect("stalled tool call server local addr");
    let requests = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let requests = requests.clone();
            let readable_path = readable_path.clone();
            tokio::spawn(async move {
                let _request = read_http_request(&mut socket, "stalled tool call request").await;
                if requests.fetch_add(1, Ordering::SeqCst) > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    return;
                }
                let chunk = serde_json::json!({
                    "choices": [{
                        "delta": {
                            "tool_calls": [{
                                "index": 0,
                                "id": "call_stalled_0",
                                "function": {
                                    "name": "read_file",
                                    "arguments": serde_json::json!({ "path": readable_path }).to_string(),
                                }
                            }]
                        }
                    }],
                    "usage": { "prompt_tokens": 7, "completion_tokens": 3 }
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {chunk}\n\ndata: [DONE]\n\n"
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write stalled tool call response");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn tool_result_is_persisted_before_the_next_llm_response_arrives() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("notes.txt");
    fs::write(&readable_path, "persist me\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url =
        spawn_tool_call_then_stalled_server(readable_path.display().to_string()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 3;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-tool-result-persisted-mid-turn";

    let task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "read the notes",
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

    let persisted_tool_result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let persisted = engine
                .history
                .list_messages(thread_id, Some(20))
                .await
                .unwrap_or_default();
            if persisted.iter().any(|message| message.role == "tool") {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or(false);
    task.abort();

    assert!(
        persisted_tool_result,
        "a client that loads the thread while the next LLM call is stalled reads the database, so the tool result must already be there"
    );
}

#[tokio::test]
async fn missing_previous_response_retries_without_chain_even_after_tools_ran() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-missing-previous-response";
    {
        let mut threads = engine.threads.write().await;
        let mut assistant = crate::agent::types::AgentMessage::user("calling a tool", 2);
        assistant.role = MessageRole::Assistant;
        assistant.response_id = Some("resp_lost_by_provider".to_string());
        let mut tool = crate::agent::types::AgentMessage::user("tool output", 3);
        tool.role = MessageRole::Tool;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Missing previous response".to_string(),
                messages: vec![
                    crate::agent::types::AgentMessage::user("hello", 1),
                    assistant,
                    tool,
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
    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary:
            "muse rejected the daemon request as invalid: referenced response not found or expired"
                .to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "referenced response not found or expired"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(thread_id, &structured, true, true, &mut attempted)
        .await
        .expect("recovery evaluation should succeed");

    assert!(
        disposition.retry_attempted,
        "dropping the chain and resending history re-runs no tools, so earlier tool calls in the turn must not block the retry"
    );
    assert_eq!(
        disposition.signature.as_deref(),
        Some("request-invalid-missing-previous-response")
    );
    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(
        thread
            .messages
            .iter()
            .all(|message| message.response_id.is_none()),
        "the retry must not reference the response the provider lost"
    );
    assert_eq!(
        thread.messages.len(),
        3,
        "the committed tool call and its result must stay in the resent history"
    );
    drop(threads);

    let repeat = engine
        .maybe_recover_fixable_upstream_failure(thread_id, &structured, true, true, &mut attempted)
        .await
        .expect("repeat recovery evaluation should succeed");
    assert!(
        !repeat.retry_attempted,
        "a provider that keeps rejecting must fail loudly instead of looping"
    );
}

#[tokio::test]
async fn stale_continuation_repair_stays_blocked_after_tools_ran() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary: "provider rejected the daemon request as invalid: stale previous_response_id"
            .to_string(),
        diagnostics: serde_json::json!({}),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-stale-continuation",
            &structured,
            false,
            true,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(
        !disposition.retry_attempted,
        "repairs that rewrite tool-call history must keep the side-effect guard"
    );
}

async fn spawn_tool_call_then_lost_response_server(
    readable_path: String,
    requests: Arc<AtomicUsize>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind lost response server");
    let addr = listener
        .local_addr()
        .expect("lost response server local addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let requests = requests.clone();
            let readable_path = readable_path.clone();
            tokio::spawn(async move {
                let _request = read_http_request(&mut socket, "lost response request").await;
                let response = match requests.fetch_add(1, Ordering::SeqCst) {
                    0 => {
                        let chunk = serde_json::json!({
                            "choices": [{
                                "delta": {
                                    "tool_calls": [{
                                        "index": 0,
                                        "id": "call_lost_0",
                                        "function": {
                                            "name": "read_file",
                                            "arguments": serde_json::json!({ "path": readable_path }).to_string(),
                                        }
                                    }]
                                }
                            }],
                            "usage": { "prompt_tokens": 7, "completion_tokens": 3 }
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\ndata: {chunk}\n\ndata: [DONE]\n\n"
                        )
                    }
                    1 => {
                        let body = serde_json::json!({
                            "error": {
                                "message": "referenced response not found or expired",
                                "type": "invalid_request_error"
                            }
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    }
                    _ => concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Recovered.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                    .to_string(),
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write lost response reply");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn turn_recovers_when_provider_loses_previous_response_after_a_tool_call() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("notes.txt");
    fs::write(&readable_path, "recover me\n").expect("write readable file");
    let requests = Arc::new(AtomicUsize::new(0));

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_tool_call_then_lost_response_server(
        readable_path.display().to_string(),
        requests.clone(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 4;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-lost-previous-response";

    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        engine.send_message_inner(
            Some(thread_id),
            "read the notes",
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
    .expect("turn should not hang")
    .expect("turn should recover instead of failing on the lost response");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let tool_results = thread
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .count();
    assert_eq!(
        tool_results, 1,
        "the recovery retry must not run the tool again"
    );
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered.")
    );
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn responses_turn_drops_lost_previous_response_id_and_recovers() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind lost previous response server");
    let addr = listener
        .local_addr()
        .expect("lost previous response server addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                tokio::spawn(async move {
                    let request =
                        read_http_request(&mut socket, "lost previous response request").await;
                    let attempt = {
                        let mut bodies = recorded_bodies.lock().expect("lock recorded bodies");
                        bodies.push(request_body(&request));
                        bodies.len() - 1
                    };
                    let response = if attempt == 0 {
                        let body = serde_json::json!({
                            "error": {
                                "message": "referenced response not found or expired",
                                "type": "invalid_request_error"
                            }
                        })
                        .to_string();
                        format!(
                            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        let body = concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_fresh\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Recovered without chain\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_fresh\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":4,\"total_tokens\":14},\"error\":null}}\n\n"
                        );
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    };
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write lost previous response reply");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-responses-lost-previous-response";
    let mut anchored = crate::agent::types::AgentMessage::user("Earlier answer", 2);
    anchored.role = MessageRole::Assistant;
    anchored.response_id = Some("resp_lost_by_provider".to_string());
    anchored.provider = Some(PROVIDER_ID_OPENAI.to_string());
    anchored.model = Some("gpt-5.4".to_string());
    anchored.api_transport = Some(ApiTransport::Responses);
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Lost previous response".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user("Earlier question", 1),
                anchored,
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

    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        engine.send_message_inner(
            Some(thread_id),
            "continue",
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
    .expect("turn should not hang")
    .expect("turn should recover from the lost previous response");

    let bodies = recorded_bodies
        .lock()
        .expect("lock recorded bodies")
        .clone();
    assert_eq!(
        bodies.len(),
        2,
        "expected one rejected and one recovered request: {bodies:?}"
    );
    assert!(bodies[0].contains("resp_lost_by_provider"));
    assert!(
        !bodies[1].contains("resp_lost_by_provider"),
        "the retry must resend history instead of chaining to the response the provider lost: {}",
        bodies[1]
    );
    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered without chain")
    );
}

#[tokio::test]
async fn cleared_continuation_survives_reloading_the_thread_from_the_database() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-cleared-continuation-reload";
    let mut anchored = crate::agent::types::AgentMessage::user("Earlier answer", 2);
    anchored.role = MessageRole::Assistant;
    anchored.response_id = Some("resp_lost_by_provider".to_string());
    let mut tool_result = crate::agent::types::AgentMessage::user("tool output", 3);
    tool_result.role = MessageRole::Tool;
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Cleared continuation reload".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user("Earlier question", 1),
                anchored,
                tool_result,
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
    engine.persist_thread_by_id(thread_id).await;

    engine.clear_thread_continuation_state(thread_id).await;
    let reloaded = engine
        .restore_thread_with_messages_from_db(thread_id)
        .await
        .expect("thread should reload from the database");

    assert!(
        reloaded.messages.iter().all(|message| message.response_id.is_none()),
        "retry paths reload the thread from the database, so a cleared chain must not come back from older persisted rows"
    );
}

async fn spawn_repeated_lost_response_server(
    readable_path: String,
    recorded_bodies: Arc<StdMutex<Vec<String>>>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind repeated lost response server");
    let addr = listener
        .local_addr()
        .expect("repeated lost response server addr");

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let recorded_bodies = recorded_bodies.clone();
            let readable_path = readable_path.clone();
            tokio::spawn(async move {
                let request =
                    read_http_request(&mut socket, "repeated lost response request").await;
                let attempt = {
                    let mut bodies = recorded_bodies.lock().expect("lock recorded bodies");
                    bodies.push(request_body(&request));
                    bodies.len() - 1
                };
                let lost = || {
                    let body = serde_json::json!({
                        "error": {
                            "message": "referenced response not found or expired",
                            "type": "invalid_request_error"
                        }
                    })
                    .to_string();
                    format!(
                        "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                let stream = |body: String| {
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                let response = match attempt {
                    0 | 2 => lost(),
                    1 => {
                        let item = serde_json::json!({
                            "type": "function_call",
                            "id": "fc_read",
                            "call_id": "call_read",
                            "name": "read_file",
                            "arguments": serde_json::json!({ "path": readable_path }).to_string(),
                            "status": "completed"
                        });
                        stream(format!(
                            "data: {}\n\ndata: {}\n\ndata: {}\n\n",
                            serde_json::json!({"type": "response.created", "response": {"id": "resp_after_first_loss"}}),
                            serde_json::json!({"type": "response.output_item.done", "output_index": 0, "item": item}),
                            serde_json::json!({"type": "response.completed", "response": {"id": "resp_after_first_loss", "object": "response", "status": "completed", "output": [item], "usage": {"input_tokens": 10, "output_tokens": 4, "total_tokens": 14}, "error": null}}),
                        ))
                    }
                    _ => stream(
                        concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_final\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Recovered twice\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_final\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":10,\"output_tokens\":4,\"total_tokens\":14},\"error\":null}}\n\n"
                        )
                        .to_string(),
                    ),
                };
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write repeated lost response reply");
            });
        }
    });

    format!("http://{addr}/v1")
}

#[tokio::test]
async fn responses_turn_recovers_from_a_second_lost_response_after_the_chain_rebuilt() {
    let root = tempdir().unwrap();
    let readable_path = root.path().join("notes.txt");
    fs::write(&readable_path, "read me\n").expect("write readable file");
    let recorded_bodies = Arc::new(StdMutex::new(Vec::<String>::new()));

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_repeated_lost_response_server(
        readable_path.display().to_string(),
        recorded_bodies.clone(),
    )
    .await;
    config.model = "gpt-5.4".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 4;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-repeated-lost-response";
    let mut anchored = crate::agent::types::AgentMessage::user("Earlier answer", 2);
    anchored.role = MessageRole::Assistant;
    anchored.response_id = Some("resp_lost_first".to_string());
    anchored.provider = Some(PROVIDER_ID_OPENAI.to_string());
    anchored.model = Some("gpt-5.4".to_string());
    anchored.api_transport = Some(ApiTransport::Responses);
    engine.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: None,
            title: "Repeated lost response".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user("Earlier question", 1),
                anchored,
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

    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        engine.send_message_inner(
            Some(thread_id),
            "read the notes",
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
    .expect("turn should not hang")
    .expect("a long turn must survive more than one lost response");

    let bodies = recorded_bodies
        .lock()
        .expect("lock recorded bodies")
        .clone();
    assert_eq!(
        bodies.len(),
        4,
        "expected two rejected and two recovered requests: {bodies:?}"
    );
    assert!(bodies[2].contains("resp_after_first_loss"));
    assert!(
        !bodies[3].contains("resp_after_first_loss"),
        "the second recovery must also drop the lost chain: {}",
        bodies[3]
    );
    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::Tool)
            .count(),
        1,
        "recovering must not run the tool again"
    );
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Recovered twice")
    );
}
