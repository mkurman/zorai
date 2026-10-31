use super::*;
use tempfile::TempDir;
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

async fn ask_restart_survivable_question(
    engine: &Arc<AgentEngine>,
    thread_id: &str,
    tool_call_id: &str,
) -> String {
    let mut assistant = AgentMessage::user("", 1_001);
    assistant.role = MessageRole::Assistant;
    assistant.tool_calls = Some(vec![ToolCall::with_default_weles_review(
        tool_call_id.to_string(),
        ToolFunction {
            name: "ask_questions".to_string(),
            arguments: r#"{"content":"Pick one:\nA. Alpha\nB. Beta","options":["A","B"]}"#
                .to_string(),
        },
    )]);
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Question Thread".to_string(),
            messages: vec![AgentMessage::user("ask me something", 1_000), assistant],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1_000,
            updated_at: 1_001,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    engine.persist_thread_by_id(thread_id).await;

    let mut events = engine.event_tx.subscribe();
    let engine_for_task = engine.clone();
    let thread_for_task = thread_id.to_string();
    let call_for_task = tool_call_id.to_string();
    tokio::spawn(async move {
        let _ = engine_for_task
            .ask_operator_question_for_tool_call(
                "Pick one:\nA. Alpha\nB. Beta",
                vec!["A".to_string(), "B".to_string()],
                None,
                Some(thread_for_task),
                Some(call_for_task),
            )
            .await;
    });
    loop {
        if let AgentEvent::OperatorQuestion { question_id, .. } =
            events.recv().await.expect("question event")
        {
            return question_id;
        }
    }
}

fn mock_openai_config(base_url: String) -> AgentConfig {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config
}

async fn restart_engine(temp_dir: &TempDir, config: AgentConfig) -> Arc<AgentEngine> {
    let restarted = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        config,
        temp_dir.path(),
    )
    .await;
    restarted.hydrate().await.expect("hydrate should succeed");
    restarted
}

#[tokio::test]
async fn operator_question_survives_a_daemon_restart_and_its_answer_resumes_the_waiting_turn() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let config = mock_openai_config(spawn_recording_openai_server(recorded_bodies.clone()).await);
    let (engine, temp_dir) = make_runner_test_engine(config.clone()).await;
    let thread_id = "thread-question-restart";
    let question_id = ask_restart_survivable_question(&engine, thread_id, "call_ask_1").await;

    let restarted = restart_engine(&temp_dir, config).await;

    let replayed = restarted
        .pending_operator_question_events_for_thread(thread_id)
        .await;
    assert!(
        matches!(
            replayed.as_slice(),
            [AgentEvent::OperatorQuestion { question_id: id, options, .. }]
                if id == &question_id && options == &vec!["A".to_string(), "B".to_string()]
        ),
        "a restart must not lose the question the operator has not answered yet: {replayed:?}"
    );

    restarted
        .answer_operator_question(&question_id, "B")
        .await
        .expect("answering a restored question must be accepted");

    let thread = restarted.get_thread(thread_id).await.expect("thread");
    let tool_result = thread
        .messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("call_ask_1"))
        .expect("the answer must become the result of the agent's waiting ask_questions call");
    assert_eq!(tool_result.role, MessageRole::Tool);
    assert_eq!(tool_result.content, "B");
    assert!(
        restarted
            .history
            .list_pending_operator_questions()
            .await
            .expect("list")
            .is_empty(),
        "an answered question must not come back after the next restart"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let thread = restarted.get_thread(thread_id).await.expect("thread");
        if thread.messages.last().is_some_and(|message| {
            message.role == MessageRole::Assistant && message.content == "Gateway reply ok"
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the agent must continue its turn after the answer instead of staying silent"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let request = recorded_bodies
        .lock()
        .expect("lock request log")
        .back()
        .cloned()
        .expect("the resumed turn must call the model");
    let request: serde_json::Value = serde_json::from_str(&request).expect("request json");
    assert!(
        request["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .any(|message| {
                message["role"] == "tool"
                    && message["tool_call_id"] == "call_ask_1"
                    && message["content"] == "B"
            }),
        "the model must receive the operator's answer as the ask_questions result: {request}"
    );
}

#[tokio::test]
async fn restored_question_is_dropped_when_the_thread_moved_on_since_it_was_asked() {
    let (engine, temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread-question-stale";
    let question_id = ask_restart_survivable_question(&engine, thread_id, "call_ask_2").await;

    let restarted = restart_engine(&temp_dir, AgentConfig::default()).await;
    restarted.ensure_thread_messages_loaded(thread_id).await;
    if let Some(thread) = restarted.threads.write().await.get_mut(thread_id) {
        thread
            .messages
            .push(AgentMessage::user("never mind, do something else", 2_000));
    }

    let error = restarted
        .answer_operator_question(&question_id, "A")
        .await
        .expect_err("an answer must not be spliced into a conversation that already moved on");
    assert!(error.to_string().contains("expired"), "{error}");
    assert!(
        restarted
            .pending_operator_question_events_for_thread(thread_id)
            .await
            .is_empty(),
        "a stale question must stop being shown to the operator"
    );
    let thread = restarted.get_thread(thread_id).await.expect("thread");
    assert!(
        thread
            .messages
            .iter()
            .all(|message| message.role != MessageRole::Tool),
        "no tool result may be added after the operator already continued the thread"
    );
}
