use super::*;

#[test]
fn direct_message_entrypoints_box_large_send_message_futures() {
    let messaging_production = [
        repo_root().join("crates/zorai-daemon/src/agent/messaging.rs"),
        repo_root().join("crates/zorai-daemon/src/agent/messaging/direct_messages.rs"),
    ]
    .into_iter()
    .map(|path| fs::read_to_string(path).expect("read messaging source"))
    .collect::<Vec<_>>()
    .join("\n");
    let send_message_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/agent_loop/send_message/mod.rs"),
    )
    .expect("read send_message/mod.rs");
    let send_message_production = send_message_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(send_message_source.as_str());

    for required in [
        "Box::pin(self.send_message_inner(",
        "let outcome = Box::pin(self.send_message_inner(",
        "Some(\n                Box::pin(self.send_message_inner(",
    ] {
        assert!(
            messaging_production.contains(required),
            "messaging entrypoint should box oversized future: {required}"
        );
    }

    assert!(
        send_message_production
            .contains("Box::pin(run_with_agent_scope(agent_scope_id, async move {"),
        "send_message_inner should box the oversized agent loop future"
    );
    assert!(
        send_message_production.contains("Box::pin(self.run_internal_send_loop("),
        "send_message_inner should box the oversized internal send-loop future"
    );
    assert!(
        send_message_production.contains("let outcome = match Box::pin(runner.run()).await {"),
        "run_internal_send_loop should box the oversized SendMessageRunner future"
    );
    assert!(
        messaging_production.contains("Box::pin(self.send_internal_agent_message("),
        "async internal DM worker should box the oversized send future"
    );

    assert!(
        messaging_production.contains(
            "self.send_message_inner(\n            thread_id, content, None, None, None, None, None, None, None, true,"
        ) || messaging_production.contains(
            "self.send_message_inner(thread_id, content, None, None, None, None, None, None, None, true)"
        ),
        "operator-facing send_message should mark Copilot initiator as user"
    );
    assert!(
        messaging_production.contains(
            "backend_override,\n            None,\n            None,\n            client_surface,\n            false,"
        ),
        "send_task_message should mark Copilot initiator as agent"
    );
}

#[tokio::test]
async fn task_thread_id_conflicts_finds_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-persisted-task-conflict";

    let mut task = engine
        .enqueue_task(
            "Persisted task thread".to_string(),
            "reserve_unique_thread_id should avoid this persisted thread id".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    task.thread_id = Some(thread_id.to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        tasks.clear();
        tasks.push_back(task.clone());
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    assert_eq!(task.thread_id.as_deref(), Some(thread_id));
    assert!(engine.task_thread_id_conflicts(thread_id).await);
}

#[test]
fn daemon_generated_message_paths_use_internal_initiator() {
    let heartbeat =
        fs::read_to_string(repo_root().join("crates/zorai-daemon/src/agent/heartbeat.rs"))
            .expect("read heartbeat.rs");
    let heartbeat_legacy =
        fs::read_to_string(repo_root().join("crates/zorai-daemon/src/agent/heartbeat/legacy.rs"))
            .expect("read heartbeat/legacy.rs");
    let gateway_message_helpers = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/gateway_loop/message_helpers.rs"),
    )
    .expect("read gateway_loop/message_helpers.rs");

    assert!(
        heartbeat.contains("send_internal_message_as(")
            && heartbeat.contains("WELES_AGENT_ID")
            && heartbeat.contains("&synthesis_prompt"),
        "heartbeat synthesis should route daemon-owned work through Weles"
    );
    assert!(
        heartbeat_legacy.contains("send_internal_message_as(")
            && heartbeat_legacy.contains("WELES_AGENT_ID")
            && heartbeat_legacy.contains("&prompt"),
        "legacy heartbeat checks should route daemon-owned work through Weles"
    );
    assert!(
        gateway_message_helpers.contains("send_gateway_platform_tool(\"\", \"gateway_reset\"")
            && !gateway_message_helpers.contains("send_internal_message(None"),
        "gateway reset confirmations must reply directly via the gateway tool (no agent turn) so \
         they are not WELES-blocked and never route through the main agent"
    );
}

#[tokio::test]
async fn send_task_message_inherits_goal_surface_from_persisted_task_after_live_queue_clear() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-send-task-surface";

    let task = engine
        .enqueue_task(
            "Persisted task surface".to_string(),
            "task message should inherit goal surface from persisted metadata".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "goal_run",
            Some(goal_run_id.to_string()),
            None,
            None,
            Some("daemon".to_string()),
        )
        .await;
    engine
        .set_goal_run_client_surface(goal_run_id, zorai_protocol::ClientSurface::Tui)
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let result = engine
        .send_task_message(
            &task.id,
            None,
            None,
            Some("missing-provider-for-surface-test"),
            "surface inheritance probe",
        )
        .await;

    assert!(
        result.is_err(),
        "invalid provider should stop before any network request"
    );
    assert!(
        engine
            .thread_client_surfaces
            .read()
            .await
            .values()
            .any(|surface| *surface == zorai_protocol::ClientSurface::Tui),
        "new task thread should inherit the TUI surface from the persisted task's goal run"
    );
}

#[tokio::test]
async fn send_task_message_initializes_persisted_task_thread_after_live_queue_clear() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let task = engine
        .enqueue_task(
            "Persisted task thread".to_string(),
            "task message should persist the created thread id".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            None,
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let result = engine
        .send_task_message(
            &task.id,
            None,
            None,
            Some("missing-provider-for-thread-init-test"),
            "thread initialization probe",
        )
        .await;

    assert!(
        result.is_err(),
        "invalid provider should stop before any network request"
    );

    let persisted = engine
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

    assert!(
        persisted.thread_id.is_some(),
        "send setup should persist the generated task thread id"
    );
}

#[test]
fn tool_execution_hot_path_boxes_large_futures() {
    let finalize_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/agent_loop/send_message/finalize.rs"),
    )
    .expect("read finalize.rs");
    let finalize_production = finalize_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(finalize_source.as_str());
    let tool_calls_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/agent_loop/send_message/tool_calls.rs"),
    )
    .expect("read tool_calls.rs");
    let tool_calls_production = tool_calls_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(tool_calls_source.as_str());
    let execute_tool_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/tool_executor/execute_tool_impl.rs"),
    )
    .expect("read execute_tool_impl.rs");
    let execute_tool_production = execute_tool_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(execute_tool_source.as_str());
    let subagents_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/tool_executor/subagents.rs"),
    )
    .expect("read subagents.rs");
    let subagents_production = subagents_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(subagents_source.as_str());
    let thread_participants_source = fs::read_to_string(
        repo_root().join("crates/zorai-daemon/src/agent/thread_participants.rs"),
    )
    .expect("read thread_participants.rs");
    let thread_participants_production = thread_participants_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(thread_participants_source.as_str());

    assert!(
        finalize_production.contains("Box::pin(self.handle_tool_calls_chunk("),
        "tool-call iteration handling should box the large handle_tool_calls_chunk future"
    );
    assert!(
        finalize_production.contains(
            "Box::pin(self.engine\n            .maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || finalize_production.contains(
            "Box::pin(self.engine.maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || finalize_production.contains(
            "Box::pin(\n            self.engine\n                .maybe_auto_send_gateway_thread_response(&self.tid),\n        )"
        ),
        "done-chunk finalization should box gateway auto-send futures"
    );
    assert!(
        finalize_production.contains(
            "Box::pin(self.engine.flush_deferred_visible_thread_continuations(&outcome.thread_id))"
        ) || finalize_production.contains(
            "Box::pin(\n            self.engine\n                .flush_deferred_visible_thread_continuations(&outcome.thread_id)"
        ),
        "done-chunk finalization should box visible-thread continuation flush futures"
    );
    assert!(
        tool_calls_production.contains("Box::pin(execute_tool("),
        "tool execution callsites should box execute_tool futures"
    );
    assert!(
        tool_calls_production.contains(
            "Box::pin(self.engine\n            .maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || tool_calls_production.contains(
            "Box::pin(self.engine.maybe_auto_send_gateway_thread_response(&self.tid))"
        ) || tool_calls_production.contains(
            "Box::pin(\n            self.engine\n                .maybe_auto_send_gateway_thread_response(&self.tid),\n        )"
        ),
        "tool-call chunk handling should box gateway auto-send futures"
    );
    assert!(
        subagents_production.contains(".enqueue_internal_agent_message("),
        "message_agent should enqueue the internal DM instead of waiting for a nested reply"
    );
    assert!(
        thread_participants_production.contains("Box::pin(self.send_internal_agent_message("),
        "internal delegate path should box the oversized internal-agent send future"
    );
    assert!(
        thread_participants_production
            .contains(".build_internal_delegate_payload(thread_id, content, false)"),
        "internal delegate path should disable visible-thread continuation"
    );
    assert!(
        thread_participants_production.contains("run_deferred_visible_thread_continuation("),
        "visible-thread continuation flush should route continuation work through the deferred helper"
    );
    assert!(
        thread_participants_production.contains(
            "Box::pin(\n                        self.run_deferred_visible_thread_continuation(thread_id, continuation),"
        ) || thread_participants_production.contains(
            "Box::pin(self.run_deferred_visible_thread_continuation(thread_id, continuation))"
        ),
        "visible-thread continuation flush should box the deferred continuation helper"
    );
    assert!(
        thread_participants_production.contains("Box::pin(self.continue_visible_thread_as_agent("),
        "deferred continuation helper should box the oversized continuation future"
    );
    let continue_visible_thread_prelude = thread_participants_production
        .split("async fn continue_visible_thread_as_agent(")
        .nth(1)
        .and_then(|source| source.split("if force_compaction {").next())
        .expect("continue_visible_thread_as_agent prelude should be present");
    assert!(
        continue_visible_thread_prelude.contains("ensure_thread_messages_loaded(thread_id).await"),
        "visible-thread continuation should hydrate persisted messages before checking live thread state"
    );
    assert!(
        execute_tool_production.contains("pub fn execute_tool<'a>("),
        "execute_tool should return an explicitly boxed future instead of an inline async fn"
    );
    assert!(
        execute_tool_production.contains(
            "-> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>>"
        ),
        "execute_tool should expose a boxed future return type"
    );
    assert!(
        execute_tool_production.contains("Box::pin(async move {"),
        "execute_tool implementation should heap-box its async state machine"
    );
}

#[test]
fn participant_compaction_ignores_owner_pins() {
    // Compaction was extracted into a submodule (`compaction/`) — the
    // owner-pin handling now lives in `compaction/request.rs` rather
    // than the original flat `compaction.rs`. Walk the whole submodule
    // tree so this guard rail keeps verifying intent regardless of file
    // splits.
    let compaction_dir = repo_root().join("crates/zorai-daemon/src/agent/compaction");
    let mut combined_source = String::new();
    for entry in fs::read_dir(&compaction_dir).expect("read compaction dir") {
        let entry = entry.expect("read compaction dir entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        combined_source.push_str(&text);
        combined_source.push('\n');
    }
    // Also include the top-level `compaction.rs` (module declarations).
    if let Ok(text) =
        fs::read_to_string(repo_root().join("crates/zorai-daemon/src/agent/compaction.rs"))
    {
        combined_source.push_str(&text);
    }

    assert!(
        combined_source.contains("owner_only_pins")
            || combined_source.contains("ignore_owner_pins")
            || combined_source.contains("participant_pins"),
        "participant request paths should explicitly suppress owner pins during compaction"
    );
}

#[tokio::test]
async fn delete_thread_messages_updates_live_thread_and_persisted_history() {
    let root = tempdir().unwrap();

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_test";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Test".to_string(),
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
                    AgentMessage::user("first", 1),
                    AgentMessage::user("second", 2),
                    AgentMessage::user("third", 3),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    let msg_id = {
        let threads = engine.threads.read().await;
        threads.get(thread_id).unwrap().messages[1].id.clone()
    };
    let deleted = engine
        .delete_thread_messages(thread_id, &[msg_id])
        .await
        .expect("delete should succeed");
    assert_eq!(deleted, 1);

    let live = engine.threads.read().await;
    let thread = live.get(thread_id).expect("thread should still exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "first");
    assert_eq!(thread.messages[1].content, "third");
    drop(live);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].content, "first");
    assert_eq!(persisted[1].content, "third");
}

#[tokio::test]
async fn get_or_create_thread_with_target_sets_requested_initial_responder() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let (thread_id, created) = engine
        .get_or_create_thread_with_target(None, "Talk to Weles", Some("weles"))
        .await;

    assert!(created, "new target-scoped thread should be created");
    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));

    let handoff_state = engine
        .thread_handoff_state(&thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(handoff_state.active_agent_id, WELES_AGENT_ID);
    assert_eq!(
        handoff_state
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.as_str()),
        Some(WELES_AGENT_NAME)
    );
}

#[tokio::test]
async fn get_or_create_thread_with_target_preserves_user_defined_subagent_identity() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: "dola".to_string(),
        name: "Dola".to_string(),
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
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1,
    });
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let (thread_id, created) = engine
        .get_or_create_thread_with_target(None, "Talk to Dola", Some("dola"))
        .await;

    assert!(created, "new target-scoped thread should be created");
    let thread = engine
        .threads
        .read()
        .await
        .get(&thread_id)
        .cloned()
        .expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some("Dola"));

    let handoff_state = engine
        .thread_handoff_state(&thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(handoff_state.active_agent_id, "dola");
    assert_eq!(
        handoff_state
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.as_str()),
        Some("Dola")
    );
}

#[tokio::test]
async fn thread_client_surface_persists_with_thread_metadata() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_surface";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Surface".to_string(),
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
                messages: vec![AgentMessage::user("hello", 1)],
            },
        );
    }
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"client_surface\":\"tui\""));

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    rehydrated.hydrate().await.expect("hydrate");
    assert_eq!(
        rehydrated.get_thread_client_surface(thread_id).await,
        Some(zorai_protocol::ClientSurface::Tui)
    );
}

#[tokio::test]
async fn thread_client_surface_setter_updates_persisted_thread_without_live_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_surface_persisted_only";

    engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Persisted only surface".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: Some(serde_json::json!({ "pinned": true }).to_string()),
        })
        .await
        .expect("seed persisted thread");
    assert!(
        engine.threads.read().await.is_empty(),
        "test setup should leave thread outside the live map"
    );

    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Tui)
        .await;

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    rehydrated.hydrate().await.expect("hydrate");

    assert_eq!(
        rehydrated.get_thread_client_surface(thread_id).await,
        Some(zorai_protocol::ClientSurface::Tui)
    );
    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should hydrate");
    assert!(thread.pinned, "unrelated metadata should be preserved");
}

#[tokio::test]
async fn thread_handoff_state_persists_and_restores_active_agent_identity() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_state";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Handoff".to_string(),
                created_at: 1,
                updated_at: 2,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![AgentMessage::user("operator asks for audit", 1)],
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
                        entered_via_handoff_event_id: Some("handoff-1".to_string()),
                        linked_thread_id: Some("dm:svarog:weles".to_string()),
                    },
                ],
                events: vec![ThreadHandoffEvent {
                    id: "handoff-1".to_string(),
                    kind: ThreadHandoffKind::Push,
                    from_agent_id: MAIN_AGENT_ID.to_string(),
                    to_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                    from_agent_name: None,
                    to_agent_name: None,
                    requested_by: ThreadHandoffRequestedBy::Agent,
                    reason: "Security review needed".to_string(),
                    summary: "Asked Weles to inspect risky changes.".to_string(),
                    linked_thread_id: Some("dm:svarog:weles".to_string()),
                    approval_id: None,
                    stack_depth_before: 1,
                    stack_depth_after: 2,
                    created_at: 2,
                    approved_at: None,
                    completed_at: Some(2),
                    failed_at: None,
                    failure_reason: None,
                }],
                pending_approval_id: Some("approval-handoff-1".to_string()),
            },
        )
        .await;
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    assert_eq!(persisted.agent_name.as_deref(), Some("Weles"));
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"active_agent_id\":\"weles\""));
    assert!(metadata.contains("\"pending_handoff_approval_id\":\"approval-handoff-1\""));

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    rehydrated.hydrate().await.expect("hydrate");

    let restored = rehydrated
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should restore");
    assert_eq!(restored.origin_agent_id, MAIN_AGENT_ID);
    assert_eq!(
        restored.active_agent_id,
        crate::agent::agent_identity::WELES_AGENT_ID
    );
    assert_eq!(restored.responder_stack.len(), 2);

    let restored_thread = rehydrated
        .threads
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("thread should reload");
    assert_eq!(restored_thread.agent_name.as_deref(), Some("Weles"));
}

#[tokio::test]
async fn operator_turns_route_through_active_thread_responder_scope() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_active_responder_scope";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Active responder".to_string(),
                created_at: 1,
                updated_at: 2,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![AgentMessage::user("continue from Weles", 1)],
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
                        entered_via_handoff_event_id: Some("handoff-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread_active_responder_scope:handoff-1".to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    assert_eq!(
        engine.agent_scope_id_for_turn(Some(thread_id), None).await,
        crate::agent::agent_identity::WELES_AGENT_ID,
        "operator turns should run under the active responder scope"
    );
    assert_eq!(
        engine.agent_scope_id_for_turn(None, None).await,
        MAIN_AGENT_ID,
        "new threads should still default to the main agent scope"
    );
}

#[tokio::test]
async fn agent_scope_id_for_turn_resolves_persisted_task_scope() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut task = engine
        .enqueue_task(
            "Persisted turn scope".to_string(),
            "Resolve turn scope from a persisted task.".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            None,
            Some("daemon".to_string()),
        )
        .await;
    task.override_system_prompt =
        Some("Agent persona id: persisted-turn-scope\nUse task-local scope.".to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        let persisted = tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
            .expect("enqueued task should exist in live queue");
        *persisted = task.clone();
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    assert_eq!(
        engine.agent_scope_id_for_turn(None, Some(&task.id)).await,
        "persisted-turn-scope"
    );
}

#[tokio::test]
async fn agent_scope_id_for_turn_recovers_thread_owner_without_handoff_state() {
    // Why this matters: multi-window/multi-thread scenarios previously caused the daemon
    // to misattribute follow-up turns. When a sub-agent-owned thread (e.g. Weles) had no
    // handoff state and the turn arrived without a task_id, the resolver fell back to
    // MAIN_AGENT_ID, running the LLM under the wrong identity and stamping assistant
    // messages with the main agent's author_agent_name. The thread's persisted owner is
    // the authoritative fallback before MAIN_AGENT_ID.
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_owner_without_handoff";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Sub-agent owner".to_string(),
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
                messages: vec![AgentMessage::user("hello", 1)],
            },
        );
    }

    assert_eq!(
        engine.agent_scope_id_for_turn(Some(thread_id), None).await,
        crate::agent::agent_identity::WELES_AGENT_ID,
        "sub-agent-owned thread should resolve to its owner scope, not MAIN_AGENT_ID"
    );
}

#[tokio::test]
async fn agent_scope_id_for_turn_preserves_unregistered_custom_sub_agent_owner() {
    // Why this matters: a user-defined custom sub-agent (e.g. "DeepSeekorrr") may not be
    // resolvable via `resolve_agent_target` if it isn't registered in the sub-agent list
    // at the moment of resolution (timing, plugin not loaded yet, etc.). The scope must
    // still derive from the thread's persisted owner — falling back to MAIN_AGENT_ID
    // would re-introduce the multi-window bug where window 1's Swarog identity leaks
    // into window 2's custom-owned thread.
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_unregistered_custom_owner";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some("DeepSeekorrr".to_string()),
                title: "Custom owner".to_string(),
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
                messages: vec![AgentMessage::user("hello", 1)],
            },
        );
    }

    assert_eq!(
        engine.agent_scope_id_for_turn(Some(thread_id), None).await,
        "deepseekorrr",
        "unregistered custom owner must scope to its own alias, not MAIN_AGENT_ID"
    );
}

#[tokio::test]
async fn handoff_activation_clears_thread_continuation_state_for_new_responder_stream() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_clears_continuity";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Handoff continuity".to_string(),
                created_at: 1,
                updated_at: 3,
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("first", 1),
                    AgentMessage {
                        id: "assistant-anchor".to_string(),
                        role: MessageRole::Assistant,
                        content: "answer".to_string(),
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
                        provider: Some(
                            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_123".to_string()),
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
                    AgentMessage::user("continue", 3),
                ],
            },
        );
    }

    let request = crate::agent::PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind: crate::agent::ThreadHandoffKind::Push,
        target_agent_id: Some("weles".to_string()),
        requested_by: crate::agent::ThreadHandoffRequestedBy::User,
        reason: "Operator asked to talk to Weles".to_string(),
        summary: "Switch active responder to Weles".to_string(),
    };

    engine
        .apply_thread_handoff_activation(&request, None)
        .await
        .expect("handoff activation should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread.upstream_thread_id.is_none());
    assert!(thread.upstream_transport.is_none());
    assert!(thread.upstream_provider.is_none());
    assert!(thread.upstream_model.is_none());
    assert!(thread
        .messages
        .iter()
        .all(|message| message.response_id.is_none()));
}

#[tokio::test]
async fn handoff_activation_emits_thread_reload_event_for_visible_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_handoff_reload_event";
    let mut events = engine.subscribe();

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Reload me after handoff".to_string(),
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
                messages: vec![AgentMessage::user("switch me", 1)],
            },
        );
    }

    while events.try_recv().is_ok() {}

    let request = crate::agent::PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind: crate::agent::ThreadHandoffKind::Push,
        target_agent_id: Some("weles".to_string()),
        requested_by: crate::agent::ThreadHandoffRequestedBy::User,
        reason: "Operator asked to talk to Weles".to_string(),
        summary: "Switch active responder to Weles".to_string(),
    };

    engine
        .apply_thread_handoff_activation(&request, None)
        .await
        .expect("handoff activation should succeed");

    let mut saw_reload = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::ThreadReloadRequired {
            thread_id: event_thread_id,
        } = event
        {
            assert_eq!(event_thread_id, thread_id);
            saw_reload = true;
            break;
        }
    }

    assert!(
        saw_reload,
        "handoff activation should emit a thread reload event"
    );
}

fn configured_user_subagent(id: &str, name: &str, enabled: bool) -> SubAgentDefinition {
    SubAgentDefinition {
        base_url: None,
        claude_permission_mode: None,
        id: id.to_string(),
        name: name.to_string(),
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
        enabled,
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
    }
}

async fn insert_plain_handoff_thread(engine: &AgentEngine, thread_id: &str) {
    let mut threads = engine.threads.write().await;
    threads.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Code thread".to_string(),
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
            messages: vec![AgentMessage::user("switch me", 1)],
        },
    );
}

fn push_handoff_request(
    thread_id: &str,
    target_agent_id: &str,
) -> crate::agent::PendingThreadHandoffActivation {
    crate::agent::PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind: crate::agent::ThreadHandoffKind::Push,
        target_agent_id: Some(target_agent_id.to_string()),
        requested_by: crate::agent::ThreadHandoffRequestedBy::User,
        reason: "Operator switched the Code thread runtime".to_string(),
        summary: "Switch active responder to the selected subagent".to_string(),
    }
}

#[tokio::test]
async fn handoff_activation_accepts_configured_user_subagent_on_plain_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    let subagent_id = "subagent-1781800311670";
    config
        .sub_agents
        .push(configured_user_subagent(subagent_id, "hf", true));
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_code_user_subagent_handoff";
    insert_plain_handoff_thread(&engine, thread_id).await;

    engine
        .apply_thread_handoff_activation(&push_handoff_request(thread_id, subagent_id), None)
        .await
        .expect(
            "configured user subagents must be valid handoff targets on non-participant threads",
        );

    let state = engine
        .thread_handoff_state(thread_id)
        .await
        .expect("handoff state should exist");
    assert_eq!(state.active_agent_id, subagent_id);
    assert_eq!(
        state
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.as_str()),
        Some("hf")
    );
}

#[tokio::test]
async fn handoff_activation_rejects_unknown_or_disabled_user_subagent_on_plain_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(configured_user_subagent(
        "subagent-disabled",
        "disabled-specialist",
        false,
    ));
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_code_unknown_subagent_handoff";
    insert_plain_handoff_thread(&engine, thread_id).await;

    let unknown_error = engine
        .apply_thread_handoff_activation(&push_handoff_request(thread_id, "subagent-missing"), None)
        .await
        .expect_err("unknown user subagents must not become handoff targets");
    assert!(
        unknown_error
            .to_string()
            .contains("unknown handoff target: subagent-missing"),
        "unexpected error: {unknown_error}"
    );

    let disabled_error = engine
        .apply_thread_handoff_activation(
            &push_handoff_request(thread_id, "subagent-disabled"),
            None,
        )
        .await
        .expect_err("disabled user subagents must not become handoff targets");
    assert!(
        disabled_error
            .to_string()
            .contains("unknown handoff target: subagent-disabled"),
        "unexpected error: {disabled_error}"
    );
}

#[tokio::test]
async fn delete_thread_messages_rehydrates_and_clears_invalid_continuation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_continuation";

    let assistant_id = "assistant-anchor".to_string();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Continuation".to_string(),
                created_at: 1,
                updated_at: 4,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-1".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("first", 1),
                    AgentMessage {
                        id: assistant_id.clone(),
                        role: MessageRole::Assistant,
                        content: "answer".to_string(),
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
                        provider: Some(
                            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_123".to_string()),
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
                    AgentMessage::user("continue", 3),
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
        .await
        .expect("delete should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 2);
    assert!(thread
        .messages
        .iter()
        .all(|message| message.response_id.is_none()));
    assert!(thread.upstream_thread_id.is_none());
    assert!(thread.upstream_transport.is_none());
    assert!(thread.upstream_provider.is_none());
    assert!(thread.upstream_model.is_none());
    drop(threads);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert!(persisted.iter().all(|message| {
        !message
            .metadata_json
            .as_deref()
            .unwrap_or_default()
            .contains("resp_123")
    }));
}

#[tokio::test]
async fn delete_thread_messages_removes_orphaned_tool_results_during_rebuild() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_orphans";

    let assistant_id = "assistant-tool-turn".to_string();
    let tool_a_id = "tool-a".to_string();
    let tool_b_id = "tool-b".to_string();
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Orphans".to_string(),
                created_at: 1,
                updated_at: 6,
                pinned: false,
                upstream_thread_id: Some("upstream-thread-2".to_string()),
                upstream_transport: Some(ApiTransport::Responses),
                upstream_provider: Some(
                    zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                ),
                upstream_model: Some("gpt-5.4".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                messages: vec![
                    AgentMessage::user("start", 1),
                    AgentMessage {
                        id: assistant_id.clone(),
                        role: MessageRole::Assistant,
                        content: "checking".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: Some(vec![
                            ToolCall {
                                id: "call-a".to_string(),
                                function: ToolFunction {
                                    name: "tool_a".to_string(),
                                    arguments: "{}".to_string(),
                                },
                                weles_review: None,
                            },
                            ToolCall {
                                id: "call-b".to_string(),
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
                        provider: Some(
                            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_456".to_string()),
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
                        id: tool_a_id.clone(),
                        role: MessageRole::Tool,
                        content: "partial".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: None,
                        tool_call_id: Some("call-a".to_string()),
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
                    AgentMessage {
                        id: tool_b_id.clone(),
                        role: MessageRole::Tool,
                        content: "done".to_string(),
                        content_blocks: Vec::new(),
                        tool_calls: None,
                        tool_call_id: Some("call-b".to_string()),
                        tool_name: Some("tool_b".to_string()),
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
                        timestamp: 4,
                        feedback: None,
                    },
                    AgentMessage {
                        id: "assistant-final".to_string(),
                        role: MessageRole::Assistant,
                        content: "final answer".to_string(),
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
                        provider: Some(
                            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                        ),
                        model: Some("gpt-5.4".to_string()),
                        api_transport: Some(ApiTransport::Responses),
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
                        timestamp: 5,
                        feedback: None,
                    },
                ],
            },
        );
    }
    engine.persist_thread_by_id(thread_id).await;

    engine
        .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
        .await
        .expect("delete should succeed");

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.messages[0].content, "start");
    assert_eq!(thread.messages[1].content, "final answer");
    assert!(thread
        .messages
        .iter()
        .all(|message| message.role != MessageRole::Tool));
    drop(threads);

    let persisted = engine
        .history
        .list_messages(thread_id, Some(10))
        .await
        .unwrap();
    assert_eq!(persisted.len(), 2);
    assert!(persisted.iter().all(|message| message.role != "tool"));
}

#[tokio::test]
async fn thread_metadata_round_trips_latest_skill_discovery_state() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_skill_discovery_metadata";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Discovery metadata".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "debug panic".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "client_surface": "tui",
                    "latest_skill_discovery_state": {
                        "query": "debug panic",
                        "confidence_tier": "strong",
                        "recommended_skill": "systematic-debugging",
                        "recommended_action": "read_skill systematic-debugging",
                        "mesh_next_step": "read_skill",
                        "mesh_requires_approval": false,
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
    engine.persist_thread_by_id(thread_id).await;

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(
        metadata.contains("\"latest_skill_discovery_state\""),
        "expected latest skill discovery state to survive hydrate + persist: {metadata}"
    );
    assert!(metadata.contains("\"query\":\"debug panic\""));
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));
    assert!(metadata.contains("\"mesh_next_step\":\"read_skill\""));
    assert!(metadata.contains("\"mesh_requires_approval\":false"));
    assert!(metadata.contains("\"read_skill_identifier\":\"variant-systematic-debugging-v1\""));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn thread_metadata_hydrate_compacts_large_latest_skill_discovery_query() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_large_skill_discovery_metadata";
    let large_query = format!(
        "HEARTBEAT SYNTHESIS\n{}",
        "large heartbeat context ".repeat(200)
    );

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Discovery metadata".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "heartbeat".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "latest_skill_discovery_state": {
                        "query": large_query,
                        "confidence_tier": "pending",
                        "recommended_action": "await_skill_discovery",
                        "discovery_pending": true,
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
            id: "seed-message-large-query".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "heartbeat".to_string(),
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
        .get_thread_skill_discovery_state(thread_id)
        .await
        .expect("skill discovery state should hydrate");
    assert!(
        state.query.chars().count() <= 160,
        "hydrated query should be compact, got {} chars",
        state.query.chars().count()
    );
    assert!(state.query.starts_with("HEARTBEAT SYNTHESIS"));

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("metadata should persist");
    let metadata: serde_json::Value = serde_json::from_str(&metadata).expect("metadata json");
    let persisted_query = metadata
        .pointer("/latest_skill_discovery_state/query")
        .and_then(|value| value.as_str())
        .expect("persisted query");
    assert!(
        persisted_query.chars().count() <= 160,
        "persisted query should be compact, got {} chars",
        persisted_query.chars().count()
    );
}

#[tokio::test]
async fn thread_metadata_round_trips_memory_injection_state() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_memory_injection_metadata";
    let expected_state = PromptMemoryInjectionState {
        base_markdown_hash: Some("sha256:memory-base-v1".to_string()),
        base_markdown_updated_at_ms: Some(123),
        structured_summary_hash: Some("sha256:summary-v1".to_string()),
        base_markdown_injected_at_ms: Some(456),
        ..Default::default()
    };

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Memory injection metadata".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "memory bootstrap".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "prompt_memory_injection_state": {
                        "base_markdown_hash": "sha256:memory-base-v1",
                        "base_markdown_updated_at_ms": 123,
                        "structured_summary_hash": "sha256:summary-v1",
                        "base_markdown_injected_at_ms": 456
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
            id: "seed-memory-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "bootstrap memory".to_string(),
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
    engine.persist_thread_by_id(thread_id).await;

    assert!(expected_state.is_base_layer_injected());
    assert!(!expected_state.is_base_layer_stale(Some("sha256:memory-base-v1"), Some(123)));
    assert!(expected_state.is_base_layer_stale(Some("sha256:memory-base-v2"), Some(123)));

    let rehydrated_state = engine
        .get_thread_memory_injection_state(thread_id)
        .await
        .expect("memory injection state should restore");
    assert_eq!(rehydrated_state, expected_state);

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"prompt_memory_injection_state\""));
    assert!(metadata.contains("\"base_markdown_hash\":\"sha256:memory-base-v1\""));
    assert!(metadata.contains("\"base_markdown_updated_at_ms\":123"));
    assert!(metadata.contains("\"structured_summary_hash\":\"sha256:summary-v1\""));
    assert!(metadata.contains("\"base_markdown_injected_at_ms\":456"));
}

#[tokio::test]
async fn get_or_create_thread_restores_memory_injection_state_from_db_metadata() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_memory_injection_restore";
    let expected_state = PromptMemoryInjectionState {
        base_markdown_hash: Some("sha256:memory-base-v1".to_string()),
        base_markdown_updated_at_ms: Some(123),
        structured_summary_hash: Some("sha256:summary-v1".to_string()),
        base_markdown_injected_at_ms: Some(456),
        ..Default::default()
    };

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Memory injection restore".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "memory restore".to_string(),
            metadata_json: Some(
                serde_json::json!({
                    "prompt_memory_injection_state": {
                        "base_markdown_hash": "sha256:memory-base-v1",
                        "base_markdown_updated_at_ms": 123,
                        "structured_summary_hash": "sha256:summary-v1",
                        "base_markdown_injected_at_ms": 456
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
            id: "seed-memory-restore-message-1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1,
            role: "user".to_string(),
            content: "restore memory".to_string(),
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

    let (_, created) = engine
        .get_or_create_thread(Some(thread_id), "ignored")
        .await;

    assert!(!created, "thread should restore from persisted history");
    assert_eq!(
        engine.get_thread_memory_injection_state(thread_id).await,
        Some(expected_state)
    );
}

#[tokio::test]
async fn hydrated_thread_message_preserves_cost() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let seed_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-cost";

    seed_engine
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some(MAIN_AGENT_NAME.to_string()),
            title: "Cost".to_string(),
            created_at: 1,
            updated_at: 2,
            message_count: 1,
            total_tokens: 0,
            last_preview: "priced".to_string(),
            metadata_json: None,
        })
        .await
        .expect("seed thread row");
    seed_engine
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "msg-cost".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 100,
            role: "assistant".to_string(),
            content: "priced".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(4),
            output_tokens: Some(2),
            total_tokens: Some(6),
            cost_usd: Some(0.0042),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed thread message");

    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    engine.hydrate().await.expect("hydrate");

    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should exist");
    assert_eq!(thread.messages[0].cost, Some(0.0042));
}
