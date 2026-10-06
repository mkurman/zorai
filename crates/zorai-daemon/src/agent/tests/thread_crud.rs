use super::*;
use crate::session_manager::SessionManager;
use bytes::BytesMut;
use tempfile::tempdir;
use tokio_util::codec::Encoder;

const MAX_FRAME_SIZE_BYTES: usize = 16 * 1024 * 1024;

fn make_thread(
    id: &str,
    agent_name: Option<&str>,
    title: &str,
    pinned: bool,
    created_at: u64,
    updated_at: u64,
    messages: Vec<AgentMessage>,
) -> AgentThread {
    AgentThread {
        id: id.to_string(),
        agent_name: agent_name.map(ToOwned::to_owned),
        title: title.to_string(),
        messages,
        pinned,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at,
        updated_at,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

fn assistant_message(content: impl Into<String>, ts: u64) -> AgentMessage {
    let mut message = AgentMessage::user(content, ts);
    message.role = MessageRole::Assistant;
    message
}

fn weles_internal_message(ts: u64) -> AgentMessage {
    assistant_message(
        crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        ),
        ts,
    )
}

fn list_ids(threads: &[AgentThread]) -> Vec<&str> {
    threads.iter().map(|thread| thread.id.as_str()).collect()
}

#[tokio::test]
async fn first_workspace_context_write_persists_for_memory_only_thread() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "workspace-context-first-write";
    let context = ThreadWorkspaceContext {
        root: "/repo".to_string(),
        isolate_agent_tasks: true,
        updated_at: 42,
        ..ThreadWorkspaceContext::default()
    };

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Workspace context",
            false,
            1,
            2,
            Vec::new(),
        ),
    );
    assert!(!engine
        .history
        .has_thread_id(thread_id)
        .await
        .expect("thread existence query"));

    assert!(
        engine
            .set_thread_workspace_context(thread_id, Some(context.clone()))
            .await
    );
    assert_eq!(
        engine.get_thread_workspace_context(thread_id).await,
        Some(context)
    );
}

#[tokio::test]
async fn message_feedback_resolves_stale_id_by_absolute_index() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "feedback-index-thread";

    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Swarog".to_string()),
            title: "Feedback index".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await
        .expect("create thread");
    for (id, role, content, created_at) in [
        ("user-row", "user", "question", 10),
        ("assistant-row", "assistant", "answer", 20),
    ] {
        engine
            .history
            .add_message(&zorai_protocol::AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: role.to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await
            .expect("add message");
    }

    let resolved = engine
        .apply_message_feedback(
            thread_id,
            "stale-local-id",
            Some(1),
            Some(zorai_protocol::Reaction::Up),
        )
        .await
        .expect("feedback should resolve by absolute index");

    assert_eq!(resolved, Some(zorai_protocol::Reaction::Up));
    let state = engine
        .history
        .message_feedback_state(thread_id, "assistant-row")
        .await
        .expect("feedback state query")
        .expect("assistant row should exist");
    assert_eq!(state.reaction.as_deref(), Some("up"));
}

#[tokio::test]
async fn list_threads_omits_message_history_from_thread_summaries() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_message = "x".repeat(MAX_FRAME_SIZE_BYTES + 1024);

    engine.threads.write().await.insert(
        "thread-big".to_string(),
        AgentThread {
            id: "thread-big".to_string(),
            agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
            title: "Big thread".to_string(),
            messages: vec![AgentMessage::user(huge_message, 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    let listed = engine.list_threads().await;
    assert_eq!(listed.len(), 1);
    assert!(
        listed[0].messages.is_empty(),
        "thread list payload should not include full message history"
    );
    assert_eq!(listed[0].agent_name.as_deref(), Some("Weles"));

    let serialized = serde_json::to_string(&listed).expect("serialize thread summaries");
    assert!(
        serialized.len() < MAX_FRAME_SIZE_BYTES,
        "thread list payload should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn delete_thread_removes_persisted_thread_after_hydrate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-delete-persisted";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Persisted Thread",
            false,
            10,
            20,
            vec![
                AgentMessage::user("hello", 10),
                assistant_message("world", 20),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.clear();
    engine
        .clear_thread_message_hydration_pending(thread_id)
        .await;

    assert!(engine.delete_thread(thread_id).await);
    assert!(
        engine
            .history
            .get_thread(thread_id)
            .await
            .expect("read deleted thread from history")
            .is_none(),
        "deleted thread should be removed from persisted thread rows"
    );
    assert!(
        engine
            .history
            .list_messages(thread_id, None)
            .await
            .expect("read deleted thread messages from history")
            .is_empty(),
        "deleted thread should remove persisted messages via cascade"
    );

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    assert!(
        rehydrated.get_thread(thread_id).await.is_none(),
        "deleted thread should not come back after hydrate"
    );
}

#[tokio::test]
async fn delete_thread_cancels_active_stream() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-delete-active-stream";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Active Stream",
            false,
            10,
            20,
            vec![AgentMessage::user("keep working", 10)],
        ),
    );
    let (_generation, token, _retry_now) = engine.begin_stream_cancellation(thread_id).await;

    assert!(engine.delete_thread(thread_id).await);
    assert!(
        token.is_cancelled(),
        "deleting a thread should stop its active stream"
    );
}

#[tokio::test]
async fn planner_required_for_thread_uses_persisted_latest_user_without_hydrating_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-planner-persisted-latest-user";
    let planning_request =
        "Please review this branch, identify the riskiest daemon startup paths, write the tests, \
         implement the fixes, and then verify the narrow daemon targets before summarizing.";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Planner Required",
            false,
            10,
            40,
            vec![
                AgentMessage::user("quick question", 10),
                assistant_message("short answer", 20),
                AgentMessage::user(planning_request, 30),
                assistant_message("ack", 40),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Planner Required",
            false,
            10,
            40,
            Vec::new(),
        ),
    );
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(thread_id.to_string());

    assert!(engine.planner_required_for_thread(thread_id).await);
    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "planner routing should not force full thread message hydration"
    );
    assert!(
        engine
            .threads
            .read()
            .await
            .get(thread_id)
            .expect("thread shell should remain live")
            .messages
            .is_empty(),
        "planner routing should keep the live thread shell unhydrated"
    );
}

#[tokio::test]
async fn planner_required_for_thread_ignores_stale_live_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-planner-stale-live-user";
    let planning_request =
        "Please audit the daemon startup path, identify the SQL-backed fixes, write regression \
         tests first, implement the smallest safe patches, and verify the focused daemon checks.";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Planner Required",
            false,
            10,
            40,
            vec![
                AgentMessage::user("quick question", 10),
                assistant_message("short answer", 20),
                AgentMessage::user(planning_request, 30),
                assistant_message("ack", 40),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Planner Required",
            false,
            10,
            40,
            vec![AgentMessage::user("quick question", 10)],
        ),
    );
    engine
        .clear_thread_message_hydration_pending(thread_id)
        .await;

    assert!(
        engine.planner_required_for_thread(thread_id).await,
        "planner routing should use the latest persisted user message, not stale live messages"
    );
}

#[tokio::test]
async fn delete_thread_skips_session_abandon_when_existing_signal_is_older_than_recent_window() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-delete-existing-abandon";
    let now = now_millis();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Existing abandon",
            false,
            now.saturating_sub(20_000),
            now.saturating_sub(1_000),
            vec![assistant_message(
                "Waiting for operator response",
                now.saturating_sub(1_000),
            )],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    engine
        .history
        .insert_implicit_signal(&crate::history::ImplicitSignalRow {
            id: "old-session-abandon".to_string(),
            session_id: thread_id.to_string(),
            signal_type: "session_abandon".to_string(),
            weight: -0.2,
            timestamp_ms: now.saturating_sub(20_000),
            context_snapshot_json: None,
        })
        .await
        .expect("insert existing abandon signal");
    for index in 0..10u64 {
        engine
            .history
            .insert_implicit_signal(&crate::history::ImplicitSignalRow {
                id: format!("newer-non-abandon-{index}"),
                session_id: thread_id.to_string(),
                signal_type: "tool_fallback".to_string(),
                weight: -0.01,
                timestamp_ms: now.saturating_sub(10_000).saturating_add(index),
                context_snapshot_json: None,
            })
            .await
            .expect("insert newer non-abandon signal");
    }

    assert!(engine.delete_thread(thread_id).await);

    let signals = engine
        .history
        .list_implicit_signals(thread_id, 20)
        .await
        .expect("list implicit signals");
    let abandon_count = signals
        .iter()
        .filter(|signal| signal.signal_type == "session_abandon")
        .count();
    assert_eq!(
        abandon_count, 1,
        "delete should use an exact SQL existence check instead of scanning only recent rows"
    );
}

#[tokio::test]
async fn delete_thread_records_session_abandon_from_persisted_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-delete-persisted-session-abandon";
    let now = now_millis();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Persisted abandon",
            false,
            now.saturating_sub(1_000),
            now,
            vec![
                AgentMessage::user("Help me decide.", now.saturating_sub(1_000)),
                assistant_message("Here is the answer.", now),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.clear();
    engine
        .clear_thread_message_hydration_pending(thread_id)
        .await;

    assert!(engine.delete_thread(thread_id).await);

    let signals = engine
        .history
        .list_implicit_signals(thread_id, 10)
        .await
        .expect("list persisted session abandon signals");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal_type, "session_abandon");
    assert!(signals[0]
        .context_snapshot_json
        .as_deref()
        .is_some_and(|json| json.contains(thread_id) && json.contains("Here is the answer.")));
}

#[tokio::test]
async fn delete_thread_does_not_hydrate_unrelated_lazy_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let deleted_thread_id = "thread-delete-target";
    let unrelated_thread_id = "thread-delete-unrelated-lazy";

    for thread_id in [deleted_thread_id, unrelated_thread_id] {
        engine.threads.write().await.insert(
            thread_id.to_string(),
            make_thread(
                thread_id,
                Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
                "Delete lazy",
                false,
                1,
                3,
                vec![
                    AgentMessage::user(format!("{thread_id} first"), 1),
                    assistant_message(format!("{thread_id} second"), 3),
                ],
            ),
        );
        engine.persist_thread_by_id(thread_id).await;
    }

    engine.threads.write().await.insert(
        unrelated_thread_id.to_string(),
        make_thread(
            unrelated_thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Delete lazy",
            false,
            1,
            3,
            Vec::new(),
        ),
    );
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(unrelated_thread_id.to_string());

    assert!(engine.delete_thread(deleted_thread_id).await);

    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(unrelated_thread_id),
        "deleting one thread should not hydrate unrelated lazy thread messages"
    );
    assert!(
        engine
            .threads
            .read()
            .await
            .get(unrelated_thread_id)
            .expect("unrelated live shell should remain present")
            .messages
            .is_empty(),
        "unrelated lazy thread shell should stay unloaded after delete"
    );
}

#[tokio::test]
async fn list_threads_filters_visible_threads_by_default() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "visible-main".to_string(),
        make_thread(
            "visible-main",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Visible",
            false,
            10,
            30,
            vec![AgentMessage::user("operator message", 10)],
        ),
    );
    threads.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            11,
            29,
            vec![weles_internal_message(11)],
        ),
    );
    threads.insert(
        "handoff:hidden".to_string(),
        make_thread(
            "handoff:hidden",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Hidden handoff",
            false,
            12,
            28,
            vec![AgentMessage::user("handoff", 12)],
        ),
    );
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;

    assert_eq!(list_ids(&listed), vec!["visible-main"]);
    assert!(listed[0].messages.is_empty());
}

#[tokio::test]
async fn list_threads_include_internal_reveals_hidden_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "visible-main".to_string(),
        make_thread(
            "visible-main",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Visible",
            false,
            10,
            30,
            vec![AgentMessage::user("operator message", 10)],
        ),
    );
    threads.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            11,
            29,
            vec![weles_internal_message(11)],
        ),
    );
    threads.insert(
        "handoff:hidden".to_string(),
        make_thread(
            "handoff:hidden",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Hidden handoff",
            false,
            12,
            28,
            vec![AgentMessage::user("handoff", 12)],
        ),
    );
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter {
            include_internal: true,
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(
        list_ids(&listed),
        vec!["visible-main", "weles-hidden", "handoff:hidden"]
    );
}

#[tokio::test]
async fn list_threads_filtered_matches_user_defined_subagent_agent_name() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "dola".to_string(),
        name: "Dola".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("review specialist".to_string()),
        system_prompt: Some("Review code carefully.".to_string()),
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

    engine.threads.write().await.insert(
        "thread-dola".to_string(),
        make_thread(
            "thread-dola",
            Some("Dola"),
            "Dola thread",
            false,
            10,
            20,
            vec![AgentMessage::user("hello", 10)],
        ),
    );
    engine.threads.write().await.insert(
        "thread-svarog".to_string(),
        make_thread(
            "thread-svarog",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Svarog thread",
            false,
            11,
            21,
            vec![AgentMessage::user("main", 11)],
        ),
    );

    let listed = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("Dola".to_string()),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&listed), vec!["thread-dola"]);

    let listed_by_id = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("dola".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    assert_eq!(list_ids(&listed_by_id), vec!["thread-dola"]);

    let listed_main = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("svarog".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    assert_eq!(list_ids(&listed_main), vec!["thread-svarog"]);
}

#[tokio::test]
async fn list_threads_filtered_matches_registry_id_without_collapsing_to_main() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "subagent-1777065727944".to_string(),
        name: "DeepSeekorrr".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("specialist".to_string()),
        system_prompt: Some("Handle delegated work.".to_string()),
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
        reasoning_effort: None,
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;

    engine.threads.write().await.insert(
        "thread-deepseek".to_string(),
        make_thread(
            "thread-deepseek",
            Some("DeepSeekorrr"),
            "DeepSeek thread",
            false,
            10,
            40,
            vec![AgentMessage::user("hello", 10)],
        ),
    );
    engine.threads.write().await.insert(
        "thread-svarog".to_string(),
        make_thread(
            "thread-svarog",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Svarog thread",
            false,
            11,
            30,
            vec![AgentMessage::user("main", 11)],
        ),
    );
    engine.persist_thread_by_id("thread-deepseek").await;
    engine.persist_thread_by_id("thread-svarog").await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        config,
        root.path(),
    )
    .await;

    let listed = rehydrated
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("subagent-1777065727944".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    assert_eq!(
        list_ids(&listed),
        vec!["thread-deepseek"],
        "GUI keys subagent tabs by registry id while threads persist the display name"
    );

    let listed_by_name = rehydrated
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("DeepSeekorrr".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    assert_eq!(list_ids(&listed_by_name), vec!["thread-deepseek"]);

    let listed_main = rehydrated
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("svarog".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    assert_eq!(list_ids(&listed_main), vec!["thread-svarog"]);
}

#[tokio::test]
async fn pin_rejected_when_budget_would_be_exceeded() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.context_window_tokens = 100;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-pin-budget";

    let mut already_pinned = AgentMessage::user("a".repeat(80), 1);
    already_pinned.pinned_for_compaction = true;
    let candidate = AgentMessage::user("b".repeat(30), 2);
    let candidate_id = candidate.id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Pin budget",
            false,
            1,
            2,
            vec![already_pinned, candidate],
        ),
    );

    let result = engine
        .pin_thread_message_for_compaction(thread_id, &candidate_id)
        .await;

    assert!(!result.ok, "pin should be rejected when it exceeds budget");
    assert_eq!(result.thread_id, thread_id);
    assert_eq!(result.message_id, candidate_id);
    assert_eq!(result.current_pinned_chars, 80);
    assert_eq!(result.candidate_pinned_chars, Some(110));
    assert_eq!(result.pinned_budget_chars, 100);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should still exist");
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.pinned_for_compaction)
            .count(),
        1
    );
}

#[tokio::test]
async fn pin_thread_message_for_compaction_updates_persisted_message_without_hydrating_thread() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-pin-persisted-lazy";
    let candidate = AgentMessage::user("pin this persisted message", 2);
    let candidate_id = candidate.id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Pin persisted lazy",
            false,
            1,
            2,
            vec![AgentMessage::user("older context", 1), candidate],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Pin persisted lazy",
            false,
            1,
            2,
            Vec::new(),
        ),
    );
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(thread_id.to_string());

    let result = engine
        .pin_thread_message_for_compaction(thread_id, &candidate_id)
        .await;

    assert!(result.ok, "pin should update persisted message metadata");
    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "pinning a persisted message should not hydrate the full thread"
    );
    assert!(
        engine
            .threads
            .read()
            .await
            .get(thread_id)
            .expect("thread shell should remain live")
            .messages
            .is_empty(),
        "lazy thread shell should stay unhydrated after pin"
    );

    let persisted_messages = engine
        .history
        .list_messages(thread_id, None)
        .await
        .expect("list persisted messages");
    let pinned_metadata = persisted_messages
        .iter()
        .find(|message| message.id == candidate_id)
        .and_then(|message| message.metadata_json.as_deref())
        .expect("pinned message should have metadata");
    assert!(
        pinned_metadata.contains("\"pinned_for_compaction\":true"),
        "persisted message metadata should store the pin"
    );
}

#[tokio::test]
async fn unpin_thread_message_for_compaction_updates_persisted_message_without_hydrating_thread() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-unpin-persisted-lazy";
    let mut pinned = AgentMessage::user("unpin this persisted message", 2);
    pinned.pinned_for_compaction = true;
    let pinned_id = pinned.id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Unpin persisted lazy",
            false,
            1,
            2,
            vec![AgentMessage::user("older context", 1), pinned],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Unpin persisted lazy",
            false,
            1,
            2,
            Vec::new(),
        ),
    );
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(thread_id.to_string());

    let result = engine
        .unpin_thread_message_for_compaction(thread_id, &pinned_id)
        .await;

    assert!(result.ok, "unpin should update persisted message metadata");
    assert_eq!(result.current_pinned_chars, 0);
    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "unpinning a persisted message should not hydrate the full thread"
    );
    assert!(
        engine
            .threads
            .read()
            .await
            .get(thread_id)
            .expect("thread shell should remain live")
            .messages
            .is_empty(),
        "lazy thread shell should stay unhydrated after unpin"
    );

    let persisted_messages = engine
        .history
        .list_messages(thread_id, None)
        .await
        .expect("list persisted messages");
    let pinned_metadata = persisted_messages
        .iter()
        .find(|message| message.id == pinned_id)
        .and_then(|message| message.metadata_json.as_deref())
        .expect("unpinned message should have metadata");
    assert!(
        pinned_metadata.contains("\"pinned_for_compaction\":false"),
        "persisted message metadata should clear the pin"
    );
}

#[tokio::test]
async fn deleting_pinned_message_removes_pin_state() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-delete-pinned";

    let message = AgentMessage::user("keep this pinned", 1);
    let message_id = message.id.clone();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Delete pin",
            false,
            1,
            1,
            vec![message],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let result = engine
        .pin_thread_message_for_compaction(thread_id, &message_id)
        .await;
    assert!(result.ok, "pin should succeed before deletion");

    let deleted = engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&message_id))
        .await
        .expect("delete should succeed");
    assert_eq!(deleted, 1);

    let thread = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should still exist");
    assert!(
        thread
            .messages
            .iter()
            .all(|message| !message.pinned_for_compaction),
        "deleting the source message should remove the pin with it"
    );
}

#[tokio::test]
async fn list_threads_dm_visibility_is_unchanged() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::CONCIERGE_AGENT_ID,
    );

    engine.threads.write().await.insert(
        dm_thread_id.clone(),
        make_thread(
            &dm_thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Internal DM",
            false,
            10,
            30,
            vec![AgentMessage::user("dm content", 10)],
        ),
    );

    let default_list = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;
    let internal_list = engine
        .list_threads_filtered(&ThreadListFilter {
            include_internal: true,
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&default_list), vec![dm_thread_id.as_str()]);
    assert_eq!(list_ids(&internal_list), vec![dm_thread_id.as_str()]);
}

#[tokio::test]
async fn list_threads_filters_agent_aliases_and_absent_agent_name_as_main() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "main-none".to_string(),
        make_thread(
            "main-none",
            None,
            "Main none",
            false,
            1,
            30,
            vec![AgentMessage::user("main none", 1)],
        ),
    );
    threads.insert(
        "main-empty".to_string(),
        make_thread(
            "main-empty",
            Some(""),
            "Main empty",
            false,
            2,
            20,
            vec![AgentMessage::user("main empty", 2)],
        ),
    );
    threads.insert(
        "weles".to_string(),
        make_thread(
            "weles",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Weles visible",
            false,
            3,
            10,
            vec![AgentMessage::user("not hidden", 3)],
        ),
    );
    drop(threads);

    let main_list = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("main-agent".to_string()),
            ..ThreadListFilter::default()
        })
        .await;
    let weles_list = engine
        .list_threads_filtered(&ThreadListFilter {
            agent_name: Some("weles".to_string()),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&main_list), vec!["main-none", "main-empty"]);
    assert_eq!(list_ids(&weles_list), vec!["weles"]);
}

#[tokio::test]
async fn list_threads_empty_title_query_is_a_no_op() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "alpha".to_string(),
        make_thread(
            "alpha",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Alpha",
            false,
            1,
            20,
            vec![AgentMessage::user("a", 1)],
        ),
    );
    threads.insert(
        "beta".to_string(),
        make_thread(
            "beta",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Beta",
            false,
            2,
            10,
            vec![AgentMessage::user("b", 2)],
        ),
    );
    drop(threads);

    let baseline = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;
    let with_empty_title = engine
        .list_threads_filtered(&ThreadListFilter {
            title_query: Some("   ".to_string()),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&baseline), list_ids(&with_empty_title));
}

#[tokio::test]
async fn list_threads_limit_and_offset_obey_boundaries() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    for (id, updated_at) in [("one", 30), ("two", 20), ("three", 10)] {
        threads.insert(
            id.to_string(),
            make_thread(
                id,
                Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
                id,
                false,
                updated_at - 1,
                updated_at,
                vec![AgentMessage::user(id, updated_at - 1)],
            ),
        );
    }
    drop(threads);

    let zero_limit = engine
        .list_threads_filtered(&ThreadListFilter {
            limit: Some(0),
            ..ThreadListFilter::default()
        })
        .await;
    let zero_offset = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 0,
            limit: Some(2),
            ..ThreadListFilter::default()
        })
        .await;
    let oversized_limit = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 1,
            limit: Some(99),
            ..ThreadListFilter::default()
        })
        .await;
    let offset_past_end = engine
        .list_threads_filtered(&ThreadListFilter {
            offset: 3,
            ..ThreadListFilter::default()
        })
        .await;

    assert!(zero_limit.is_empty());
    assert_eq!(list_ids(&zero_offset), vec!["one", "two"]);
    assert_eq!(list_ids(&oversized_limit), vec!["two", "three"]);
    assert!(offset_past_end.is_empty());
}

#[tokio::test]
async fn list_threads_orders_same_updated_at_deterministically() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    for id in ["thread-c", "thread-a", "thread-b"] {
        threads.insert(
            id.to_string(),
            make_thread(
                id,
                Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
                id,
                false,
                1,
                10,
                vec![AgentMessage::user(id, 1)],
            ),
        );
    }
    drop(threads);

    let listed = engine
        .list_threads_filtered(&ThreadListFilter::default())
        .await;

    assert_eq!(list_ids(&listed), vec!["thread-a", "thread-b", "thread-c"]);
}

#[tokio::test]
async fn list_threads_filtered_reads_persisted_summaries_without_hydration() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut threads = engine.threads.write().await;
    threads.insert(
        "old-main".to_string(),
        make_thread(
            "old-main",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Alpha archive",
            false,
            1,
            10,
            vec![AgentMessage::user("old", 1)],
        ),
    );
    threads.insert(
        "new-main-pinned".to_string(),
        make_thread(
            "new-main-pinned",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Alpha current",
            true,
            2,
            30,
            vec![AgentMessage::user("new", 2)],
        ),
    );
    threads.insert(
        "other-agent".to_string(),
        make_thread(
            "other-agent",
            Some("Dola"),
            "Alpha delegated",
            true,
            3,
            20,
            vec![AgentMessage::user("delegated", 3)],
        ),
    );
    drop(threads);

    engine.persist_thread_by_id("old-main").await;
    engine.persist_thread_by_id("new-main-pinned").await;
    engine.persist_thread_by_id("other-agent").await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;

    let listed = rehydrated
        .list_threads_filtered(&ThreadListFilter {
            created_after: Some(2),
            updated_before: Some(30),
            agent_name: Some("main-agent".to_string()),
            title_query: Some("current".to_string()),
            pinned: Some(true),
            limit: Some(1),
            ..ThreadListFilter::default()
        })
        .await;

    assert_eq!(list_ids(&listed), vec!["new-main-pinned"]);
    assert!(
        listed[0].messages.is_empty(),
        "persisted SQL list should return summaries without message bodies"
    );
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "listing persisted summaries should not hydrate the engine thread map"
    );
}

#[tokio::test]
async fn thread_metadata_getters_read_persisted_metadata_without_thread_hydration() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-metadata-getters-cold-db";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Metadata getters",
            false,
            1,
            2,
            vec![AgentMessage::user("hello", 1)],
        ),
    );
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;
    engine
        .set_thread_skill_discovery_state(
            thread_id,
            crate::agent::types::LatestSkillDiscoveryState {
                query: "debug restart".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "read_skill systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: false,
                mesh_approval_id: None,
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: false,
                compliant: false,
                updated_at: 3,
            },
        )
        .await;
    engine
        .set_thread_memory_injection_state(
            thread_id,
            crate::agent::memory_context::PromptMemoryInjectionState {
                base_markdown_hash: Some("base-hash".to_string()),
                base_markdown_updated_at_ms: Some(4),
                soul_markdown_hash: Some("soul-hash".to_string()),
                soul_markdown_updated_at_ms: Some(5),
                memory_markdown_hash: Some("memory-hash".to_string()),
                memory_markdown_updated_at_ms: Some(6),
                user_markdown_hash: Some("user-hash".to_string()),
                user_markdown_updated_at_ms: Some(7),
                structured_summary_hash: Some("summary-hash".to_string()),
                base_markdown_injected_at_ms: Some(8),
                injected_after_compaction: true,
            },
        )
        .await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "test setup should keep the live thread map cold"
    );

    assert_eq!(
        rehydrated.get_thread_client_surface(thread_id).await,
        Some(zorai_protocol::ClientSurface::Tui)
    );
    let skill_state = rehydrated
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("skill discovery state should be loaded from persisted metadata");
    assert_eq!(
        skill_state.recommended_skill.as_deref(),
        Some("systematic-debugging")
    );
    let memory_state = rehydrated
        .get_thread_memory_injection_state(thread_id)
        .await
        .expect("memory injection state should be loaded from persisted metadata");
    assert_eq!(
        memory_state.base_markdown_hash.as_deref(),
        Some("base-hash")
    );
    assert!(memory_state.injected_after_compaction);
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "metadata getters should not hydrate full thread messages"
    );
}

#[tokio::test]
async fn thread_metadata_setter_updates_persisted_metadata_without_thread_hydration() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-metadata-setter-lazy-db";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Metadata setter",
            false,
            1,
            3,
            vec![
                AgentMessage::user("persisted first", 1),
                assistant_message("persisted second", 3),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Metadata setter",
            false,
            1,
            3,
            Vec::new(),
        ),
    );
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(thread_id.to_string());

    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Electron)
        .await;

    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "metadata-only writes should not hydrate the full thread"
    );
    assert!(
        engine
            .threads
            .read()
            .await
            .get(thread_id)
            .expect("live shell should remain present")
            .messages
            .is_empty(),
        "metadata-only writes should leave lazy thread messages unloaded"
    );

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    assert_eq!(
        rehydrated.get_thread_client_surface(thread_id).await,
        Some(zorai_protocol::ClientSurface::Electron)
    );
    let detail = rehydrated
        .get_thread_filtered(thread_id, false, None, 0)
        .await
        .expect("thread should still exist");
    assert_eq!(detail.thread.messages.len(), 2);
}

#[tokio::test]
async fn append_system_thread_message_restores_persisted_thread_before_mutating() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-append-system-cold-db";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Append system",
            false,
            1,
            2,
            vec![AgentMessage::user("persisted request", 1)],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "test setup should keep the live thread map cold"
    );

    assert!(
        rehydrated
            .append_system_thread_message(thread_id, "persisted system note")
            .await,
        "append should restore the persisted thread before mutating"
    );

    let rehydrated_again = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    let detail = rehydrated_again
        .get_thread_filtered(thread_id, false, None, 0)
        .await
        .expect("thread should still exist");
    assert_eq!(detail.thread.messages.len(), 2);
    let appended = detail.thread.messages.last().expect("system message");
    assert_eq!(appended.role, MessageRole::System);
    assert_eq!(appended.content, "persisted system note");
}

#[tokio::test]
async fn get_thread_filtered_truncates_to_last_n_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "thread-a".to_string(),
        make_thread(
            "thread-a",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread A",
            false,
            1,
            4,
            vec![
                AgentMessage::user("one", 1),
                assistant_message("two", 2),
                AgentMessage::user("three", 3),
                assistant_message("four", 4),
            ],
        ),
    );

    let detail = engine
        .get_thread_filtered("thread-a", false, Some(2), 0)
        .await
        .expect("visible thread should load");

    let contents = detail
        .thread
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["three", "four"]);
    assert!(detail.messages_truncated);

    let untruncated = engine
        .get_thread_filtered("thread-a", false, Some(99), 0)
        .await
        .expect("oversized limit should still return thread");
    assert_eq!(untruncated.thread.messages.len(), 4);
    assert!(!untruncated.messages_truncated);
}

#[tokio::test]
async fn agent_thread_detail_json_includes_offscreen_pinned_message_summaries() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut pinned = AgentMessage::user("Pinned offscreen", 1);
    pinned.pinned_for_compaction = true;
    let pinned_id = pinned.id.clone();

    engine.threads.write().await.insert(
        "thread-pinned".to_string(),
        make_thread(
            "thread-pinned",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Pinned thread",
            false,
            1,
            3,
            vec![
                AgentMessage::user("oldest", 0),
                pinned,
                assistant_message("latest visible", 2),
            ],
        ),
    );

    let json = engine
        .agent_thread_detail_json("thread-pinned", Some(1), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");
    let pinned_messages = value["pinned_messages"]
        .as_array()
        .expect("pinned_messages should be an array");

    assert_eq!(value["loaded_message_start"].as_u64(), Some(2));
    assert_eq!(value["loaded_message_end"].as_u64(), Some(3));
    assert_eq!(value["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(pinned_messages.len(), 1);
    assert_eq!(
        pinned_messages[0]["message_id"].as_str(),
        Some(pinned_id.as_str())
    );
    assert_eq!(pinned_messages[0]["absolute_index"].as_u64(), Some(1));
    assert_eq!(
        pinned_messages[0]["content"].as_str(),
        Some("Pinned offscreen")
    );
}

#[tokio::test]
async fn agent_thread_detail_json_reports_real_active_context_window_for_paged_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut compaction = assistant_message("visible compaction summary", 3);
    compaction.message_kind = AgentMessageKind::CompactionArtifact;
    compaction.compaction_payload = Some("P".repeat(40));

    engine.threads.write().await.insert(
        "thread-context-window".to_string(),
        make_thread(
            "thread-context-window",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Context window",
            false,
            1,
            4,
            vec![
                AgentMessage::user("A".repeat(400), 1),
                assistant_message("B".repeat(400), 2),
                compaction,
                AgentMessage::user("C".repeat(80), 4),
            ],
        ),
    );

    let json = engine
        .agent_thread_detail_json("thread-context-window", Some(1), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");

    assert_eq!(value["loaded_message_start"].as_u64(), Some(3));
    assert_eq!(value["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["active_context_window_start"].as_u64(), Some(2));
    assert_eq!(value["active_context_window_end"].as_u64(), Some(4));
    assert_eq!(value["active_context_window_tokens"].as_u64(), Some(54));
}

#[tokio::test]
async fn agent_thread_detail_json_treats_legacy_visible_compaction_as_active_boundary() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let legacy_compaction = assistant_message(
        "Pre-compaction context: ~842,460 / 400,000 tokens (threshold 320,000)\n\
         Trigger: token-threshold\n\
         Strategy: custom model generated summary.",
        3,
    );

    engine.threads.write().await.insert(
        "thread-legacy-context-window".to_string(),
        make_thread(
            "thread-legacy-context-window",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Legacy context window",
            false,
            1,
            4,
            vec![
                AgentMessage::user("A".repeat(400), 1),
                assistant_message("B".repeat(400), 2),
                legacy_compaction,
                AgentMessage::user("C".repeat(80), 4),
            ],
        ),
    );

    let json = engine
        .agent_thread_detail_json("thread-legacy-context-window", Some(1), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");

    assert_eq!(value["loaded_message_start"].as_u64(), Some(3));
    assert_eq!(value["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["active_context_window_start"].as_u64(), Some(2));
    assert_eq!(value["active_context_window_end"].as_u64(), Some(4));
    assert_eq!(value["active_context_window_tokens"].as_u64(), Some(78));
}

#[tokio::test]
async fn paged_persisted_thread_detail_keeps_full_history_lazy() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-paged-persisted";
    let mut messages = (0..12)
        .map(|index| AgentMessage::user(format!("message-{index}"), index))
        .collect::<Vec<_>>();
    messages[1].pinned_for_compaction = true;
    let pinned_id = messages[1].id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Paged persisted thread",
            false,
            1,
            12,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let json = rehydrated
        .agent_thread_detail_json(thread_id, Some(3), Some(2), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("thread detail json");

    assert_eq!(value["total_message_count"].as_u64(), Some(12));
    assert_eq!(value["loaded_message_start"].as_u64(), Some(7));
    assert_eq!(value["loaded_message_end"].as_u64(), Some(10));
    let messages = value["messages"]
        .as_array()
        .expect("messages should be serialized");
    let contents = messages
        .iter()
        .map(|message| message["content"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["message-7", "message-8", "message-9"]);
    let pinned_messages = value["pinned_messages"]
        .as_array()
        .expect("pinned_messages should be serialized");
    assert_eq!(pinned_messages.len(), 1);
    assert_eq!(
        pinned_messages[0]["message_id"].as_str(),
        Some(pinned_id.as_str())
    );
    assert_eq!(pinned_messages[0]["absolute_index"].as_u64(), Some(1));

    let in_memory = rehydrated.threads.read().await;
    let thread = in_memory
        .get(thread_id)
        .expect("thread summary should remain in memory");
    assert!(
        thread.messages.is_empty(),
        "paged detail should not hydrate every persisted message into memory"
    );
    drop(in_memory);

    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "full message hydration should remain pending after a paged detail request"
    );
}

#[tokio::test]
async fn continuing_paged_persisted_thread_keeps_hydration_pending_until_loaded() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-paged-continue-safe";
    let messages = (0..12)
        .map(|index| AgentMessage::user(format!("message-{index}"), index))
        .collect::<Vec<_>>();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Paged persisted thread",
            false,
            1,
            12,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let detail = rehydrated
        .get_thread_filtered(thread_id, false, Some(3), 0)
        .await
        .expect("paged detail should load");
    assert_eq!(detail.thread.messages.len(), 3);
    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "paged detail should leave full message hydration pending"
    );

    let (continued_thread_id, created) = rehydrated
        .get_or_create_thread(Some(thread_id), "follow up")
        .await;
    assert_eq!(continued_thread_id, thread_id);
    assert!(!created);
    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "continuing an existing lazy thread must not clear pending hydration"
    );

    assert!(rehydrated.ensure_thread_messages_loaded(thread_id).await);
    let in_memory = rehydrated.threads.read().await;
    let thread = in_memory
        .get(thread_id)
        .expect("continued thread should remain in memory");
    assert_eq!(thread.messages.len(), 12);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("message-11")
    );
}

#[tokio::test]
async fn ensure_thread_messages_loaded_restores_missing_live_thread_from_sqlite() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-ensure-loads-from-sqlite";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Load from sqlite",
            false,
            1,
            3,
            vec![
                AgentMessage::user("persisted user", 1),
                assistant_message("persisted assistant", 3),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;

    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .is_empty(),
        "test setup should not rely on pending hydration markers"
    );
    assert!(
        rehydrated.ensure_thread_messages_loaded(thread_id).await,
        "ensure should use persisted thread/message rows when the live map is cold"
    );

    let threads = rehydrated.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("persisted thread should be restored into memory");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("persisted assistant")
    );
}

#[tokio::test]
async fn get_or_create_persisted_thread_restores_shell_without_loading_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-restore-shell-only";
    let messages = (0..24)
        .map(|index| AgentMessage::user(format!("message-{index}"), index))
        .collect::<Vec<_>>();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Restore shell only",
            false,
            1,
            24,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;

    let (restored_thread_id, created) = rehydrated
        .get_or_create_thread(Some(thread_id), "follow up")
        .await;
    assert_eq!(restored_thread_id, thread_id);
    assert!(!created);

    let in_memory = rehydrated.threads.read().await;
    let thread = in_memory
        .get(thread_id)
        .expect("thread shell should be restored");
    assert!(
        thread.messages.is_empty(),
        "restoring a persisted thread shell must not load every persisted message"
    );
    drop(in_memory);
    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "restored thread shell should keep full message hydration pending"
    );
}

#[tokio::test]
async fn get_thread_filtered_reloads_soft_deleted_persisted_messages_after_hydration() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-authoritative-refresh";
    let messages = (0..6)
        .map(|index| {
            if index % 2 == 0 {
                AgentMessage::user(format!("message-{index}"), index)
            } else {
                assistant_message(format!("message-{index}"), index)
            }
        })
        .collect::<Vec<_>>();
    let deleted_message_id = messages[2].id.clone();

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Authoritative refresh",
            false,
            1,
            6,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let initial_detail = rehydrated
        .get_thread_filtered(thread_id, false, None, 0)
        .await
        .expect("initial detail should load");
    assert_eq!(initial_detail.thread.messages.len(), 6);
    assert!(
        initial_detail
            .thread
            .messages
            .iter()
            .any(|message| message.id == deleted_message_id),
        "sanity check: persisted message should exist before soft delete"
    );

    let deleted = rehydrated
        .history
        .delete_messages(thread_id, &[deleted_message_id.as_str()])
        .await
        .expect("soft delete persisted message");
    assert_eq!(deleted, 1);

    let refreshed_detail = rehydrated
        .get_thread_filtered(thread_id, false, None, 0)
        .await
        .expect("refreshed detail should load");
    assert_eq!(refreshed_detail.thread.messages.len(), 5);
    assert!(
        refreshed_detail
            .thread
            .messages
            .iter()
            .all(|message| message.id != deleted_message_id),
        "authoritative refresh should exclude soft-deleted persisted rows"
    );

    let threads = rehydrated.threads.read().await;
    let cached = threads
        .get(thread_id)
        .expect("thread should remain cached after refresh");
    assert_eq!(cached.messages.len(), 5);
    assert!(
        cached
            .messages
            .iter()
            .all(|message| message.id != deleted_message_id),
        "authoritative refresh should also repair in-memory thread messages"
    );
}

#[tokio::test]
async fn get_thread_capped_for_ipc_truncates_oversized_thread_detail_payload() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let huge_message = "x".repeat(MAX_FRAME_SIZE_BYTES + 1024);

    engine.threads.write().await.insert(
        "thread-huge".to_string(),
        make_thread(
            "thread-huge",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Huge thread",
            false,
            1,
            3,
            vec![
                AgentMessage::user(huge_message, 1),
                assistant_message("recent tail", 2),
            ],
        ),
    );

    let detail = engine
        .get_thread_capped_for_ipc("thread-huge", false)
        .await
        .expect("visible thread should load");

    let contents = detail
        .thread
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["recent tail"]);
    assert!(detail.messages_truncated);

    let thread_json =
        serde_json::to_string(&Some(detail.thread)).expect("serialize capped thread detail json");
    let mut frame = BytesMut::new();
    zorai_protocol::DaemonCodec::default()
        .encode(
            zorai_protocol::DaemonMessage::AgentThreadDetail { thread_json },
            &mut frame,
        )
        .expect("serialize capped daemon message");
    assert!(
        frame.len().saturating_sub(4) <= MAX_FRAME_SIZE_BYTES,
        "capped thread detail should stay below the IPC frame cap"
    );
}

#[tokio::test]
async fn get_thread_capped_for_ipc_uses_persisted_window_without_full_restore() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-persisted-capped-ipc";
    let mut messages = (0..199)
        .map(|index| AgentMessage::user(format!("message-{index}"), index))
        .collect::<Vec<_>>();
    messages.push(assistant_message("recent tail", 200));

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Persisted capped IPC",
            false,
            1,
            200,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;
    engine.threads.write().await.remove(thread_id);
    engine
        .clear_thread_message_hydration_pending(thread_id)
        .await;

    let detail = engine
        .get_thread_capped_for_ipc(thread_id, false)
        .await
        .expect("persisted thread should load");

    assert_eq!(detail.total_message_count, 200);
    assert_eq!(detail.loaded_message_start, 136);
    assert_eq!(detail.loaded_message_end, 200);
    assert!(detail.messages_truncated);
    assert_eq!(
        detail
            .thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("recent tail")
    );
    assert!(
        engine.threads.read().await.get(thread_id).is_none(),
        "capped IPC detail should not restore the full persisted message list into memory"
    );
    assert!(
        engine
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "windowed detail should leave full message hydration pending"
    );
}

#[tokio::test]
async fn get_thread_capped_for_ipc_keeps_persisted_lazy_thread_unhydrated() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-lazy-capped-ipc";
    let huge_message = "x".repeat(MAX_FRAME_SIZE_BYTES + 1024);
    let mut messages = (0..198)
        .map(|index| AgentMessage::user(format!("message-{index}"), index))
        .collect::<Vec<_>>();
    messages.push(AgentMessage::user(huge_message, 199));
    messages.push(assistant_message("recent tail", 200));

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Lazy capped IPC",
            false,
            1,
            200,
            messages,
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let detail = rehydrated
        .get_thread_capped_for_ipc(thread_id, false)
        .await
        .expect("visible lazy thread should load");

    assert_eq!(detail.total_message_count, 200);
    assert!(
        detail.loaded_message_start >= 136,
        "capped lazy IPC should read only a bounded tail window"
    );
    assert!(detail.messages_truncated);
    assert_eq!(
        detail
            .thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("recent tail")
    );
    let in_memory = rehydrated.threads.read().await;
    let thread = in_memory
        .get(thread_id)
        .expect("thread summary should remain in memory");
    assert!(
        thread.messages.is_empty(),
        "capped IPC detail must not hydrate every persisted message into memory"
    );
    drop(in_memory);
    assert!(
        rehydrated
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id),
        "capped IPC detail should leave full message hydration pending"
    );
}

#[tokio::test]
async fn get_thread_filtered_hides_internal_threads_unless_requested() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "weles-hidden".to_string(),
        make_thread(
            "weles-hidden",
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden Weles",
            false,
            1,
            2,
            vec![weles_internal_message(1)],
        ),
    );

    assert!(engine
        .get_thread_filtered("weles-hidden", false, None, 0)
        .await
        .is_none());

    let detail = engine
        .get_thread_filtered("weles-hidden", true, None, 0)
        .await
        .expect("include_internal should reveal hidden thread");
    assert_eq!(detail.thread.id, "weles-hidden");
    assert!(!detail.messages_truncated);
}

#[tokio::test]
async fn get_thread_filtered_rejects_hidden_persisted_thread_without_hydrating_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "playground:hidden-persisted-detail";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Hidden persisted detail",
            false,
            1,
            3,
            vec![
                AgentMessage::user("hidden persisted user message", 1),
                assistant_message("hidden persisted assistant message", 3),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    assert!(
        rehydrated
            .get_thread_filtered(thread_id, false, None, 0)
            .await
            .is_none(),
        "default detail lookup should reject hidden persisted threads"
    );
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "rejecting a hidden persisted thread should not hydrate its messages into memory"
    );

    let detail = rehydrated
        .get_thread_filtered(thread_id, true, None, 0)
        .await
        .expect("include_internal should reveal hidden persisted thread");
    assert_eq!(detail.thread.id, thread_id);
    assert_eq!(detail.thread.messages.len(), 2);
    assert!(!detail.messages_truncated);
}

#[tokio::test]
async fn get_thread_filtered_rejects_persisted_weles_thread_without_hydrating_messages() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "weles-hidden-persisted-detail";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::WELES_AGENT_NAME),
            "Hidden persisted Weles",
            false,
            1,
            3,
            vec![
                weles_internal_message(1),
                assistant_message("hidden reply", 3),
            ],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    assert!(
        rehydrated
            .get_thread_filtered(thread_id, false, None, 0)
            .await
            .is_none(),
        "default detail lookup should reject persisted WELES internal threads"
    );
    assert!(
        rehydrated.threads.read().await.is_empty(),
        "rejecting a persisted WELES internal thread should not hydrate its messages into memory"
    );

    let detail = rehydrated
        .get_thread_filtered(thread_id, true, None, 0)
        .await
        .expect("include_internal should reveal persisted WELES thread");
    assert_eq!(detail.thread.id, thread_id);
    assert_eq!(detail.thread.messages.len(), 2);
}

#[tokio::test]
async fn agent_thread_detail_json_paginates_internal_dm_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine.threads.write().await.insert(
        "dm:svarog:weles".to_string(),
        make_thread(
            "dm:svarog:weles",
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Internal DM · Svarog ↔ WELES",
            false,
            1,
            4,
            vec![
                AgentMessage::user("message-0", 1),
                assistant_message("message-1", 2),
                AgentMessage::user("message-2", 3),
                assistant_message("message-3", 4),
            ],
        ),
    );

    let json = engine
        .agent_thread_detail_json("dm:svarog:weles", Some(2), Some(1), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("thread detail json");

    assert_eq!(value["id"].as_str(), Some("dm:svarog:weles"));
    assert_eq!(value["total_message_count"].as_u64(), Some(4));
    assert_eq!(value["loaded_message_start"].as_u64(), Some(1));
    assert_eq!(value["loaded_message_end"].as_u64(), Some(3));
    let messages = value["messages"]
        .as_array()
        .expect("messages should be serialized");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["content"].as_str(), Some("message-1"));
    assert_eq!(messages[1]["content"].as_str(), Some("message-2"));
}

#[tokio::test]
async fn thread_persistence_round_trips_offload_and_structural_refs() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-offload-refs";

    let mut message = AgentMessage::user("persist me", 1_000);
    message.offloaded_payload_id = Some("payload-123".to_string());
    message.tool_output_preview_path = Some(
        "/tmp/.zorai/.cache/tools/thread-thread-offload-refs/bash_command-1000.txt".to_string(),
    );
    message.structural_refs = vec![
        "artifact://summary/1".to_string(),
        "skill://brainstorming".to_string(),
    ];

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME),
            "Thread offload refs",
            false,
            1_000,
            1_000,
            vec![message],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should be restored after hydrate");
    let restored = thread
        .messages
        .first()
        .expect("thread should restore first message");

    assert_eq!(
        restored.offloaded_payload_id.as_deref(),
        Some("payload-123")
    );
    assert_eq!(
        restored.tool_output_preview_path.as_deref(),
        Some("/tmp/.zorai/.cache/tools/thread-thread-offload-refs/bash_command-1000.txt")
    );
    assert_eq!(
        restored.structural_refs,
        vec![
            "artifact://summary/1".to_string(),
            "skill://brainstorming".to_string(),
        ]
    );
}

#[tokio::test]
async fn agent_thread_detail_json_falls_back_to_message_provider_when_execution_profile_missing() {
    // Why this matters: threads opened from history (or created before the
    // execution-profile field was persisted) have no entry in
    // `thread_execution_profiles`. Without a fallback, the TUI would render
    // the header using the daemon's global config — which can be any other
    // agent the user happens to have selected in another window. Falling back
    // to the thread's own assistant-message provider/model keeps the header
    // anchored to the thread's actual responder.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-history-without-execution-profile";

    let mut assistant = assistant_message("hello from a sub-agent", 2);
    assistant.provider = Some("deepseek".to_string());
    assistant.model = Some("deepseek-v4-pro".to_string());

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("DeepSeekorrr"),
            "History thread without profile",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1), assistant],
        ),
    );

    // Ensure no execution profile is registered for this thread.
    engine
        .thread_execution_profiles
        .write()
        .await
        .remove(thread_id);

    let json = engine
        .agent_thread_detail_json(thread_id, Some(10), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");

    assert_eq!(value["profile_provider"].as_str(), Some("deepseek"));
    assert_eq!(value["profile_model"].as_str(), Some("deepseek-v4-pro"));
}

#[tokio::test]
async fn agent_thread_detail_json_skips_compaction_artifact_when_deriving_profile() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-compaction-artifact-profile";

    let mut older = assistant_message("older kept assistant", 1);
    older.provider = Some("openai".to_string());
    older.model = Some("gpt-5.4".to_string());

    let mut artifact = assistant_message("Pre-compaction context", 2);
    artifact.message_kind = AgentMessageKind::CompactionArtifact;
    artifact.provider = Some("openai".to_string());
    artifact.model = Some("gpt-5.4".to_string());

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Swarog"),
            "Compaction artifact profile",
            false,
            1,
            2,
            vec![older, artifact],
        ),
    );
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.5".to_string()),
            reasoning_effort: None,
            context_window_tokens: Some(200_000),
        },
    );

    let json = engine
        .agent_thread_detail_json(thread_id, Some(10), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");

    assert_eq!(value["profile_provider"].as_str(), Some("github-copilot"));
    assert_eq!(value["profile_model"].as_str(), Some("gpt-5.5"));
    assert_eq!(
        value["profile_context_window_tokens"].as_u64(),
        Some(200_000)
    );
}

#[tokio::test]
async fn sync_thread_execution_profiles_for_agent_updates_owned_threads() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-weles-owned";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Weles"),
            "Weles thread",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1)],
        ),
    );
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4".to_string()),
            reasoning_effort: Some("medium".to_string()),
            context_window_tokens: Some(128_000),
        },
    );

    let mut config = engine.get_config().await;
    config.builtin_sub_agents.weles.provider = Some("openai".to_string());
    config.builtin_sub_agents.weles.model = Some("gpt-5.4-mini".to_string());
    config.builtin_sub_agents.weles.context_window_tokens = Some(256_000);
    engine.set_config(config).await;
    engine
        .sync_thread_execution_profiles_for_agent("weles")
        .await;

    let profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("execution profile should exist");
    assert_eq!(profile.model.as_deref(), Some("gpt-5.4-mini"));
    assert_eq!(profile.context_window_tokens, Some(256_000));
}

#[tokio::test]
async fn set_sub_agent_syncs_owned_thread_execution_profiles() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-subagent-owned";
    let subagent_id = "reviewer";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Reviewer"),
            "Reviewer thread",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1)],
        ),
    );
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("ollama".to_string()),
            model: Some("glm-5.3:cloud".to_string()),
            reasoning_effort: None,
            context_window_tokens: None,
        },
    );

    engine
        .set_sub_agent(crate::agent::types::SubAgentDefinition {
            id: subagent_id.to_string(),
            name: "Reviewer".to_string(),
            provider: "z.ai-coding-plan".to_string(),
            model: "glm-5.3".to_string(),
            role: None,
            system_prompt: None,
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
            reasoning_effort: None,
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            base_url: None,
            claude_permission_mode: None,
            created_at: 1,
        })
        .await
        .expect("set sub-agent");

    let profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("execution profile should exist");
    assert_eq!(profile.provider.as_deref(), Some("z.ai-coding-plan"));
    assert_eq!(profile.model.as_deref(), Some("glm-5.3"));
}

#[tokio::test]
async fn set_sub_agent_syncs_owned_thread_execution_profiles_for_rod_builtin_persona() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-rod-owned";

    {
        let mut config = engine.get_config().await;
        config.builtin_sub_agents.rod.provider = Some("ollama".to_string());
        config.builtin_sub_agents.rod.model = Some("glm-5.3:cloud".to_string());
        engine.set_config(config).await;
    }

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Rod"),
            "Rod thread",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1)],
        ),
    );
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("ollama".to_string()),
            model: Some("glm-5.3:cloud".to_string()),
            reasoning_effort: None,
            context_window_tokens: None,
        },
    );

    engine
        .set_sub_agent(crate::agent::types::SubAgentDefinition {
            id: "rod".to_string(),
            name: "Rod".to_string(),
            provider: "z.ai-coding-plan".to_string(),
            model: "glm-5.3".to_string(),
            role: None,
            system_prompt: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_window_tokens: None,
            max_duration_secs: None,
            supervisor_config: None,
            enabled: true,
            builtin: true,
            immutable_identity: true,
            disable_allowed: false,
            delete_allowed: false,
            protected_reason: None,
            reasoning_effort: None,
            api_transport: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            huggingface_provider: None,
            base_url: None,
            claude_permission_mode: None,
            created_at: 1,
        })
        .await
        .expect("set rod builtin persona");

    let config = engine.get_config().await;
    assert_eq!(
        config.builtin_sub_agents.rod.provider.as_deref(),
        Some("z.ai-coding-plan")
    );
    assert_eq!(
        config.builtin_sub_agents.rod.model.as_deref(),
        Some("glm-5.3")
    );
    assert!(
        !config.sub_agents.iter().any(|entry| entry.id == "rod"),
        "rod should not be duplicated in sub_agents"
    );

    let profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("execution profile should exist");
    assert_eq!(profile.provider.as_deref(), Some("z.ai-coding-plan"));
    assert_eq!(profile.model.as_deref(), Some("glm-5.3"));
}

#[tokio::test]
async fn paged_thread_reload_keeps_a_live_model_change() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-paged-model-switch";
    let metadata_json = serde_json::json!({
        "execution_profile": {
            "provider": "openrouter",
            "model": "meta/muse-spark-1.3-contributor",
            "reasoning_effort": "high",
            "context_window_tokens": 1_000_000,
        }
    })
    .to_string();
    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("muse".to_string()),
            title: "Paged model switch".to_string(),
            created_at: 1,
            updated_at: 5_000,
            message_count: 1200,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: Some(metadata_json),
        })
        .await
        .expect("create persisted thread");
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(thread_id, Some("muse"), "Paged model switch", false, 1, 1_000, Vec::new()),
    );
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("openrouter".to_string()),
            model: Some("deepseek/deepseek-flash-latest".to_string()),
            reasoning_effort: Some("high".to_string()),
            context_window_tokens: Some(1_000_000),
        },
    );

    engine.persist_thread_by_id(thread_id).await;
    let skipped = engine
        .history
        .thread_metadata_json(thread_id)
        .await
        .expect("read metadata")
        .expect("metadata exists");
    assert!(
        skipped.contains("meta/muse-spark-1.3-contributor"),
        "a paged shell must not rewrite the newer persisted row, so the model change is not in that snapshot"
    );

    engine
        .restore_thread_from_db(thread_id)
        .await
        .expect("paged reload of the previous profile");
    assert_eq!(
        engine
            .get_thread_execution_profile(thread_id)
            .await
            .and_then(|profile| profile.model),
        Some("deepseek/deepseek-flash-latest".to_string()),
        "reloading a paged thread must not put the previous model back over the operator selection"
    );

    engine
        .persist_stored_thread_execution_profile(thread_id)
        .await;
    let patched = engine
        .history
        .thread_metadata_json(thread_id)
        .await
        .expect("read patched metadata")
        .expect("metadata exists");
    assert!(
        patched.contains("deepseek/deepseek-flash-latest"),
        "the selected model has to land in thread metadata even when the full snapshot is stale"
    );

    engine
        .thread_execution_profiles
        .write()
        .await
        .remove(thread_id);
    engine
        .restore_thread_from_db(thread_id)
        .await
        .expect("cold reload");
    assert_eq!(
        engine
            .get_thread_execution_profile(thread_id)
            .await
            .and_then(|profile| profile.model),
        Some("deepseek/deepseek-flash-latest".to_string()),
        "a later session has to load the model the operator selected"
    );
}

#[tokio::test]
async fn commit_thread_execution_profile_if_unchanged_skips_newer_user_selection() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-runtime-switch";
    let original = crate::agent::types::ThreadExecutionProfile {
        provider: Some("ollama".to_string()),
        model: Some("glm-5.3:cloud".to_string()),
        reasoning_effort: Some("high".to_string()),
        context_window_tokens: Some(1_000_000),
    };
    engine
        .thread_execution_profiles
        .write()
        .await
        .insert(thread_id.to_string(), original.clone());
    engine.thread_execution_profiles.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadExecutionProfile {
            provider: Some("z.ai-coding-plan".to_string()),
            model: Some("glm-5.3".to_string()),
            reasoning_effort: Some("high".to_string()),
            context_window_tokens: Some(1_000_000),
        },
    );

    let wrote = engine
        .commit_thread_execution_profile_if_unchanged(
            thread_id,
            Some(&original),
            crate::agent::types::ThreadExecutionProfile {
                provider: Some("ollama".to_string()),
                model: Some("glm-5.3:cloud".to_string()),
                reasoning_effort: Some("high".to_string()),
                context_window_tokens: Some(1_000_000),
            },
        )
        .await;

    assert!(
        !wrote,
        "a newer user-selected profile must not be overwritten"
    );
    let stored = engine
        .thread_execution_profiles
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("profile should remain");
    assert_eq!(stored.provider.as_deref(), Some("z.ai-coding-plan"));
    assert_eq!(stored.model.as_deref(), Some("glm-5.3"));
}

#[tokio::test]
async fn agent_thread_detail_json_backfills_agent_name_from_message_author_when_missing() {
    // Why this matters: older threads (or in-memory state that lost the field)
    // can be persisted without `agent_name`, making the TUI's sticky owner
    // label fall back to "Swarog". The latest assistant message's
    // `author_agent_name` is a reliable secondary source of truth — it was
    // stamped by the daemon at message creation under the actual responder
    // scope.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-history-without-agent-name";

    let mut assistant = assistant_message("answer from a sub-agent", 2);
    assistant.author_agent_name = Some("DeepSeekorrr".to_string());

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            None,
            "History thread without agent_name",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1), assistant],
        ),
    );

    let json = engine
        .agent_thread_detail_json(thread_id, Some(10), Some(0), false)
        .await;
    let value: serde_json::Value = serde_json::from_str(&json).expect("decode thread detail");

    assert_eq!(value["agent_name"].as_str(), Some("DeepSeekorrr"));
}

#[tokio::test]
async fn restore_thread_from_db_preserves_custom_sub_agent_name_instead_of_canonicalizing() {
    // Why this matters: when a thread is lazily restored from SQLite (e.g.
    // opened from history), the daemon hydrates `agent_name`. The
    // `canonical_agent_name` helper only knows the built-in personas and
    // collapses any unfamiliar id (e.g. a user-defined sub-agent UUID) back to
    // "Swarog". For custom sub-agents we must prefer the DB row's stored name
    // (or the responder stack frame) so the TUI's sticky owner label, the
    // rolling responder, and the [SubAgent] thread-tab placement all stay
    // consistent. Without this, opening DeepSeekorrr's thread from history
    // would silently relabel the responder to "Swarog".
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-restore-preserves-custom-name";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("DeepSeekorrr"),
            "Custom sub-agent thread",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1)],
        ),
    );
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: "subagent-1777065727944".to_string(),
                active_agent_id: "subagent-1777065727944".to_string(),
                responder_stack: vec![ThreadResponderFrame {
                    agent_id: "subagent-1777065727944".to_string(),
                    agent_name: "DeepSeekorrr".to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;
    engine.persist_thread_by_id(thread_id).await;

    // Cold-start a fresh engine and force a lazy restore from the DB.
    let cold = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    cold.threads.write().await.remove(thread_id);
    let restored = cold
        .restore_thread_from_db(thread_id)
        .await
        .expect("thread should restore from db");

    assert_eq!(
        restored.agent_name.as_deref(),
        Some("DeepSeekorrr"),
        "restored thread must keep the custom sub-agent name, not collapse to Swarog"
    );
}

#[tokio::test]
async fn restore_thread_from_db_hydrates_fork_execution_profile_and_responder() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-restore-fork-profile";
    let metadata_json = serde_json::json!({
        "source": "tui_message_fork",
        "execution_profile": {
            "provider": "groq",
            "model": "llama-3.3-70b-versatile",
            "reasoning_effort": "high",
            "context_window_tokens": 456789,
        },
        "thread_profile": {
            "provider": "groq",
            "model": "llama-3.3-70b-versatile",
            "reasoning_effort": "high",
            "context_window_tokens": 456789,
        },
        "origin_agent_id": "design-reviewer",
        "active_agent_id": "design-reviewer",
        "handoff_stack": [
            {
                "agent_id": "design-reviewer",
                "agent_name": "Design Reviewer",
                "entered_at": 12,
            }
        ],
        "handoff_events": [],
    })
    .to_string();

    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Design Reviewer".to_string()),
            title: "Forked sub-agent thread".to_string(),
            created_at: 12,
            updated_at: 12,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: Some(metadata_json),
        })
        .await
        .expect("create thread");

    let restored = engine
        .restore_thread_from_db(thread_id)
        .await
        .expect("thread should restore from db");
    assert_eq!(restored.agent_name.as_deref(), Some("Design Reviewer"));
    let profile = engine
        .thread_execution_profiles
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("execution profile should hydrate");
    assert_eq!(profile.provider.as_deref(), Some("groq"));
    assert_eq!(profile.model.as_deref(), Some("llama-3.3-70b-versatile"));
    assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(profile.context_window_tokens, Some(456_789));

    let handoff = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should hydrate");
    assert_eq!(handoff.active_agent_id, "design-reviewer");
    assert_eq!(
        handoff
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.as_str()),
        Some("Design Reviewer")
    );
}

#[tokio::test]
async fn restore_thread_from_db_preserves_custom_sub_agent_name_when_participants_present() {
    // Why this matters: once a user adds a participant (e.g. @Mokosh) to a
    // sub-agent-owned thread, the `has_thread_participants` branch of
    // `visible_thread_owner_agent_name_for_handoff_state` activates. Before this
    // guard, that branch went straight to `canonical_agent_name(origin_agent_id)`,
    // which collapses unfamiliar sub-agent UUIDs back to "Swarog" — silently
    // relabelling the thread's responder the moment the participant joined.
    // The owner-name lookup must walk the responder stack first so the
    // sub-agent's display name survives across the participant lifecycle.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-preserves-owner-name";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("DeepSeekorrr"),
            "Sub-agent thread with participant",
            false,
            1,
            2,
            vec![AgentMessage::user("hi", 1)],
        ),
    );
    engine.thread_participants.write().await.insert(
        thread_id.to_string(),
        vec![ThreadParticipantState {
            agent_id: "mokosh".to_string(),
            agent_name: "Mokosh".to_string(),
            instruction: "observe and chime in".to_string(),
            status: ThreadParticipantStatus::Active,
            created_at: 1,
            updated_at: 1,
            deactivated_at: None,
            last_contribution_at: None,
            last_observed_visible_message_at: None,
            always_auto_response: false,
            last_contributed_epoch: None,
            duplicate_suppression_streak: 0,
            last_suppressed_at: None,
        }],
    );
    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: "subagent-1777065727944".to_string(),
                active_agent_id: "subagent-1777065727944".to_string(),
                responder_stack: vec![ThreadResponderFrame {
                    agent_id: "subagent-1777065727944".to_string(),
                    agent_name: "DeepSeekorrr".to_string(),
                    entered_at: 1,
                    entered_via_handoff_event_id: None,
                    linked_thread_id: None,
                }],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;
    engine.persist_thread_by_id(thread_id).await;

    let cold = AgentEngine::new_test(
        SessionManager::new_test(root.path()).await,
        AgentConfig::default(),
        root.path(),
    )
    .await;
    cold.threads.write().await.remove(thread_id);
    let restored = cold
        .restore_thread_from_db(thread_id)
        .await
        .expect("thread should restore from db");

    assert_eq!(
        restored.agent_name.as_deref(),
        Some("DeepSeekorrr"),
        "owner-name lookup must walk the responder stack when participants are present, \
         not collapse the sub-agent id to Swarog"
    );
}

#[tokio::test]
async fn ensure_thread_messages_loaded_preserves_in_memory_tail_added_after_hydration_marker() {
    // Why this matters: when the TUI fetches a paged thread detail, the daemon
    // re-marks the thread for lazy hydration. If the agent loop then completes a
    // turn and calls persist_thread_by_id, persist first calls
    // ensure_thread_messages_loaded. Before this guard, that call blindly
    // overwrote thread.messages with the DB snapshot — wiping the
    // just-appended assistant message before persistence could write it back.
    // The result was the user's "latest agent message constantly removed" bug.
    // The hydration loader must defer to the in-memory tail when it's ahead of
    // the DB.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-pending-hydration-preserves-tail";

    let mut persisted_assistant = assistant_message("persisted assistant", 2);
    persisted_assistant.id = "msg-persisted-assistant".to_string();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("DeepSeekorrr"),
            "Pending hydration thread",
            false,
            1,
            2,
            vec![AgentMessage::user("user-1", 1), persisted_assistant],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    // Simulate the TUI fetching a paged thread detail — `restore_thread_from_db`
    // (called inside that code path) re-inserts the thread into the lazy
    // hydration set.
    engine
        .thread_message_hydration_pending
        .write()
        .await
        .insert(thread_id.to_string());

    // Now simulate the agent loop appending a fresh assistant message in memory
    // before the persistence call has the chance to write it through.
    {
        let mut threads = engine.threads.write().await;
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        let mut new_assistant = assistant_message("new assistant tail", 3);
        new_assistant.id = "msg-new-assistant".to_string();
        thread.messages.push(new_assistant);
        thread.updated_at = 3;
    }

    // ensure_thread_messages_loaded must not wipe the in-memory tail.
    assert!(engine.ensure_thread_messages_loaded(thread_id).await);

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread retained");
    assert_eq!(thread.messages.len(), 3, "in-memory tail must survive");
    assert_eq!(
        thread.messages.last().map(|m| m.id.as_str()),
        Some("msg-new-assistant"),
        "newly appended assistant tail must remain after hydration check"
    );
}

#[tokio::test]
async fn get_thread_does_not_replace_live_tail_with_stale_db_snapshot() {
    // Why this matters: MLflow enrichment used to call get_thread() on every
    // agent event. get_thread prefers get_authoritative_persisted_thread_detail,
    // which loaded SQLite and overwrote live memory. During a turn that wipe
    // dropped streaming assistant/tool rows; persist then wrote the truncated
    // snapshot back. Both GUI and TUI then showed missing messages even after
    // reload, and the UI treated the full (wiped) window as complete so scroll-up
    // stopped requesting older pages.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-get-thread-preserves-live-tail";

    let mut persisted_assistant = assistant_message("persisted assistant", 2);
    persisted_assistant.id = "msg-persisted-assistant".to_string();
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Svarog"),
            "Live tail thread",
            false,
            1,
            2,
            vec![AgentMessage::user("user-1", 1), persisted_assistant],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut threads = engine.threads.write().await;
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        let mut new_assistant = assistant_message("new assistant tail", 3);
        new_assistant.id = "msg-new-assistant".to_string();
        thread.messages.push(new_assistant);
        thread.updated_at = 3;
    }

    let detail = engine
        .get_thread(thread_id)
        .await
        .expect("thread should remain readable");
    assert_eq!(
        detail.messages.len(),
        3,
        "returned detail must keep the live tail"
    );
    assert_eq!(
        detail.messages.last().map(|m| m.id.as_str()),
        Some("msg-new-assistant")
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread retained");
    assert_eq!(
        thread.messages.len(),
        3,
        "in-memory tail must survive get_thread"
    );
    assert_eq!(
        thread.messages.last().map(|m| m.content.as_str()),
        Some("new assistant tail")
    );
}

#[tokio::test]
async fn get_thread_does_not_replace_richer_live_tool_payload_with_empty_db_stub() {
    // Why this matters: streaming tool-call rows keep the same id/timestamp as
    // the empty stub persisted at creation. Count-and-tail-id checks treat that
    // as "not ahead", so get_thread used to restore the empty DB stub over the
    // filled live payload. Those empty Svarog headers then survived reload.
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-get-thread-preserves-richer-tool";

    let mut stub = assistant_message("", 2);
    stub.id = "msg-tool-stub".to_string();
    stub.role = MessageRole::Tool;
    stub.tool_name = Some("bash_command".to_string());
    engine.threads.write().await.insert(
        thread_id.to_string(),
        make_thread(
            thread_id,
            Some("Svarog"),
            "Tool stub thread",
            false,
            1,
            2,
            vec![AgentMessage::user("run ls", 1), stub],
        ),
    );
    engine.persist_thread_by_id(thread_id).await;

    {
        let mut threads = engine.threads.write().await;
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        let live = thread
            .messages
            .last_mut()
            .expect("tool stub should exist in memory");
        live.tool_arguments = Some(r#"{"command":"ls"}"#.to_string());
        live.content = "file.txt\n".to_string();
    }

    let detail = engine
        .get_thread(thread_id)
        .await
        .expect("thread should remain readable");
    let returned = detail.messages.last().expect("tool message");
    assert_eq!(returned.content, "file.txt\n");
    assert_eq!(
        returned.tool_arguments.as_deref(),
        Some(r#"{"command":"ls"}"#)
    );

    let threads = engine.threads.read().await;
    let live = threads
        .get(thread_id)
        .expect("thread retained")
        .messages
        .last()
        .expect("live tool message");
    assert_eq!(
        live.tool_arguments.as_deref(),
        Some(r#"{"command":"ls"}"#),
        "empty DB stub must not replace the filled live tool payload"
    );
    assert_eq!(live.content, "file.txt\n");
}
