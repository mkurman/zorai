use super::*;
use zorai_shared::providers::{PROVIDER_ID_ANTHROPIC, PROVIDER_ID_OPENAI};

#[tokio::test]
async fn send_message_with_ephemeral_user_override_keeps_thread_history_clean() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let server_url = spawn_recording_openai_server(recorded_bodies.clone()).await;
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.max_retries = 0;
    config.auto_retry = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_ephemeral_user_override(
            None,
            "What model are you bro?",
            "[discord message from mariuszkurman]: What model are you bro?\nYour final assistant response will be delivered back to the user automatically.",
            std::time::Duration::from_secs(120),
        )
        .await
        .expect("send message with ephemeral override");

    let messages = engine
        .history
        .list_messages(&thread_id, Some(10))
        .await
        .expect("load persisted messages");
    let stored_user = messages
        .iter()
        .find(|message| message.role == "user")
        .expect("stored user message");
    assert_eq!(stored_user.content, "What model are you bro?");

    let request_body = recorded_bodies
        .lock()
        .expect("lock request log")
        .pop_front()
        .expect("captured llm request");
    assert!(
        request_body.contains(
            "Your final assistant response will be delivered back to the user automatically."
        ),
        "LLM request should include the ephemeral gateway wrapper"
    );
    assert!(
        !request_body.contains("\"content\":\"What model are you bro?\""),
        "LLM request should replace the raw stored user text with the ephemeral override"
    );
}

#[tokio::test]
async fn persisted_assistant_messages_reload_upstream_message_metadata() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_upstream_message_reload";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Upstream message reload".to_string(),
                messages: vec![
                    AgentMessage::user("hello", 1),
                    AgentMessage {
                        reasoning_tokens: None,
                        thinking_ms: None,
                        id: "assistant-upstream-message".to_string(),
                        role: MessageRole::Assistant,
                        content: "Hello from Claude.".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 2,
                        output_tokens: 4,
                        cost: None,
                        provider: Some(PROVIDER_ID_ANTHROPIC.to_string()),
                        model: Some("claude-sonnet-4-20250514".to_string()),
                        api_transport: Some(ApiTransport::ChatCompletions),
                        response_id: Some("msg_upstream_reloaded".to_string()),
                        upstream_message: Some(CompletionUpstreamMessage {
                            id: Some("msg_upstream_reloaded".to_string()),
                            message_type: Some("message".to_string()),
                            role: Some("assistant".to_string()),
                            model: Some("claude-sonnet-4-20250514".to_string()),
                            container: None,
                            stop_reason: Some("end_turn".to_string()),
                            stop_sequence: None,
                            content_blocks: vec![CompletionUpstreamContentBlock {
                                block_type: "text".to_string(),
                                id: None,
                                name: None,
                                text: Some("Hello from Claude.".to_string()),
                                thinking: None,
                                signature: None,
                                input_json: None,
                            }],
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens: None,
                        }),
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
                total_input_tokens: 2,
                total_output_tokens: 4,
                created_at: 1,
                updated_at: 2,
            },
        );
    }

    engine.persist_thread_by_id(thread_id).await;
    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded_engine.hydrate().await.expect("hydrate");
    let thread = reloaded_engine
        .get_thread(thread_id)
        .await
        .expect("thread should reload");
    let assistant = thread
        .messages
        .iter()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("assistant message should reload");
    let upstream = assistant
        .upstream_message
        .as_ref()
        .expect("upstream message should reload");

    assert_eq!(
        assistant.response_id.as_deref(),
        Some("msg_upstream_reloaded")
    );
    assert_eq!(upstream.id.as_deref(), Some("msg_upstream_reloaded"));
    assert_eq!(upstream.message_type.as_deref(), Some("message"));
    assert_eq!(upstream.role.as_deref(), Some("assistant"));
    assert_eq!(upstream.model.as_deref(), Some("claude-sonnet-4-20250514"));
    assert_eq!(upstream.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(upstream.content_blocks.len(), 1);
    assert_eq!(
        upstream.content_blocks[0].text.as_deref(),
        Some("Hello from Claude.")
    );
}
