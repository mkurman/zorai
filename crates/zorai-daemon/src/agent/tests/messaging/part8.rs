use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};
use zorai_shared::providers::PROVIDER_ID_OPENAI;

async fn make_runner_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");
    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        reqwest::Client::new(),
    );
    (engine, temp_dir)
}

async fn seed_queued_participant_suggestion(
    engine: &Arc<AgentEngine>,
    thread_id: &str,
    target_agent_id: &str,
    target_agent_name: &str,
    instruction: &str,
) -> ThreadParticipantSuggestion {
    let suggestion = ThreadParticipantSuggestion {
        id: format!("seeded-{thread_id}"),
        target_agent_id: target_agent_id.to_string(),
        target_agent_name: target_agent_name.to_string(),
        instruction: instruction.to_string(),
        suggestion_kind: ThreadParticipantSuggestionKind::PreparedMessage,
        force_send: false,
        status: ThreadParticipantSuggestionStatus::Queued,
        created_at: 1,
        updated_at: 1,
        auto_send_at: None,
        source_message_timestamp: None,
        error: None,
    };
    engine
        .thread_participant_suggestions
        .write()
        .await
        .insert(thread_id.to_string(), vec![suggestion.clone()]);
    engine.persist_thread_by_id(thread_id).await;
    suggestion
}

#[tokio::test]
async fn request_thread_auto_response_suggestion_queues_for_requested_participant() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_auto_response_requested_for_main_reply";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Auto response queue".to_string(),
            messages: vec![
                AgentMessage::user("keep going", 1),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "I finished the current slice and the next likely step is validating the migration.".to_string(),
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
                    timestamp: 30,
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
            created_at: 1,
            updated_at: 30,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("weles participant should register");
    engine
        .upsert_thread_participant(thread_id, "domowoj", "push the work forward")
        .await
        .expect("domowoj participant should register");

    {
        let mut participants = engine.thread_participants.write().await;
        let entry = participants
            .get_mut(thread_id)
            .expect("thread participants should exist");
        entry
            .iter_mut()
            .find(|participant| participant.agent_id == "weles")
            .expect("weles should exist")
            .last_contribution_at = Some(10);
        entry
            .iter_mut()
            .find(|participant| participant.agent_id == "domowoj")
            .expect("domowoj should exist")
            .last_contribution_at = Some(20);
    }
    engine.persist_thread_by_id(thread_id).await;

    let queued = engine
        .request_thread_auto_response_suggestion(thread_id, "weles")
        .await
        .expect("explicit auto-response request should succeed");
    assert!(
        queued,
        "request should queue a new auto-response suggestion"
    );

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(
        suggestions.len(),
        1,
        "exactly one auto response should be queued"
    );
    let suggestion = &suggestions[0];
    assert_eq!(
        suggestion.suggestion_kind,
        ThreadParticipantSuggestionKind::AutoResponse
    );
    assert_eq!(suggestion.target_agent_id, "weles");
    assert_eq!(suggestion.target_agent_name, "Weles");
    assert_eq!(suggestion.source_message_timestamp, Some(30));
    assert!(
        suggestion.auto_send_at.is_some(),
        "auto response should carry a countdown deadline"
    );
    assert_eq!(
        suggestion.instruction, "Gateway reply ok",
        "the queue should show the drafted reply, not the prompt that asked for one"
    );

    let participants = engine.list_thread_participants(thread_id).await;
    let weles = participants
        .iter()
        .find(|participant| participant.agent_id == "weles")
        .expect("weles should still be registered");
    let domowoj = participants
        .iter()
        .find(|participant| participant.agent_id == "domowoj")
        .expect("domowoj should still be registered");
    assert_eq!(weles.last_observed_visible_message_at, Some(30));
    assert_eq!(
        domowoj.last_observed_visible_message_at, None,
        "only the requested participant should be drafted"
    );
}

#[tokio::test]
async fn participant_send_now_posts_direct_visible_message_then_continues_thread() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_send_direct_post";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant direct post".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    let suggestion = seed_queued_participant_suggestion(
        &engine,
        thread_id,
        "weles",
        "Weles",
        "Participant verdict: claim is false.",
    )
    .await;

    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send suggestion");

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist after participant send-now")
            .messages
            .clone()
    };
    let assistant = thread_messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("send-now should append a visible participant-authored message");

    assert_eq!(assistant.content, "Participant verdict: claim is false.");
    assert_eq!(assistant.author_agent_name.as_deref(), Some("Weles"));
    assert_eq!(assistant.provider, None);
    assert_eq!(assistant.model, None);
    assert_eq!(
        thread_messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "send-now should not add a fresh user turn"
    );
    let follow_up = thread_messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("send-now should trigger a separate active-agent follow-up");
    assert_eq!(follow_up.content, "Gateway reply ok");

    assert_eq!(
        recorded_bodies.lock().expect("lock request log").len(),
        1,
        "send-now should wake the active thread agent once after the direct participant post"
    );
    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        "weles",
    );
    assert!(
        engine.get_thread(&dm_thread_id).await.is_none(),
        "send-now should not create a hidden Weles DM thread"
    );
}

#[tokio::test]
async fn participant_auto_response_send_posts_visible_message_then_continues_thread() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_auto_response_direct_post";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant auto response direct post".to_string(),
            messages: vec![
                AgentMessage::user("hello", 1),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "Main agent reply".to_string(),
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
                    timestamp: 30,
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
            created_at: 1,
            updated_at: 30,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    let suggestion = ThreadParticipantSuggestion {
        id: format!("auto-response-{thread_id}"),
        target_agent_id: "weles".to_string(),
        target_agent_name: "Weles".to_string(),
        instruction: "Respond to the latest main agent message.".to_string(),
        suggestion_kind: ThreadParticipantSuggestionKind::AutoResponse,
        force_send: false,
        status: ThreadParticipantSuggestionStatus::Queued,
        created_at: 31,
        updated_at: 31,
        auto_send_at: Some(31),
        source_message_timestamp: Some(30),
        error: None,
    };
    engine
        .thread_participant_suggestions
        .write()
        .await
        .insert(thread_id.to_string(), vec![suggestion.clone()]);
    engine.persist_thread_by_id(thread_id).await;

    recorded_bodies.lock().expect("lock request log").clear();

    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send auto-response suggestion");

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        request_bodies.len(),
        2,
        "auto-response send should trigger one participant turn and one main-agent continuation"
    );

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist after auto-response send-now")
            .messages
            .clone()
    };
    let participant_idx = thread_messages
        .iter()
        .position(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("auto-response should append a visible participant-authored message");
    let follow_up = thread_messages[participant_idx + 1..]
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("auto-response send-now should trigger a separate active-agent follow-up");
    assert_eq!(follow_up.content, "Gateway reply ok");
}

#[tokio::test]
async fn participant_post_resumes_main_agent_with_participant_aware_continuation_prompt() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_aware_continuation_prompt";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant continuation prompt".to_string(),
            messages: vec![AgentMessage::user("Implement the fix and keep going", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    engine
        .append_visible_thread_participant_message(
            thread_id,
            "weles",
            "Participant verdict: claim is false.",
        )
        .await
        .expect("participant message should append");

    recorded_bodies.lock().expect("lock request log").clear();

    engine
        .continue_thread_after_participant_post_or_notice(thread_id)
        .await;

    let continuation_request = recorded_bodies
        .lock()
        .expect("lock request log")
        .front()
        .cloned()
        .expect("continuation request body should be recorded");
    assert!(
        continuation_request
            .contains("A thread participant (Weles) just posted a visible message."),
        "main-agent continuation should explicitly identify the participant contribution as the latest actionable context"
    );
    assert!(
        continuation_request.contains("Latest participant contribution:"),
        "main-agent continuation should label the participant contribution in the resume prompt"
    );
    assert!(
        continuation_request.contains("Participant verdict: claim is false."),
        "main-agent continuation should include the latest participant message in the resume prompt"
    );
    assert!(
        continuation_request.contains("prefer `spawn_subagent`"),
        "participant follow-up continuation should steer visible follow-up toward spawned subagents instead of hidden DMs"
    );
    assert!(
        continuation_request.contains("Do not stop after only sending `message_agent`"),
        "participant follow-up continuation should explicitly forbid DM-only follow-up"
    );
}

#[tokio::test]
async fn participant_follow_up_and_later_turn_persist_across_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_participant_reload_persistence";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant reload persistence".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    let suggestion = seed_queued_participant_suggestion(
        &engine,
        thread_id,
        "weles",
        "Weles",
        "Participant verdict: claim is false.",
    )
    .await;

    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send suggestion");

    engine
        .resend_existing_user_message(thread_id, "hello")
        .await
        .expect("later resend should succeed");

    let live_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist")
            .messages
            .clone()
    };
    assert!(
        live_messages.len() >= 4,
        "expected user, participant post, participant follow-up, and a later main-agent turn"
    );

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    let reloaded_messages = reloaded
        .get_thread(thread_id)
        .await
        .expect("reloaded thread should exist")
        .messages;

    assert_eq!(reloaded_messages.len(), live_messages.len());
    assert_eq!(
        reloaded_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        live_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reloaded_messages
            .last()
            .and_then(|message| message.author_agent_id.as_deref()),
        live_messages
            .last()
            .and_then(|message| message.author_agent_id.as_deref())
    );
}

#[tokio::test]
async fn participant_post_continuation_does_not_rerun_same_participant_after_main_agent_follow_up()
{
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant continuation observer server");
    let addr = listener
        .local_addr()
        .expect("participant continuation observer addr");
    let request_counter = Arc::new(AtomicUsize::new(0));

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        let request_counter = request_counter.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                let request_counter = request_counter.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read participant continuation observer request");
                    recorded_bodies
                        .lock()
                        .expect("lock request log")
                        .push_back(body.clone());

                    let request_index = request_counter.fetch_add(1, Ordering::SeqCst);
                    let response = match request_index {
                        0 => concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Gateway reply ok\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                        _ => concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"NO_SUGGESTION\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                    };
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write participant continuation observer response");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_post_continuation_observer_rerun";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant continuation observer rerun".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    let suggestion = seed_queued_participant_suggestion(
        &engine,
        thread_id,
        "weles",
        "Weles",
        "Participant verdict: claim is false.",
    )
    .await;

    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send initial participant suggestion");

    let thread_messages = engine
        .get_thread(thread_id)
        .await
        .expect("thread should still exist")
        .messages;
    assert!(
        thread_messages.iter().all(|message| {
            message.author_agent_id.as_deref() != Some("weles")
                || message.content != "Second participant follow-up."
        }),
        "participant-triggered main-agent continuations should not recursively rerun the same participant observer without new visible input"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "participant-triggered continuation should not leave behind queued observer work"
    );

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        request_bodies.len(),
        1,
        "participant-triggered continuation should stop after the single main-agent follow-up instead of re-entering participant observer loops"
    );
}

#[tokio::test]
async fn queued_participant_suggestion_auto_sends_after_active_stream_finishes() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind queued participant server");
    let addr = listener.local_addr().expect("queued participant addr");
    let request_counter = Arc::new(AtomicUsize::new(0));
    let first_request_started = Arc::new(tokio::sync::Notify::new());
    let release_first_response = Arc::new(tokio::sync::Notify::new());
    let request_counter_task = request_counter.clone();
    let first_request_started_task = first_request_started.clone();
    let release_first_response_task = release_first_response.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let body = read_http_request_body(&mut socket)
                .await
                .expect("read queued participant request");
            let request_index = request_counter_task.fetch_add(1, Ordering::SeqCst);
            let response_body = match request_index {
                0 => {
                    first_request_started_task.notify_waiters();
                    release_first_response_task.notified().await;
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Main reply before participant note.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":5}}\n\n",
                        "data: [DONE]\n\n"
                    )
                }
                _ if body.contains("Role: participant observer") => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"NO_SUGGESTION\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
                    "data: [DONE]\n\n"
                ),
                1 => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Swarozyc followed the queued participant note.\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":6}}\n\n",
                    "data: [DONE]\n\n"
                ),
                _ => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"unexpected extra request\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                    "data: [DONE]\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write queued participant response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = Arc::new(AgentEngine::new_test(manager, config, root.path()).await);
    let thread_id = "thread_participant_queue_auto_send";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant queue auto send".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .resend_existing_user_message(thread_id, "hello")
                .await
        }
    });

    timeout(Duration::from_secs(1), first_request_started.notified())
        .await
        .expect("initial resend should start");
    timeout(Duration::from_secs(1), async {
        loop {
            let streams = engine.stream_cancellations.lock().await;
            if streams.contains_key(thread_id) {
                break;
            }
            drop(streams);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("active stream should be registered");

    let queued = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Queued participant note.", false)
        .await
        .expect("queue participant suggestion while stream is active");
    assert_eq!(queued.status, ThreadParticipantSuggestionStatus::Queued);

    release_first_response.notify_waiters();

    timeout(Duration::from_secs(2), send_task)
        .await
        .expect("resend plus queued follow-up should finish")
        .expect("join resend task")
        .expect("resend plus queued follow-up should succeed");

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist")
            .messages
            .clone()
    };
    let participant_idx = thread_messages
        .iter()
        .position(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
                && message.content == "Queued participant note."
        })
        .expect("queued participant note should auto-post once the stream finishes");
    let follow_up = thread_messages[participant_idx + 1..]
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("queued participant note should wake the main agent once");
    assert_eq!(
        follow_up.content,
        "Swarozyc followed the queued participant note."
    );
    assert_eq!(
        request_counter.load(Ordering::SeqCst),
        2,
        "expected one main resend and one follow-up after auto-sending the queued participant note"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "auto-sent queued participant suggestion should be dismissed from the queue"
    );
}

#[tokio::test]
async fn queued_participant_suggestion_auto_sends_when_thread_is_already_idle() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_queue_auto_send_idle";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant queue auto send idle".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    engine
        .queue_thread_participant_suggestion(
            thread_id,
            "weles",
            "Late idle participant note.",
            false,
        )
        .await
        .expect("queue participant suggestion after thread is already idle");

    timeout(Duration::from_secs(2), async {
        loop {
            let thread_messages = {
                let threads = engine.threads.read().await;
                threads
                    .get(thread_id)
                    .expect("thread should still exist")
                    .messages
                    .clone()
            };
            let has_participant_note = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() == Some("weles")
                    && message.content == "Late idle participant note."
            });
            let has_main_follow_up = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() != Some("weles")
                    && message.content == "Gateway reply ok"
            });
            if has_participant_note && has_main_follow_up {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect(
        "queued participant note should auto-send even when it arrives after the thread is idle",
    );

    assert_eq!(
        recorded_bodies.lock().expect("lock request log").len(),
        1,
        "idle auto-send should wake the main thread agent once"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "idle auto-sent suggestion should not remain queued"
    );
}

#[tokio::test]
async fn hydrated_idle_participant_auto_send_is_visible_in_thread_detail() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url.clone();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_participant_queue_auto_send_idle_rehydrated";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant queue auto send idle hydrated".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register before restart");
    engine.persist_thread_by_id(thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    reloaded
        .queue_thread_participant_suggestion(
            thread_id,
            "weles",
            "Hydrated idle participant note.",
            false,
        )
        .await
        .expect("queue participant suggestion after hydrate");

    timeout(Duration::from_secs(2), async {
        loop {
            let thread_messages = {
                let threads = reloaded.threads.read().await;
                threads
                    .get(thread_id)
                    .expect("thread should still exist after hydrate")
                    .messages
                    .clone()
            };
            let has_participant_note = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() == Some("weles")
                    && message.content == "Hydrated idle participant note."
            });
            let has_main_follow_up = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() != Some("weles")
                    && message.content == "Gateway reply ok"
            });
            if has_participant_note && has_main_follow_up {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("participant note and follow-up should appear after hydrate");

    let thread_json = reloaded
        .agent_thread_detail_json(thread_id, None, None, false)
        .await;
    let detail: serde_json::Value =
        serde_json::from_str(&thread_json).expect("decode hydrated thread detail");
    let messages = detail
        .get("messages")
        .and_then(|entry| entry.as_array())
        .expect("thread detail should include messages");
    assert!(messages.iter().any(|message| {
        message.get("content").and_then(|entry| entry.as_str())
            == Some("Hydrated idle participant note.")
            && message
                .get("author_agent_id")
                .and_then(|entry| entry.as_str())
                == Some("weles")
    }));
    assert!(messages.iter().any(|message| {
        message.get("content").and_then(|entry| entry.as_str()) == Some("Gateway reply ok")
    }));
}

#[tokio::test]
async fn due_auto_response_is_not_sent_implicitly_after_restart() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url.clone();
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_hydrate_restores_due_auto_response";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrate due auto response".to_string(),
            messages: vec![
                AgentMessage::user("keep momentum", 1),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "I finished the patch and the next step is verifying the diff."
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register before restart");
    engine
        .thread_participant_suggestions
        .write()
        .await
        .insert(
            thread_id.to_string(),
            vec![ThreadParticipantSuggestion {
                id: "auto-response-1".to_string(),
                target_agent_id: "weles".to_string(),
                target_agent_name: "Weles".to_string(),
                instruction:
                    "Respond to the latest main agent message on this thread and continue the same workstream."
                        .to_string(),
                suggestion_kind: ThreadParticipantSuggestionKind::AutoResponse,
                force_send: false,
                status: ThreadParticipantSuggestionStatus::Queued,
                created_at: 2,
                updated_at: 2,
                auto_send_at: Some(1),
                source_message_timestamp: Some(2),
                error: None,
            }],
        );
    engine.persist_thread_by_id(thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate should succeed");
    let suggestions = reloaded
        .list_thread_participant_suggestions(thread_id)
        .await;
    assert_eq!(
        suggestions.len(),
        1,
        "hydrate should keep the queued suggestion"
    );
    assert_eq!(
        suggestions[0].id, "auto-response-1",
        "hydrate should preserve the due auto-response instead of sending it in the background"
    );

    let thread_messages = {
        let threads = reloaded.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist after hydrate")
            .messages
            .clone()
    };
    assert!(
        !thread_messages.iter().any(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        }),
        "hydrate should not send the participant reply before a TUI-opened thread accepts it"
    );
}

#[tokio::test]
async fn due_auto_response_does_not_background_send_when_thread_becomes_idle() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_due_auto_response_stays_queued";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Due auto response".to_string(),
            messages: vec![
                AgentMessage::user("keep momentum", 1),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "I finished the patch and the next step is verifying the diff."
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    engine
        .thread_participant_suggestions
        .write()
        .await
        .insert(
            thread_id.to_string(),
            vec![ThreadParticipantSuggestion {
                id: "auto-response-1".to_string(),
                target_agent_id: "weles".to_string(),
                target_agent_name: "Weles".to_string(),
                instruction:
                    "Respond to the latest main agent message on this thread and continue the same workstream."
                        .to_string(),
                suggestion_kind: ThreadParticipantSuggestionKind::AutoResponse,
                force_send: false,
                status: ThreadParticipantSuggestionStatus::Queued,
                created_at: 2,
                updated_at: 2,
                auto_send_at: Some(0),
                source_message_timestamp: Some(2),
                error: None,
            }],
        );

    let sent = engine
        .maybe_auto_send_next_thread_participant_suggestion(thread_id)
        .await
        .expect("idle drain should succeed");

    assert!(
        !sent,
        "due auto-response suggestions should stay queued until the open thread accepts them"
    );
    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(
        suggestions.len(),
        1,
        "due auto-response should remain queued"
    );
    assert_eq!(
        suggestions[0].id, "auto-response-1",
        "idle drain should not consume queued auto-response suggestions"
    );
}

#[tokio::test]
async fn hydrate_trims_persisted_participant_playground_threads_to_recent_tail() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_playground_hydrate_reset";
    let playground_thread_id =
        crate::agent::agent_identity::participant_playground_thread_id(thread_id, "weles");

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Visible thread for playground hydrate reset".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    let playground_messages = (0..120)
        .map(|index| AgentMessage {
            id: generate_message_id(),
            role: MessageRole::Assistant,
            content: format!("scratch reply {index}"),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 1,
            output_tokens: 2,
            cost: None,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: Some("weles".to_string()),
            author_agent_name: Some("Weles".to_string()),
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: (index + 2) as u64,
            feedback: None,
        })
        .collect::<Vec<_>>();
    engine.threads.write().await.insert(
        playground_thread_id.clone(),
        AgentThread {
            id: playground_thread_id.clone(),
            agent_name: Some("Weles".to_string()),
            title: "Participant playground".to_string(),
            total_input_tokens: 120,
            total_output_tokens: 240,
            messages: playground_messages,
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 2,
            updated_at: 121,
        },
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.persist_thread_by_id(&playground_thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    assert!(
        reloaded
            .thread_message_hydration_pending
            .read()
            .await
            .contains(&playground_thread_id),
        "hydrate trim should leave the full playground thread message load lazy"
    );
    assert!(
        reloaded
            .threads
            .read()
            .await
            .get(&playground_thread_id)
            .expect("playground shell should be hydrated")
            .messages
            .is_empty(),
        "hydrate trim should not load playground message bodies into memory"
    );

    let playground = reloaded
        .get_thread_filtered(&playground_thread_id, true, None, 0)
        .await
        .expect("playground thread should still exist after hydrate")
        .thread;
    assert_eq!(
        playground.messages.len(),
        100,
        "hydrate should retain a bounded recent playground tail instead of wiping all hidden context"
    );
    assert_eq!(
        playground
            .messages
            .first()
            .map(|message| message.content.as_str()),
        Some("scratch reply 20")
    );
    assert_eq!(
        playground
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("scratch reply 119")
    );
    assert_eq!(playground.total_input_tokens, 100);
    assert_eq!(playground.total_output_tokens, 200);
}

#[tokio::test]
async fn participant_failed_suggestions_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_failed_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant failed reload".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }
    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Check claim", false)
        .await
        .expect("queue suggestion");
    engine
        .fail_thread_participant_suggestion(thread_id, &suggestion.id, "provider unavailable")
        .await
        .expect("fail suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    let suggestions = reloaded
        .list_thread_participant_suggestions(thread_id)
        .await;
    assert_eq!(suggestions.len(), 1);
    assert_eq!(
        suggestions[0].status,
        ThreadParticipantSuggestionStatus::Failed
    );
    assert_eq!(
        suggestions[0].error.as_deref(),
        Some("provider unavailable")
    );
}

#[tokio::test]
async fn participant_dismissed_suggestions_do_not_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_dismiss_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant dismiss reload".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }
    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Check claim", false)
        .await
        .expect("queue suggestion");
    engine
        .dismiss_thread_participant_suggestion(thread_id, &suggestion.id)
        .await
        .expect("dismiss suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    assert!(reloaded
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_sent_suggestions_do_not_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_send_reload\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Sent suggestion\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_send_reload\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":2},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_send_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant send reload".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");
    let suggestion = ThreadParticipantSuggestion {
        id: "sent-suggestion".to_string(),
        target_agent_id: "weles".to_string(),
        target_agent_name: "Weles".to_string(),
        instruction: "Check claim".to_string(),
        suggestion_kind: ThreadParticipantSuggestionKind::PreparedMessage,
        force_send: false,
        status: ThreadParticipantSuggestionStatus::Queued,
        created_at: 1,
        updated_at: 1,
        auto_send_at: None,
        source_message_timestamp: None,
        error: None,
    };
    engine
        .thread_participant_suggestions
        .write()
        .await
        .insert(thread_id.to_string(), vec![suggestion.clone()]);
    engine.persist_thread_by_id(thread_id).await;
    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    assert!(reloaded
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_observer_failure_keeps_outer_send_successful() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant observer failure server");
    let addr = listener.local_addr().expect("observer failure addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let _body = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            request_count += 1;
            match request_count {
                1 => {
                    let response_body = concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_outer_send\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"outer send ok\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_outer_send\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write outer send response");
                }
                2 => {
                    drop(socket);
                    return;
                }
                _ => return,
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_outer_send_contract";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Outer send contract".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    let mut events = engine.subscribe();
    let send_result = engine
        .send_message_with_session_and_surface(Some(thread_id), None, "Please respond", None, None)
        .await;
    assert!(
        send_result.is_ok(),
        "observer failure should not fail the outer send"
    );

    let notice = timeout(Duration::from_secs(1), async {
        loop {
            match events.recv().await.expect("event stream should stay open") {
                AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    message,
                    details,
                } if event_thread_id == thread_id && kind == "participant_observer_error" => {
                    return (message, details);
                }
                _ => continue,
            }
        }
    })
    .await
    .expect("expected participant observer workflow notice");

    assert_eq!(notice.0, "participant observers failed");
    assert!(
        notice.1.is_some(),
        "workflow notice should include failure details"
    );

    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "force-send participant failure should not leave queued or failed suggestions behind"
    );
}

#[tokio::test]
async fn resend_existing_user_message_runs_participant_observers_after_stream_completion() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind resend observer server");
    let addr = listener.local_addr().expect("resend observer addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request");
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push_back(body.clone());
                let response_body = if body.contains("Role: participant observer") {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"NO_SUGGESTION\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
                        "data: [DONE]\n\n"
                    )
                } else {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Main reply before observer.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                    response_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_resend_runs_participant_observers";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Resend observer hook".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    recorded_bodies.lock().expect("lock request log").clear();

    engine
        .resend_existing_user_message(thread_id, "hello")
        .await
        .expect("resend should succeed");

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        request_bodies
            .iter()
            .any(|body| body.contains("Role: participant observer")),
        "resend completion should trigger a participant observer prompt"
    );
}

#[tokio::test]
async fn active_participant_responder_completion_does_not_run_self_observer() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_active_participant_responder_no_self_observer";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
            title: "Active participant responder self observer guard".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
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
                        entered_via_handoff_event_id: Some("handoff-1".to_string()),
                        linked_thread_id: Some("dm:svarog:weles".to_string()),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register");

    recorded_bodies.lock().expect("lock request log").clear();

    engine
        .resend_existing_user_message(thread_id, "hello")
        .await
        .expect("active participant resend should succeed");

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        request_bodies.len(),
        1,
        "active participant responder should only issue its visible-thread turn request"
    );
    assert!(
        request_bodies
            .iter()
            .all(|body| !body.contains("Role: participant observer")),
        "active participant responder should not run a self-observer prompt"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "active participant responder should not queue a self-observer suggestion after its own turn"
    );
}

#[tokio::test]
async fn hydrate_runs_participant_observers_for_restored_main_agent_tail() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hydrate observer server");
    let addr = listener.local_addr().expect("hydrate observer addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read hydrate observer request");
                    recorded_bodies
                        .lock()
                        .expect("lock hydrate observer log")
                        .push_back(body.clone());

                    let response_body = if body.contains("Role: participant observer") {
                        concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_hydrate_observer\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Review the restarted main-agent reply.\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_hydrate_observer\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                        )
                    } else {
                        concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_hydrate_default\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Gateway reply ok\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_hydrate_default\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write hydrate observer response");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_hydrate_runs_participant_observers";

    // Use a current timestamp so `should_restore_participant_observers_after_hydrate`'s
    // restore window check (`updated_at + window_hours >= now_millis()`) passes;
    // an epoch-relative `updated_at: 2` would be far outside the default 24h window
    // and the restore would short-circuit before running observers.
    let now = now_millis();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrate participant observer restore".to_string(),
            messages: vec![
                AgentMessage::user("hello", now - 1_000),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "Main agent reply before restart.".to_string(),
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
                    timestamp: now,
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
            created_at: now - 1_000,
            updated_at: now,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register before restart");
    engine.persist_thread_by_id(thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate should succeed");

    timeout(Duration::from_secs(2), async {
        loop {
            let thread_messages = {
                let threads = reloaded.threads.read().await;
                threads
                    .get(thread_id)
                    .expect("thread should still exist after hydrate")
                    .messages
                    .clone()
            };
            let has_participant_note = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() == Some("weles")
                    && message.content == "Review the restarted main-agent reply."
            });
            let has_main_follow_up = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() != Some("weles")
                    && message.content == "Gateway reply ok"
            });
            if has_participant_note && has_main_follow_up {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("hydrate should surface the participant note and main-agent follow-up");

    assert!(
        reloaded
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "idle hydrated participant suggestion should auto-send instead of remaining queued"
    );

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock hydrate observer log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        request_bodies
            .iter()
            .any(|body| body.contains("Role: participant observer")),
        "hydrate should trigger a participant observer request for the restored main-agent tail"
    );
}

#[tokio::test]
async fn hydrate_returns_before_background_participant_observer_restore_finishes() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let observer_requests = Arc::new(AtomicUsize::new(0));
    let request_started = Arc::new(tokio::sync::Notify::new());
    let release_response = Arc::new(tokio::sync::Notify::new());
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hydrate background observer server");
    let addr = listener
        .local_addr()
        .expect("hydrate background observer addr");

    tokio::spawn({
        let observer_requests = observer_requests.clone();
        let request_started = request_started.clone();
        let release_response = release_response.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let observer_requests = observer_requests.clone();
                let request_started = request_started.clone();
                let release_response = release_response.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read hydrate background observer request");
                    if body.contains("Role: participant observer") {
                        observer_requests.fetch_add(1, Ordering::SeqCst);
                        request_started.notify_one();
                        release_response.notified().await;
                    }

                    let response_body = concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_hydrate_background_observer\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_hydrate_background_observer\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":1},\"error\":null}}\n\n"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write hydrate background observer response");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_hydrate_background_participant_observers";

    // Current timestamp so the restore-window gate in
    // `should_restore_participant_observers_after_hydrate` doesn't drop
    // this thread as too old (default window is 24h).
    let now = now_millis();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrate background participant observer restore".to_string(),
            messages: vec![
                AgentMessage::user("hello", now - 1_000),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "Main agent reply before restart.".to_string(),
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
                    timestamp: now,
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
            created_at: now - 1_000,
            updated_at: now,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register before restart");
    engine.persist_thread_by_id(thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;

    timeout(Duration::from_secs(1), reloaded.hydrate())
        .await
        .expect("hydrate should return without waiting for participant observer replay")
        .expect("hydrate should succeed");

    timeout(Duration::from_secs(3), request_started.notified())
        .await
        .expect("background participant observer replay should still start after hydrate");

    assert_eq!(
        observer_requests.load(Ordering::SeqCst),
        1,
        "hydrate should schedule exactly one observer replay request in the background"
    );

    release_response.notify_one();
}

#[tokio::test]
async fn hydrate_does_not_rerun_participant_observers_for_already_reviewed_message() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let observer_requests = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hydrate dedupe server");
    let addr = listener.local_addr().expect("hydrate dedupe addr");

    tokio::spawn({
        let observer_requests = observer_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let observer_requests = observer_requests.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read hydrate dedupe request");
                    if body.contains("Role: participant observer") {
                        observer_requests.fetch_add(1, Ordering::SeqCst);
                    }

                    let response_body = concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_hydrate_dedupe\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_hydrate_dedupe\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":1},\"error\":null}}\n\n"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write hydrate dedupe response");
                });
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_hydrate_participant_observer_dedupe";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrate participant observer dedupe".to_string(),
            messages: vec![
                AgentMessage::user("hello", 1),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "Main agent reply before restart.".to_string(),
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
            updated_at: 2,
        },
    );
    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant should register before initial observer run");
    engine
        .run_participant_observers(thread_id)
        .await
        .expect("initial participant observer run should succeed");
    assert_eq!(observer_requests.load(Ordering::SeqCst), 1);
    engine.persist_thread_by_id(thread_id).await;

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate should succeed");

    assert_eq!(
        observer_requests.load(Ordering::SeqCst),
        1,
        "hydrate should not rerun participant observers for the same already-reviewed visible message"
    );
}

#[tokio::test]
async fn hydrate_keeps_stale_participant_threads_lazy_during_observer_restore() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_hydrate_stale_participant_lazy";
    let participant_id = "weles";
    let old_timestamp = now_millis().saturating_sub(25 * 60 * 60 * 1000);

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Stale participant lazy hydrate".to_string(),
            messages: vec![
                AgentMessage::user("old request", old_timestamp),
                AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Assistant,
                    content: "Old participant reply.".to_string(),
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
                    author_agent_id: Some(participant_id.to_string()),
                    author_agent_name: Some("Weles".to_string()),
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: old_timestamp + 1,
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
            created_at: old_timestamp,
            updated_at: old_timestamp + 1,
        },
    );
    engine
        .upsert_thread_participant(thread_id, participant_id, "watch for issues")
        .await
        .expect("participant should register");
    engine.persist_thread_by_id(thread_id).await;
    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate should succeed");
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        reloaded
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "stale participant observer restore should not hydrate full thread messages"
    );
}
