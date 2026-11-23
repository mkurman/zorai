use super::analysis::{classify_stalled_turn, follow_through_observed};
use super::runtime::StalledTurnCandidate;
use super::types::{StalledTurnClass, TurnEvidence};
use crate::agent::types::{
    AgentConfig, AgentMessage, AgentTask, AgentThread, ApiTransport, GoalRun, GoalRunStatus,
    MessageRole, TaskPriority, TaskStatus, ToolCall, ToolFunction,
};
use crate::agent::{
    StreamProgressKind, ThreadHandoffState, ThreadResponderFrame, CONCIERGE_AGENT_ID,
    CONCIERGE_AGENT_NAME, MAIN_AGENT_ID, MAIN_AGENT_NAME, WELES_AGENT_ID,
};
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn base_evidence() -> TurnEvidence {
    TurnEvidence {
        last_assistant_message: "Working. Let me draft the redesigned content now.".to_string(),
        preceded_by_tool_result: false,
        new_tool_call_followed: false,
        new_substantive_assistant_message_followed: false,
        task_or_goal_progressed: false,
        user_replied: false,
    }
}

#[test]
fn classify_promise_without_action_for_progress_filler_message() {
    let evidence = base_evidence();
    let actual = classify_stalled_turn(&evidence);
    assert_eq!(actual, Some(StalledTurnClass::PromiseWithoutAction));
}

#[test]
fn classify_post_tool_result_no_follow_through_when_promise_follows_tool_result() {
    let mut evidence = base_evidence();
    evidence.preceded_by_tool_result = true;

    let actual = classify_stalled_turn(&evidence);
    assert_eq!(
        actual,
        Some(StalledTurnClass::PostToolResultNoFollowThrough)
    );
}

#[test]
fn ignores_message_when_concrete_follow_through_happened() {
    let mut evidence = base_evidence();
    evidence.new_tool_call_followed = true;

    assert!(follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn ignores_message_when_user_replied() {
    let mut evidence = base_evidence();
    evidence.user_replied = true;

    assert!(follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn ignores_non_promise_message() {
    let mut evidence = base_evidence();
    evidence.last_assistant_message =
        "The redesign is complete. Here is the new landing page copy.".to_string();

    assert!(!follow_through_observed(&evidence));
    assert_eq!(classify_stalled_turn(&evidence), None);
}

#[test]
fn stalled_turn_candidate_starts_with_thirty_second_grace_window() {
    let candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);
    assert_eq!(candidate.retries_sent, 0);
    assert_eq!(candidate.next_evaluation_at, 31_000);
}

#[test]
fn stalled_turn_candidate_uses_increasing_retry_backoff() {
    let mut candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);

    candidate.record_retry(31_000);
    assert_eq!(candidate.retries_sent, 1);
    assert_eq!(candidate.next_evaluation_at, 91_000);

    candidate.record_retry(91_000);
    assert_eq!(candidate.retries_sent, 2);
    assert_eq!(candidate.next_evaluation_at, 211_000);
}

#[test]
fn stalled_turn_candidate_marks_escalation_ready_after_third_retry_window() {
    let mut candidate =
        StalledTurnCandidate::new("thread-1", StalledTurnClass::PromiseWithoutAction, 1_000);

    candidate.record_retry(31_000);
    candidate.record_retry(91_000);
    candidate.record_retry(211_000);

    assert_eq!(candidate.retries_sent, 3);
    assert!(!candidate.escalation_ready(330_999));
    assert!(candidate.escalation_ready(331_000));
}

#[test]
fn stalled_turn_worker_returns_continue_thread_action() {
    let observation = super::types::ThreadStallObservation {
        thread_id: "thread-1".to_string(),
        last_message_id: "assistant-1".to_string(),
        last_message_at: 1_000,
        last_assistant_message: "Working. Let me draft the redesigned content now.".to_string(),
        class: StalledTurnClass::PromiseWithoutAction,
        stream_progress_kind: None,
        task_id: Some("task-1".to_string()),
        goal_run_id: None,
    };

    let result = crate::agent::background_workers::domain_safety::evaluate_tick(
        vec![observation],
        Vec::new(),
        31_000,
    );

    assert_eq!(result.len(), 1);
    assert!(matches!(
        &result[0],
        crate::agent::background_workers::protocol::SafetyDecision::Retry { candidate }
            if candidate.thread_id == "thread-1" && candidate.retries_sent == 0
    ));
}

async fn spawn_stub_assistant_server(response_text: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub assistant server");
    let addr = listener
        .local_addr()
        .expect("stub assistant server local addr");
    let response_text = response_text.to_string();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let response_text = response_text.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 65536];
                let _ = socket.read(&mut buffer).await;
                let response = format!(
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
                        "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                        "data: [DONE]\n\n"
                    ),
                    response_text
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write stub assistant response");
            });
        }
    });

    format!("http://{addr}/v1")
}

async fn build_test_engine(response_text: &str) -> Arc<crate::agent::AgentEngine> {
    let root = tempdir().expect("tempdir should succeed").keep();
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_stub_assistant_server(response_text).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    crate::agent::AgentEngine::new_test(manager, config, &root).await
}

fn promise_thread(thread_id: &str, now: u64) -> AgentThread {
    let mut assistant = AgentMessage::user(
        "Working. Let me continue the delegated task now.",
        now.saturating_sub(60_000),
    );
    assistant.id = "assistant-promise".to_string();
    assistant.role = MessageRole::Assistant;

    AgentThread {
        id: thread_id.to_string(),
        agent_name: Some("Dazhbog".to_string()),
        title: "Spawned worker".to_string(),
        messages: vec![
            AgentMessage::user("Finish the delegated work.", now.saturating_sub(61_000)),
            assistant,
        ],
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
        created_at: now.saturating_sub(61_000),
        updated_at: now.saturating_sub(60_000),
    }
}

fn spawned_task(thread_id: &str, task_id: &str, status: TaskStatus, now: u64) -> AgentTask {
    AgentTask {
        id: task_id.to_string(),
        title: "Spawned worker".to_string(),
        description: "Delegated worker".to_string(),
        status,
        priority: TaskPriority::Normal,
        progress: if matches!(status, TaskStatus::Completed) {
            100
        } else {
            0
        },
        created_at: now.saturating_sub(120_000),
        started_at: Some(now.saturating_sub(120_000)),
        completed_at: matches!(status, TaskStatus::Completed).then_some(now.saturating_sub(90_000)),
        error: None,
        result: matches!(status, TaskStatus::Completed).then_some("done".to_string()),
        thread_id: Some(thread_id.to_string()),
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
    }
}

fn terminal_goal_run_for_thread(goal_run_id: &str, thread_id: &str, now: u64) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "Stopped goal".to_string(),
        goal: "Do goal work".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Cancelled,
        priority: TaskPriority::Normal,
        created_at: now.saturating_sub(120_000),
        updated_at: now.saturating_sub(30_000),
        started_at: Some(now.saturating_sub(120_000)),
        completed_at: Some(now.saturating_sub(30_000)),
        thread_id: Some(thread_id.to_string()),
        root_thread_id: Some(thread_id.to_string()),
        supervision_thread_id: None,
        active_thread_id: Some(thread_id.to_string()),
        execution_thread_ids: vec![thread_id.to_string()],
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        step_owner_overrides: std::collections::BTreeMap::new(),
        replan_count: 0,
        max_replans: 0,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        step_failure_history: Vec::new(),
        stopped_reason: Some("operator_stop".to_string()),
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
        autonomy_level: super::super::autonomy::AutonomyLevel::Autonomous,
        authorship_tag: None,
    }
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_active_tool_turns() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-active-tool";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Active tool turn".to_string(),
                messages: vec![AgentMessage {
                    reasoning_tokens: None,
                    thinking_ms: None,
                    id: "assistant-tool".to_string(),
                    role: MessageRole::Assistant,
                    content: String::new(),
                    content_blocks: Vec::new(),
                    tool_calls: Some(vec![ToolCall::with_default_weles_review(
                        "call-1".to_string(),
                        ToolFunction {
                            name: zorai_protocol::tool_names::READ_FILE.to_string(),
                            arguments: "{}".to_string(),
                        },
                    )]),
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
                    message_kind: crate::agent::types::AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: now.saturating_sub(60_000),
                    feedback: None,
                }],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(observations.is_empty());
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_idle_stream_with_unanswered_tool_call() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-active-bash-tool-stream";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Active bash tool stream".to_string(),
                messages: vec![
                    AgentMessage::user("Run the long command.", now.saturating_sub(120_000)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-bash-tool".to_string(),
                        role: MessageRole::Assistant,
                        content: String::new(),
                        content_blocks: Vec::new(),
                        tool_calls: Some(vec![ToolCall::with_default_weles_review(
                            "call-bash-1".to_string(),
                            ToolFunction {
                                name: zorai_protocol::tool_names::BASH_COMMAND.to_string(),
                                arguments: r#"{"command":"sleep 60"}"#.to_string(),
                            },
                        )]),
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
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(60_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::ToolCalls,
            "bash_command",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;

    assert!(
        observations.is_empty(),
        "stalled-turn recovery must not resume while tool_call_id call-bash-1 is awaiting a tool result"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_promise_without_action() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-promise-stall";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Promise stall".to_string(),
                messages: vec![
                    AgentMessage::user("Design the page", now.saturating_sub(61_000)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-stall".to_string(),
                        role: MessageRole::Assistant,
                        content: "Working. Let me draft the redesigned content now.".to_string(),
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
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(60_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(
        observations[0].class,
        StalledTurnClass::PromiseWithoutAction
    );
    assert_eq!(observations[0].stream_progress_kind, None);
}

#[tokio::test]
async fn collect_stalled_turn_observations_ignores_completed_spawned_task_thread() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-completed-spawned-worker";
    let task_id = "task-completed-spawned-worker";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(thread_id.to_string(), promise_thread(thread_id, now));
    }
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(spawned_task(thread_id, task_id, TaskStatus::Completed, now));
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations
            .iter()
            .all(|observation| observation.thread_id != thread_id),
        "completed spawned task threads must not be woken by stalled-turn recovery"
    );
}

#[tokio::test]
async fn supervise_stalled_turns_retries_with_internal_ping_and_continue() {
    let engine = build_test_engine("Recovered.").await;
    let mut events = engine.subscribe();
    let now = super::now_millis();
    let thread_id = "thread-recovery";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Recovery thread".to_string(),
                messages: vec![
                    AgentMessage::user("Make the redesign", now.saturating_sub(61_000)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-recovery".to_string(),
                        role: MessageRole::Assistant,
                        content: "Excellent. Let me start drafting the redesigned landing page."
                            .to_string(),
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
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(60_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should retry");

    let mut saw_reload = false;
    while let Ok(event) = events.try_recv() {
        if let crate::agent::types::AgentEvent::ThreadReloadRequired {
            thread_id: event_thread_id,
        } = event
        {
            if event_thread_id == thread_id {
                saw_reload = true;
                break;
            }
        }
    }
    assert!(
        saw_reload,
        "stalled-turn retry should emit thread reload so TUI refreshes the recovery message"
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("WELES stalled-turn recovery")
    }));
    assert!(
        !thread
            .messages
            .iter()
            .any(|message| { message.role == MessageRole::User && message.content == "continue" }),
        "stalled-turn retries should not append a synthetic user 'continue' message"
    );
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
    drop(threads);

    let dm_thread_id =
        crate::agent::agent_identity::internal_dm_thread_id(WELES_AGENT_ID, MAIN_AGENT_ID);
    let threads = engine.threads.read().await;
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("internal recovery DM should exist");
    assert_eq!(dm_thread.agent_name.as_deref(), Some(MAIN_AGENT_NAME));
    assert!(dm_thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
}

#[tokio::test]
async fn supervise_stalled_turns_retries_system_seeded_thread_without_user_message() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-recovery-system-seed";

    let mut seed = AgentMessage::user(
        "Goal thread seed: continue the current step",
        now.saturating_sub(61_000),
    );
    seed.role = MessageRole::System;
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Recovery thread".to_string(),
                messages: vec![
                    seed,
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-recovery-system-seed".to_string(),
                        role: MessageRole::Assistant,
                        content: "Excellent. Let me start drafting the redesigned landing page."
                            .to_string(),
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
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(60_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should retry without a prior user message");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("WELES stalled-turn recovery")
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_idle_active_reasoning_stream() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-idle-stream";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Idle stream".to_string(),
                messages: vec![AgentMessage::user("Keep going", now.saturating_sub(61_000))],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(61_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: CONCIERGE_AGENT_ID.to_string(),
                        agent_name: CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(60_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "thinking through the next step",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].class, StalledTurnClass::ActiveStreamIdle);
    assert_eq!(
        observations[0].stream_progress_kind,
        Some(StreamProgressKind::Reasoning)
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_idle_stream_for_cancelled_goal_thread() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-cancelled-goal-stream";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(thread_id.to_string(), promise_thread(thread_id, now));
    }
    engine
        .goal_runs
        .lock()
        .await
        .push_back(terminal_goal_run_for_thread(
            "goal-cancelled-stream",
            thread_id,
            now,
        ));

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "still thinking after operator stop",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "cancelled goal threads must not be recovered by stalled-turn supervision"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_idle_stream_for_persisted_cancelled_goal_thread_after_live_queue_clear(
) {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-persisted-cancelled-goal-stream";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(thread_id.to_string(), promise_thread(thread_id, now));
    }
    engine
        .goal_runs
        .lock()
        .await
        .push_back(terminal_goal_run_for_thread(
            "goal-persisted-cancelled-stream",
            thread_id,
            now,
        ));
    engine.persist_goal_runs().await;
    engine.goal_runs.lock().await.clear();

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "still thinking after persisted operator stop",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "persisted cancelled goal threads must not be recovered by stalled-turn supervision"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_skips_stream_awaiting_operator_question() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-awaiting-operator-question";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Awaiting operator question".to_string(),
                messages: vec![AgentMessage::user(
                    "Ask me if you need a decision",
                    now.saturating_sub(61_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::ToolCalls,
            zorai_protocol::tool_names::ASK_QUESTIONS,
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    let mut operator_events = engine.event_tx.subscribe();
    let question_engine = engine.clone();
    let question_thread_id = thread_id.to_string();
    let question_task = tokio::spawn(async move {
        question_engine
            .ask_operator_question(
                "Choose:\nA. Continue\nB. Stop",
                vec!["A".to_string(), "B".to_string()],
                Some("session-1".to_string()),
                Some(question_thread_id),
            )
            .await
    });
    let question_id =
        match tokio::time::timeout(std::time::Duration::from_secs(2), operator_events.recv())
            .await
            .expect("operator question event should arrive promptly")
            .expect("operator question event")
        {
            crate::agent::AgentEvent::OperatorQuestion {
                question_id,
                thread_id: event_thread_id,
                ..
            } => {
                assert_eq!(event_thread_id.as_deref(), Some(thread_id));
                question_id
            }
            other => panic!("expected operator question event, got {other:?}"),
        };

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "pending operator questions should keep the stream out of stalled-turn recovery"
    );

    engine
        .answer_operator_question(&question_id, "A")
        .await
        .expect("operator answer should unblock the question");
    question_task
        .await
        .expect("question task should join")
        .expect("question task should succeed");
}

#[tokio::test]
async fn supervise_stalled_turns_recovers_idle_reasoning_stream_via_internal_dm() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-idle-stream-recovery";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Idle stream recovery".to_string(),
                messages: vec![AgentMessage::user(
                    "Continue the unfinished job",
                    now.saturating_sub(61_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(61_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: CONCIERGE_AGENT_ID.to_string(),
                        agent_name: CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: now.saturating_sub(60_000),
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let (generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
    engine
        .note_stream_progress(
            thread_id,
            generation,
            StreamProgressKind::Reasoning,
            "thinking through the next step",
        )
        .await;
    {
        let mut streams = engine.stream_cancellations.lock().await;
        let entry = streams
            .get_mut(thread_id)
            .expect("active stream entry should exist");
        entry.last_progress_at = now.saturating_sub(31_000);
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("idle active stream should recover");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message
                .content
                .contains("stream went idle before completion")
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));

    let dm_thread_id =
        crate::agent::agent_identity::internal_dm_thread_id(WELES_AGENT_ID, CONCIERGE_AGENT_ID);
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("recovery DM should exist");
    assert_eq!(dm_thread.agent_name.as_deref(), Some(CONCIERGE_AGENT_NAME));
    assert!(dm_thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant && message.content.contains("Recovered.")
    }));
}

#[tokio::test]
async fn supervise_stalled_turns_escalates_after_third_retry_window() {
    let engine = build_test_engine("Recovered.").await;
    let now = super::now_millis();
    let thread_id = "thread-escalate";
    let task_id = "task-escalate";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Escalation thread".to_string(),
                messages: vec![
                    AgentMessage::user("Do the work", now.saturating_sub(121_000)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-escalate".to_string(),
                        role: MessageRole::Assistant,
                        content: "Working. Let me draft the redesigned content now.".to_string(),
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
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(120_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now,
                updated_at: now,
            },
        );
    }
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Escalation task".to_string(),
            description: "Escalation task".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now.saturating_sub(180_000),
            started_at: Some(now.saturating_sub(180_000)),
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
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
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
    engine.persist_tasks().await;
    {
        let mut candidates = engine.stalled_turn_candidates.lock().await;
        let mut candidate =
            StalledTurnCandidate::new(thread_id, StalledTurnClass::PromiseWithoutAction, now);
        candidate.last_message_id = "assistant-escalate".to_string();
        candidate.last_message_at = now.saturating_sub(300_000);
        candidate.last_message_excerpt =
            "Working. Let me draft the redesigned content now.".to_string();
        candidate.task_id = Some(task_id.to_string());
        candidate.retries_sent = 3;
        candidate.next_evaluation_at = now.saturating_sub(1_000);
        candidates.insert(thread_id.to_string(), candidate);
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should escalate");

    let tasks = engine.tasks.lock().await;
    let task = tasks
        .iter()
        .find(|task| task.id == task_id)
        .expect("task should remain present");
    assert_eq!(task.blocked_reason.as_deref(), Some("stuck_needs_recovery"));
}

#[tokio::test]
async fn stalled_turn_escalation_updates_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine =
        crate::agent::AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let now = super::now_millis();
    let thread_id = "thread-persisted-escalation";
    let task = engine
        .enqueue_task(
            "Persisted escalation task".to_string(),
            "Mark the persisted row as needing recovery.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.tasks.lock().await.clear();

    let mut candidate =
        StalledTurnCandidate::new(thread_id, StalledTurnClass::PromiseWithoutAction, now);
    candidate.last_message_excerpt = "Working. Let me finish this now.".to_string();
    candidate.task_id = Some(task.id.clone());
    candidate.retries_sent = 3;

    engine.perform_stalled_turn_escalation(&candidate).await;

    let updated = engine
        .list_tasks_filtered(&crate::history::AgentTaskListQuery {
            id: Some(task.id.clone()),
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
        .expect("persisted task should remain queryable");

    assert_eq!(
        updated.blocked_reason.as_deref(),
        Some("stuck_needs_recovery")
    );
    assert!(
        updated.logs.iter().any(|entry| {
            entry.phase == "stalled-turn-recovery"
                && entry
                    .message
                    .contains("automatic stalled-turn recovery exhausted")
        }),
        "persisted task should record the escalation log"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_recent_subagent_tool_loop() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-tool-loop";
    let task_id = "task-subagent-tool-loop";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "Keep working until the bug is fixed.",
                    now.saturating_sub(120_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(120_000),
                updated_at: now.saturating_sub(60_000),
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Spawned worker".to_string(),
            description: "Fix the stalled worker".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now.saturating_sub(120_000),
            started_at: Some(now.saturating_sub(120_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
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

    let task = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .expect("subagent task should exist")
    };
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::READ_FILE,
            false,
            100,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::SEARCH_FILES,
            false,
            110,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::READ_FILE,
            false,
            120,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::SEARCH_FILES,
            false,
            130,
        )
        .await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after tool loop");
        stats.last_tool_call_at = Some(now.saturating_sub(31_000));
        stats.last_progress_at = Some(now.saturating_sub(31_000));
        stats.updated_at = now.saturating_sub(31_000);
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].class, StalledTurnClass::ToolCallLoop);
    assert_eq!(observations[0].task_id.as_deref(), Some(task_id));
}

#[tokio::test]
async fn collect_stalled_turn_observations_ignores_threads_inactive_for_over_24_hours() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let day_ms = 24 * 60 * 60 * 1000;
    let thread_id = "thread-subagent-too-old";
    let task_id = "task-subagent-too-old";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Old spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "This request is stale now.",
                    now.saturating_sub(day_ms + 60_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(day_ms + 120_000),
                updated_at: now.saturating_sub(day_ms + 60_000),
            },
        );
    }

    let task = AgentTask {
        id: task_id.to_string(),
        title: "Old spawned worker".to_string(),
        description: "Too old for stalled-turn retry".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now.saturating_sub(day_ms + 120_000),
        started_at: Some(now.saturating_sub(day_ms + 120_000)),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
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
    };
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(day_ms + 120_000);
        stats.updated_at = now.saturating_sub(day_ms + 60_000);
        stats.last_tool_call_at = Some(now.saturating_sub(day_ms + 60_000));
        stats.last_progress_at = Some(now.saturating_sub(day_ms + 60_000));
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "threads inactive for more than 24 hours should be ignored by stalled-turn"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_uses_configured_restore_window() {
    let engine = build_test_engine("Acknowledged.").await;
    {
        let mut config = engine.config.write().await;
        config.participant_observer_restore_window_hours = 1;
    }
    let now = super::now_millis();
    let two_hours_ms = 2 * 60 * 60 * 1000;
    let thread_id = "thread-subagent-outside-config-window";
    let task_id = "task-subagent-outside-config-window";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Outside configured window".to_string(),
                messages: vec![AgentMessage::user(
                    "This request is outside the restore window.",
                    now.saturating_sub(two_hours_ms),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(two_hours_ms + 60_000),
                updated_at: now.saturating_sub(two_hours_ms),
            },
        );
    }

    let mut task = spawned_task(thread_id, task_id, TaskStatus::InProgress, now);
    task.title = "Outside configured window".to_string();
    task.description = "Too old for configured stalled-turn retry".to_string();
    task.created_at = now.saturating_sub(two_hours_ms + 60_000);
    task.started_at = Some(now.saturating_sub(two_hours_ms + 60_000));
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(two_hours_ms + 60_000);
        stats.updated_at = now.saturating_sub(two_hours_ms);
        stats.last_tool_call_at = Some(now.saturating_sub(two_hours_ms));
        stats.last_progress_at = Some(now.saturating_sub(two_hours_ms));
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "stalled-turn supervision must honor the configured restore window"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_recent_subagent_no_progress() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-no-progress";
    let task_id = "task-subagent-no-progress";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "Keep working until it is fixed.",
                    now.saturating_sub(600_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(600_000),
                updated_at: now.saturating_sub(31_000),
            },
        );
    }

    let task = AgentTask {
        id: task_id.to_string(),
        title: "Spawned worker".to_string(),
        description: "Waiting with no progress".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now.saturating_sub(600_000),
        started_at: Some(now.saturating_sub(600_000)),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
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
    };
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(600_000);
        stats.updated_at = now.saturating_sub(31_000);
        stats.last_tool_call_at = Some(now.saturating_sub(31_000));
        stats.last_progress_at = None;
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].class, StalledTurnClass::NoProgress);
    assert_eq!(observations[0].task_id.as_deref(), Some(task_id));
}

#[tokio::test]
async fn collect_stalled_turn_observations_treats_reasoning_only_assistant_turn_as_recent_progress()
{
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-reasoning-only-done";
    let task_id = "task-subagent-reasoning-only-done";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![
                    AgentMessage::user("Check the issue.", now.saturating_sub(600_000)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-reasoning-only".to_string(),
                        role: MessageRole::Assistant,
                        content: String::new(),
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
                        reasoning: Some("final reasoning".to_string()),
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(31_000),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(600_000),
                updated_at: now.saturating_sub(31_000),
            },
        );
    }

    let task = AgentTask {
        id: task_id.to_string(),
        title: "Spawned worker".to_string(),
        description: "Recently returned only reasoning".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now.saturating_sub(600_000),
        started_at: Some(now.saturating_sub(600_000)),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
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
    };
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(600_000);
        stats.updated_at = now.saturating_sub(31_000);
        stats.last_tool_call_at = None;
        stats.last_progress_at = None;
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert!(
        observations.is_empty(),
        "recent reasoning-only assistant completion should refresh runtime progress fallback"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_primary_thread_empty_after_tool_result() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-primary-empty-after-tool-result";

    let mut tool_message =
        AgentMessage::user("Created file /tmp/output.md.", now.saturating_sub(60_000));
    tool_message.id = "tool-result-1".to_string();
    tool_message.role = MessageRole::Tool;
    tool_message.tool_call_id = Some("call-1".to_string());
    tool_message.tool_name = Some("write_file".to_string());

    let assistant_with_tool_call = AgentMessage {
        reasoning_tokens: None,
        thinking_ms: None,
        id: "assistant-tool-call".to_string(),
        role: MessageRole::Assistant,
        content: String::new(),
        content_blocks: Vec::new(),
        tool_calls: Some(vec![ToolCall {
            id: "call-1".to_string(),
            function: ToolFunction {
                name: "write_file".to_string(),
                arguments: "{}".to_string(),
            },
            weles_review: None,
        }]),
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
        message_kind: crate::agent::types::AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp: now.saturating_sub(120_000),
        feedback: None,
    };

    let empty_assistant_after_tool_result = AgentMessage {
        reasoning_tokens: None,
        thinking_ms: None,
        id: "assistant-empty-after-tool".to_string(),
        role: MessageRole::Assistant,
        content: String::new(),
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
        reasoning: Some("brief".to_string()),
        message_kind: crate::agent::types::AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp: now.saturating_sub(360_000),
        feedback: None,
    };

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("DeepSeekorrr".to_string()),
                title: "Primary chat with empty turn after tool result".to_string(),
                messages: vec![
                    AgentMessage::user("Continue the project.", now.saturating_sub(180_000)),
                    assistant_with_tool_call,
                    tool_message,
                    empty_assistant_after_tool_result,
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(180_000),
                updated_at: now.saturating_sub(360_000),
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(
        observations.len(),
        1,
        "primary thread that ends with empty assistant content after a tool result must be detected as stalled"
    );
    assert_eq!(
        observations[0].class,
        StalledTurnClass::PostToolResultNoFollowThrough
    );
    assert_eq!(observations[0].thread_id, thread_id);
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_stall_behind_background_completion_notice() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-stall-behind-background-notice";

    let mut tool_message =
        AgentMessage::user("Created file /tmp/output.md.", now.saturating_sub(60_000));
    tool_message.id = "tool-result-1".to_string();
    tool_message.role = MessageRole::Tool;
    tool_message.tool_call_id = Some("call-1".to_string());
    tool_message.tool_name = Some("write_file".to_string());

    let assistant_with_tool_call = AgentMessage {
        reasoning_tokens: None,
        thinking_ms: None,
        id: "assistant-tool-call".to_string(),
        role: MessageRole::Assistant,
        content: String::new(),
        content_blocks: Vec::new(),
        tool_calls: Some(vec![ToolCall {
            id: "call-1".to_string(),
            function: ToolFunction {
                name: "write_file".to_string(),
                arguments: "{}".to_string(),
            },
            weles_review: None,
        }]),
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
        message_kind: crate::agent::types::AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp: now.saturating_sub(120_000),
        feedback: None,
    };

    let mut empty_assistant_after_tool_result =
        AgentMessage::user(String::new(), now.saturating_sub(360_000));
    empty_assistant_after_tool_result.id = "assistant-empty-after-tool".to_string();
    empty_assistant_after_tool_result.role = MessageRole::Assistant;
    empty_assistant_after_tool_result.reasoning = Some("brief".to_string());

    let mut background_completion_notice = AgentMessage::user(
        "Background operation finished.\n\nOperation result saved to file.",
        now.saturating_sub(30_000),
    );
    background_completion_notice.id = "background-completion-notice".to_string();
    background_completion_notice.role = MessageRole::System;

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("DeepSeekorrr".to_string()),
                title: "Primary chat stalled behind background completion notice".to_string(),
                messages: vec![
                    AgentMessage::user("Continue the project.", now.saturating_sub(180_000)),
                    assistant_with_tool_call,
                    tool_message,
                    empty_assistant_after_tool_result,
                    background_completion_notice,
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(180_000),
                updated_at: now.saturating_sub(30_000),
            },
        );
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(
        observations.len(),
        1,
        "a trailing background-completion system notice must not hide a stalled turn from recovery, since it no longer wakes the agent on its own"
    );
    assert_eq!(
        observations[0].class,
        StalledTurnClass::PostToolResultNoFollowThrough
    );
    assert_eq!(observations[0].thread_id, thread_id);
    assert_eq!(
        observations[0].last_message_id,
        "assistant-empty-after-tool"
    );
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_subagent_stuck_on_empty_first_turn() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-empty-first-turn";
    let task_id = "task-subagent-empty-first-turn";
    let started_ago_ms: u64 = 900_000;
    let empty_message_ago_ms: u64 = 360_000;

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![
                    AgentMessage::user("Check the issue.", now.saturating_sub(started_ago_ms)),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-empty-first-turn".to_string(),
                        role: MessageRole::Assistant,
                        content: String::new(),
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
                        reasoning: Some("final reasoning".to_string()),
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(empty_message_ago_ms),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(started_ago_ms),
                updated_at: now.saturating_sub(empty_message_ago_ms),
            },
        );
    }

    let task = AgentTask {
        id: task_id.to_string(),
        title: "Spawned worker".to_string(),
        description: "Stalled on first turn with empty/reasoning-only assistant message"
            .to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now.saturating_sub(started_ago_ms),
        started_at: Some(now.saturating_sub(started_ago_ms)),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
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
    };
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(started_ago_ms);
        stats.updated_at = now.saturating_sub(empty_message_ago_ms);
        stats.last_tool_call_at = None;
        stats.last_progress_at = None;
    }

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(
        observations.len(),
        1,
        "subagent stalled on first turn with empty/reasoning-only message for {}s must be detected as NoProgress",
        empty_message_ago_ms / 1000,
    );
    assert_eq!(observations[0].class, StalledTurnClass::NoProgress);
    assert_eq!(observations[0].task_id.as_deref(), Some(task_id));
}

#[tokio::test]
async fn collect_stalled_turn_observations_detects_recent_subagent_no_progress_without_runtime() {
    let engine = build_test_engine("Acknowledged.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-no-runtime";
    let task_id = "task-subagent-no-runtime";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "Continue until the issue is resolved.",
                    now.saturating_sub(601_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(601_000),
                updated_at: now.saturating_sub(601_000),
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Spawned worker".to_string(),
            description: "Lost runtime state".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now.saturating_sub(601_000),
            started_at: Some(now.saturating_sub(601_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
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
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    assert!(
        engine.subagent_runtime.read().await.get(task_id).is_none(),
        "test requires missing live subagent runtime"
    );

    let observations = engine.collect_stalled_turn_observations().await;
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].class, StalledTurnClass::NoProgress);
    assert_eq!(observations[0].task_id.as_deref(), Some(task_id));
}

#[tokio::test]
async fn supervise_stalled_turns_recovers_recent_subagent_tool_loop_via_task_retry() {
    let engine = build_test_engine("Recovered child thread.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-recovery";
    let task_id = "task-subagent-recovery";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "Investigate the regression and continue until fixed.",
                    now.saturating_sub(120_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(120_000),
                updated_at: now.saturating_sub(60_000),
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Spawned worker".to_string(),
            description: "Investigate the regression".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now.saturating_sub(120_000),
            started_at: Some(now.saturating_sub(120_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
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
            override_system_prompt: Some(
                "Agent persona: Dazhbog\nAgent persona id: dazhbog\nTask-owned builtin persona."
                    .to_string(),
            ),
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        });
    }

    let task = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .expect("subagent task should exist")
    };
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::READ_FILE,
            false,
            100,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::SEARCH_FILES,
            false,
            110,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::READ_FILE,
            false,
            120,
        )
        .await;
    engine
        .record_subagent_tool_result(
            &task,
            thread_id,
            zorai_protocol::tool_names::SEARCH_FILES,
            false,
            130,
        )
        .await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after tool loop");
        stats.last_tool_call_at = Some(now.saturating_sub(31_000));
        stats.last_progress_at = Some(now.saturating_sub(31_000));
        stats.updated_at = now.saturating_sub(31_000);
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should recover subagent thread");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("subagent thread should remain present");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("WELES stalled-turn recovery")
            && message.content.contains("tool loop")
    }));
    assert!(
        !thread
            .messages
            .iter()
            .any(|message| message.role == MessageRole::User && message.content == "continue"),
        "stalled-turn retries should not append a synthetic visible continue turn"
    );
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message.content.contains("Recovered child thread.")
    }));
    let dm_thread_id =
        crate::agent::agent_identity::internal_dm_thread_id(WELES_AGENT_ID, "dazhbog");
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("stalled-turn recovery should notify the owning subagent");
    assert_eq!(
        dm_thread.agent_name.as_deref(),
        Some(crate::agent::agent_identity::DAZHBOG_AGENT_NAME)
    );
}

#[tokio::test]
async fn supervise_stalled_turns_recovers_recent_subagent_no_progress_without_runtime() {
    let engine = build_test_engine("Recovered missing runtime child thread.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-recovery-no-runtime";
    let task_id = "task-subagent-recovery-no-runtime";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![AgentMessage::user(
                    "Keep going until it is fixed.",
                    now.saturating_sub(601_000),
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(601_000),
                updated_at: now.saturating_sub(601_000),
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(AgentTask {
            id: task_id.to_string(),
            title: "Spawned worker".to_string(),
            description: "Recover without runtime state".to_string(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: now.saturating_sub(601_000),
            started_at: Some(now.saturating_sub(601_000)),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
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

    assert!(
        engine.subagent_runtime.read().await.get(task_id).is_none(),
        "test requires missing live subagent runtime"
    );

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should recover subagent thread without runtime");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("subagent thread should remain present");
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::System
            && message.content.contains("appears stalled with no progress")
    }));
    assert!(thread.messages.iter().any(|message| {
        message.role == MessageRole::Assistant
            && message
                .content
                .contains("Recovered missing runtime child thread.")
    }));
}

#[tokio::test]
async fn supervise_stalled_turns_recovers_subagent_stuck_on_empty_first_turn() {
    let engine = build_test_engine("Recovered stalled empty-turn child.").await;
    let now = super::now_millis();
    let thread_id = "thread-subagent-empty-first-turn-recovery";
    let task_id = "task-subagent-empty-first-turn-recovery";
    let started_ago_ms: u64 = 900_000;
    let empty_message_ago_ms: u64 = 360_000;

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("Dazhbog".to_string()),
                title: "Spawned worker".to_string(),
                messages: vec![
                    AgentMessage::user(
                        "Investigate the regression and report back.",
                        now.saturating_sub(started_ago_ms),
                    ),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-empty-first-turn-recovery".to_string(),
                        role: MessageRole::Assistant,
                        content: String::new(),
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
                        reasoning: Some("final reasoning".to_string()),
                        message_kind: crate::agent::types::AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        tool_output_preview_path: None,
                        structural_refs: Vec::new(),
                        pinned_for_compaction: false,
                        timestamp: now.saturating_sub(empty_message_ago_ms),
                        feedback: None,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: now.saturating_sub(started_ago_ms),
                updated_at: now.saturating_sub(empty_message_ago_ms),
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    let task = AgentTask {
        id: task_id.to_string(),
        title: "Spawned worker".to_string(),
        description: "Stalled on first turn with empty/reasoning-only assistant message"
            .to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now.saturating_sub(started_ago_ms),
        started_at: Some(now.saturating_sub(started_ago_ms)),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
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
    };
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(task.clone());
    }
    engine.ensure_subagent_runtime(&task, Some(thread_id)).await;
    {
        let mut runtime = engine.subagent_runtime.write().await;
        let stats = runtime
            .get_mut(task_id)
            .expect("subagent runtime should exist after initialization");
        stats.started_at = now.saturating_sub(started_ago_ms);
        stats.updated_at = now.saturating_sub(empty_message_ago_ms);
        stats.last_tool_call_at = None;
        stats.last_progress_at = None;
    }

    engine
        .supervise_stalled_turns()
        .await
        .expect("stalled-turn supervision should recover empty-first-turn subagent thread");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("subagent thread should remain present");
    assert!(
        thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message.content.contains("WELES stalled-turn recovery")
        }),
        "WELES recovery system message must be appended to the stalled subagent thread"
    );
}
