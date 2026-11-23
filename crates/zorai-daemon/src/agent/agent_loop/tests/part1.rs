use super::*;
use zorai_shared::providers::PROVIDER_ID_OPENAI;

#[test]
fn parse_plugin_command_basic() {
    let result = parse_plugin_command("/gmail-calendar.inbox");
    assert_eq!(result, Some(("/gmail-calendar.inbox", "")));
}

#[test]
fn parse_plugin_command_with_args() {
    let result = parse_plugin_command("/gmail-calendar.inbox check today");
    assert_eq!(result, Some(("/gmail-calendar.inbox", "check today")));
}

#[test]
fn parse_plugin_command_regular_message() {
    let result = parse_plugin_command("regular message");
    assert_eq!(result, None);
}

#[test]
fn parse_plugin_command_no_dot() {
    let result = parse_plugin_command("/help");
    assert_eq!(result, None);
}

#[test]
fn parse_plugin_command_with_whitespace() {
    let result = parse_plugin_command("  /weather.forecast London  ");
    assert_eq!(result, Some(("/weather.forecast", "London")));
}

#[test]
fn parse_plugin_command_slash_no_dot_with_args() {
    let result = parse_plugin_command("/help me please");
    assert_eq!(result, None);
}

#[tokio::test]
async fn repair_tool_call_sequence_updates_persisted_history() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_repair";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Repair test".to_string(),
                created_at: 1,
                updated_at: 1,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("start", 1),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-tool-turn".to_string(),
                        role: MessageRole::Assistant,
                        content: "checking".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: Some(vec![
                            ToolCall {
                                id: "2013".to_string(),
                                function: ToolFunction {
                                    name: "tool_a".to_string(),
                                    arguments: "{}".to_string(),
                                },
                                weles_review: None,
                            },
                            ToolCall {
                                id: "2014".to_string(),
                                function: ToolFunction {
                                    name: "tool_b".to_string(),
                                    arguments: "{}".to_string(),
                                },
                                weles_review: None,
                            },
                        ]),
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
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "tool-result-2013".to_string(),
                        role: MessageRole::Tool,
                        content: "partial".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: None,
                        tool_call_id: Some("2013".to_string()),
                        tool_name: Some("tool_a".to_string()),
                        tool_arguments: Some("{}".to_string()),
                        tool_status: Some("done".to_string()),
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
                        timestamp: 3,
                        feedback: None,
                    },
                    AgentMessage::user("continue", 4),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine.repair_tool_call_sequence(thread_id).await;

    let live = engine.threads.read().await;
    let thread = live.get(thread_id).expect("thread should still exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "start");
    assert_eq!(thread.messages[1].content, "continue");
    drop(live);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].content, "start");
    assert_eq!(persisted[1].content, "continue");
}

#[tokio::test]
async fn policy_halt_aborts_before_guarded_tool_execution_and_persists_failure_trace() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_tool_call_server().await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-policy-loop-proof";

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(crate::agent::types::AgentTask {
            id: "task-policy-loop-proof".to_string(),
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
            goal_run_id: Some("goal-1".to_string()),
            goal_run_title: Some("Test goal".to_string()),
            goal_step_id: Some("step-1".to_string()),
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
            id: "goal-1".to_string(),
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
            child_task_ids: vec!["task-policy-loop-proof".to_string()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: Some("task-policy-loop-proof".to_string()),
            pending_review_report: None,
            duration_ms: None,
            steps: vec![crate::agent::types::GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "Investigate failure".to_string(),
                instructions: "Inspect the failing path".to_string(),
                kind: crate::agent::types::GoalRunStepKind::Research,
                success_criteria: "Know why it failed".to_string(),
                session_id: None,
                status: crate::agent::types::GoalRunStepStatus::InProgress,
                task_id: Some("task-policy-loop-proof".to_string()),
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

    let now_epoch_secs = current_epoch_secs();
    let args_hash =
        crate::agent::episodic::counter_who::compute_approach_hash("definitely_unknown_tool", "{}");
    let scope = crate::agent::orchestrator_policy::PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
    };
    engine
        .record_retry_guard(&scope, &args_hash, now_epoch_secs)
        .await;
    {
        let mut stores = engine.episodic_store.write().await;
        let store = stores
            .entry(crate::agent::agent_identity::MAIN_AGENT_ID.to_string())
            .or_default();
        store.counter_who.tried_approaches = VecDeque::from(vec![
            crate::agent::episodic::TriedApproach {
                approach_hash: args_hash.clone(),
                description: "definitely_unknown_tool({})".to_string(),
                outcome: crate::agent::episodic::EpisodeOutcome::Failure,
                timestamp: 1,
            },
            crate::agent::episodic::TriedApproach {
                approach_hash: args_hash.clone(),
                description: "definitely_unknown_tool({})".to_string(),
                outcome: crate::agent::episodic::EpisodeOutcome::Failure,
                timestamp: 2,
            },
        ])
        .into_iter()
        .collect();
    }
    {
        let mut awareness = engine.awareness.write().await;
        for ts in 1..=4 {
            awareness.record_outcome(
                thread_id,
                "thread",
                "definitely_unknown_tool",
                &args_hash,
                false,
                false,
                ts,
            );
        }
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate the failure",
            None,
            Some("task-policy-loop-proof"),
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

    let mut saw_guarded_tool_call = false;
    let mut seen_tool_result = false;
    let mut buffered = Vec::new();
    while let Ok(event) = events.try_recv() {
        match &event {
            AgentEvent::ToolCall {
                thread_id: event_thread_id,
                name,
                ..
            } if event_thread_id == thread_id && name == "definitely_unknown_tool" => {
                saw_guarded_tool_call = true;
            }
            AgentEvent::ToolResult {
                thread_id: event_thread_id,
                name,
                ..
            } if event_thread_id == thread_id && name == "definitely_unknown_tool" => {
                seen_tool_result = true;
            }
            _ => {}
        }
        buffered.push(event);
    }
    assert!(
        !saw_guarded_tool_call,
        "expected retry guard to abort before emitting a guarded tool call",
    );
    assert!(
        !seen_tool_result,
        "expected retry guard to abort before producing a guarded tool result",
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread");
    assert!(!thread.messages.iter().any(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some("definitely_unknown_tool")
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message
                .content
                .contains("Policy halted a repeated retry for the same failing approach.")
    }));
    drop(threads);

    let tasks = engine.tasks.lock().await;
    let task = tasks
        .iter()
        .find(|task| task.id == "task-policy-loop-proof")
        .expect("task");
    assert_eq!(task.status, TaskStatus::Failed);
    drop(tasks);

    let trace_outcome = latest_trace_outcome_for_task(root.path(), "task-policy-loop-proof").await;
    assert_eq!(trace_outcome.as_deref(), Some("failure"));
}
