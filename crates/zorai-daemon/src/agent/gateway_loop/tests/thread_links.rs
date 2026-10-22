use super::*;

async fn linked_engine(name: &str) -> (std::sync::Arc<AgentEngine>, tokio::sync::mpsc::UnboundedReceiver<zorai_protocol::DaemonMessage>, std::path::PathBuf) {
    let root = make_test_root(name);
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;
    (engine, rx, root)
}

fn telegram_message(content: &str) -> super::gateway::IncomingMessage {
    super::gateway::IncomingMessage {
        platform: "Telegram".to_string(),
        sender: "alice".to_string(),
        content: content.to_string(),
        channel: "42".to_string(),
        message_id: None,
        thread_context: None,
    }
}

async fn link_and_ack(
    engine: &std::sync::Arc<AgentEngine>,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<zorai_protocol::DaemonMessage>,
    thread_id: &str,
    channel_key: &str,
) -> String {
    let link_engine = engine.clone();
    let link_thread = thread_id.to_string();
    let link_key = channel_key.to_string();
    let link_task = tokio::spawn(async move {
        link_engine
            .link_thread_to_gateway_chat(&link_thread, Some(&link_key))
            .await
    });
    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("handoff note should be sent")
        .expect("handoff note should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "telegram".to_string(),
            channel_id: request.channel_id.clone(),
            requested_channel_id: Some(request.channel_id.clone()),
            delivery_id: Some("delivery-link".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;
    link_task
        .await
        .expect("link task should join")
        .expect("link should succeed");
    request.content
}

#[tokio::test]
async fn linking_a_chat_that_never_messaged_zorai_is_rejected_so_replies_are_not_lost() {
    let (engine, _rx, root) = linked_engine("gateway-link-unknown-chat").await;
    let (thread_id, _) = engine.get_or_create_thread(None, "hello").await;

    let error = engine
        .link_thread_to_gateway_chat(&thread_id, Some("Telegram:999"))
        .await
        .expect_err("unknown chat must be rejected");
    assert!(error.to_string().contains("has not messaged zorai"), "{error}");
    assert!(engine.gateway_threads.read().await.get("Telegram:999").is_none());
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn linked_gui_thread_continues_on_the_chat_with_its_own_agent_and_title() {
    let (engine, mut rx, root) = linked_engine("gateway-link-gui-thread").await;
    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("hi"), "Ready")
        .await
        .expect("seed chat");
    let gateway_thread = engine.gateway_threads.read().await.get("Telegram:42").cloned();

    let (thread_id, _) = engine.get_or_create_thread(None, "refactor auth").await;
    engine.ensure_thread_identity(&thread_id, "Refactor auth", false).await;

    let note = link_and_ack(&engine, &mut rx, &thread_id, "Telegram:42").await;
    assert!(note.contains("Refactor auth"), "{note}");
    assert!(note.contains("!new"), "{note}");

    assert_eq!(
        engine.gateway_threads.read().await.get("Telegram:42"),
        Some(&thread_id)
    );
    assert_eq!(
        engine.gateway_route_modes.read().await.get("Telegram:42").copied(),
        Some(super::gateway::GatewayRouteMode::Swarog),
        "a linked Svarog thread must not be answered by the gateway default (Rarog)"
    );
    let chats = engine.list_gateway_chats().await.expect("list chats");
    let chat = chats.iter().find(|chat| chat.channel_key == "Telegram:42").expect("chat listed");
    assert_eq!(chat.thread_id.as_deref(), Some(thread_id.as_str()));
    assert_ne!(gateway_thread.as_deref(), Some(thread_id.as_str()));

    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("from phone"), "ok")
        .await
        .expect("continue on phone");
    assert_eq!(
        engine.get_thread(&thread_id).await.expect("thread").title,
        "Refactor auth",
        "messages from the chat must not retitle the linked thread"
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn linking_a_legacy_lowercase_chat_key_moves_the_chat_instead_of_binding_a_second_thread() {
    let (engine, mut rx, root) = linked_engine("gateway-link-legacy-key").await;
    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("hi"), "Ready")
        .await
        .expect("seed chat");
    let old_thread = engine
        .gateway_threads
        .read()
        .await
        .get("Telegram:42")
        .cloned()
        .expect("old thread bound");
    let (legacy_thread, _) = engine.get_or_create_thread(None, "legacy").await;
    engine
        .history
        .upsert_gateway_thread_binding("telegram:42", &legacy_thread, 1)
        .await
        .expect("seed legacy row");

    let chats = engine.list_gateway_chats().await.expect("list chats");
    assert_eq!(
        chats.iter().filter(|chat| chat.channel_id == "42").count(),
        1,
        "one chat must be listed once, whatever casing older rows used"
    );

    let (new_thread, _) = engine.get_or_create_thread(None, "new work").await;
    link_and_ack(&engine, &mut rx, &new_thread, "telegram:42").await;

    let bindings = engine.gateway_threads.read().await.clone();
    assert_eq!(bindings.get("Telegram:42"), Some(&new_thread));
    assert!(bindings.get("telegram:42").is_none());
    assert!(
        !bindings.values().any(|thread| thread == &old_thread),
        "incoming messages must stop landing in the previously linked thread"
    );
    let persisted = engine
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("persisted bindings");
    assert_eq!(persisted, vec![("Telegram:42".to_string(), new_thread.clone())]);
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn unlinking_returns_the_chat_to_gateway_defaults() {
    let (engine, mut rx, root) = linked_engine("gateway-link-unlink").await;
    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("hi"), "Ready")
        .await
        .expect("seed chat");
    let (thread_id, _) = engine.get_or_create_thread(None, "work").await;
    link_and_ack(&engine, &mut rx, &thread_id, "Telegram:42").await;

    engine
        .link_thread_to_gateway_chat(&thread_id, None)
        .await
        .expect("unlink");
    assert!(engine.gateway_threads.read().await.get("Telegram:42").is_none());
    assert!(engine.gateway_route_modes.read().await.get("Telegram:42").is_none());
    let chats = engine.list_gateway_chats().await.expect("list chats");
    assert!(
        chats.iter().any(|chat| chat.channel_key == "Telegram:42" && chat.thread_id.is_none()),
        "an unlinked chat stays available to link again"
    );
    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn gateway_chat_list_reads_titles_of_threads_that_are_not_in_memory() {
    let (engine, _rx, root) = linked_engine("gateway-chat-list-titles").await;
    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("hi"), "Ready")
        .await
        .expect("seed chat");
    let gateway_thread = engine
        .gateway_threads
        .read()
        .await
        .get("Telegram:42")
        .cloned()
        .expect("gateway thread bound");
    let gateway_title = engine.get_thread(&gateway_thread).await.expect("thread").title;
    engine.threads.write().await.remove(&gateway_thread);

    let chats = engine.list_gateway_chats().await.expect("list chats");
    let chat = chats.iter().find(|chat| chat.channel_key == "Telegram:42").expect("chat listed");
    assert_eq!(chat.thread_id.as_deref(), Some(gateway_thread.as_str()));
    assert_eq!(chat.thread_title.as_deref(), Some(gateway_title.as_str()));
    assert_eq!(chat.last_thread_id, gateway_thread);
    assert_eq!(chat.last_thread_title.as_deref(), Some(gateway_title.as_str()));
    fs::remove_dir_all(&root).ok();
}

#[test]
fn per_platform_default_agent_falls_back_to_the_global_default() {
    let mut gateway = GatewayConfig {
        default_agent: "rarog".into(),
        telegram_default_agent: "swarog".into(),
        ..GatewayConfig::default()
    };
    assert_eq!(gateway_default_agent_for_platform(&gateway, "Telegram"), "swarog");
    assert_eq!(gateway_default_agent_for_platform(&gateway, "discord"), "rarog");
    gateway.telegram_default_agent = "  ".into();
    assert_eq!(gateway_default_agent_for_platform(&gateway, "telegram"), "rarog");
}

#[tokio::test]
async fn new_command_resets_the_chat_immediately_even_while_a_reply_is_running() {
    let (engine, mut rx, root) = linked_engine("gateway-reset-during-turn").await;
    engine
        .persist_gateway_fast_path_exchange("Telegram:42", &telegram_message("hi"), "Ready")
        .await
        .expect("seed chat");
    engine
        .gateway_inflight_channels
        .lock()
        .await
        .insert("Telegram:42".to_string());

    let reset_engine = engine.clone();
    let reset_task = tokio::spawn(async move {
        reset_engine
            .enqueue_gateway_message(telegram_message("!new"))
            .await
            .expect("enqueue reset");
        reset_engine.process_gateway_messages().await;
    });
    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("reset confirmation should be sent without waiting for the running turn")
        .expect("reset confirmation should exist")
    {
        zorai_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert!(request.content.contains("Conversation reset"), "{}", request.content);
    engine
        .complete_gateway_send_result(zorai_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "telegram".to_string(),
            channel_id: request.channel_id.clone(),
            requested_channel_id: Some(request.channel_id.clone()),
            delivery_id: Some("delivery-reset".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;
    reset_task.await.expect("reset task should join");

    assert!(engine.gateway_threads.read().await.get("Telegram:42").is_none());
    assert!(
        engine
            .gateway_pending_followups
            .lock()
            .await
            .get("Telegram:42")
            .map_or(true, |queue| queue.is_empty()),
        "!new must not be queued behind the running turn"
    );
    fs::remove_dir_all(&root).ok();
}
