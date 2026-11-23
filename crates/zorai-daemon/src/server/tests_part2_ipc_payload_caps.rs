use super::tests_part2_support::*;
use super::*;
#[tokio::test]
async fn get_work_context_truncates_oversized_payload_for_ipc() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-work-context-huge";
    let huge_path = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    conn.agent.thread_work_contexts.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: vec![crate::agent::types::WorkContextEntry {
                path: huge_path,
                previous_path: None,
                kind: crate::agent::types::WorkContextEntryKind::Artifact,
                source: "test".to_string(),
                change_kind: None,
                repo_root: None,
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: 1,
            }],
        },
    );

    conn.framed
        .send(ClientMessage::AgentGetWorkContext {
            thread_id: thread_id.to_string(),
        })
        .await
        .expect("request work context");

    match conn.recv().await {
        DaemonMessage::AgentWorkContextDetail {
            thread_id: returned_thread_id,
            context_json,
        } => {
            assert_eq!(returned_thread_id, thread_id);
            let context: crate::agent::types::ThreadWorkContext =
                serde_json::from_str(&context_json).expect("decode work context");
            assert!(
                context.entries.is_empty(),
                "oversized work context entry should be dropped to fit IPC"
            );
        }
        other => panic!("expected work context detail, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_streams_oversized_thread_detail_without_truncation() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-huge";
    let huge_message = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);
    let expected_huge_message = huge_message.clone();

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some("main".to_string()),
            title: "Huge thread".to_string(),
            messages: vec![
                crate::agent::types::AgentMessage::user(huge_message, 1),
                crate::agent::types::AgentMessage::user("recent tail", 2),
            ],
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

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: None,
            message_offset: None,
            collapse_tool_calls: false,
        })
        .await
        .expect("request thread detail");

    let mut chunks = Vec::new();
    loop {
        match conn.recv().await {
            DaemonMessage::AgentThreadDetailChunk {
                thread_id: returned_thread_id,
                thread_json_chunk,
                done,
            } => {
                assert_eq!(returned_thread_id, thread_id);
                chunks.extend(thread_json_chunk);
                if done {
                    break;
                }
            }
            other => panic!("expected thread detail chunk, got {other:?}"),
        }
    }

    let thread_json = String::from_utf8(chunks).expect("thread detail chunks should be utf-8");
    let thread: Option<crate::agent::types::AgentThread> =
        serde_json::from_str(&thread_json).expect("decode streamed thread detail");
    let thread = thread.expect("thread detail should exist");
    let contents = thread
        .messages
        .iter()
        .map(|message| message.content.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        contents,
        vec![expected_huge_message, "recent tail".to_string()]
    );

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_respects_requested_message_page_and_reports_window_metadata() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-paged";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some("main".to_string()),
            title: "Paged thread".to_string(),
            messages: (0..120)
                .map(|index| {
                    crate::agent::types::AgentMessage::user(format!("msg {index}"), index as u64)
                })
                .collect(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 120,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: Some(50),
            message_offset: Some(50),
            collapse_tool_calls: false,
        })
        .await
        .expect("request paged thread detail");

    let thread_json = match conn.recv().await {
        DaemonMessage::AgentThreadDetail { thread_json } => thread_json,
        other => panic!("expected thread detail, got {other:?}"),
    };

    let value: serde_json::Value =
        serde_json::from_str(&thread_json).expect("decode paged thread detail");
    let messages = value["messages"]
        .as_array()
        .expect("messages should be an array");
    let first_content = messages
        .first()
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str);
    let last_content = messages
        .last()
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str);

    assert_eq!(value["total_message_count"].as_u64(), Some(120));
    assert_eq!(value["loaded_message_start"].as_u64(), Some(20));
    assert_eq!(value["loaded_message_end"].as_u64(), Some(70));
    assert_eq!(messages.len(), 50);
    assert_eq!(first_content, Some("msg 20"));
    assert_eq!(last_content, Some("msg 69"));

    conn.shutdown().await;
}

#[tokio::test]
async fn get_agent_thread_reads_bounded_message_tail_from_db() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "agent-db-thread-tail";

    conn.agent
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("main".to_string()),
            title: "Agent DB tail".to_string(),
            created_at: 1,
            updated_at: 200,
            message_count: 200,
            total_tokens: 0,
            last_preview: "db message 199".to_string(),
            metadata_json: None,
        })
        .await
        .expect("persist thread");
    for index in 0..200 {
        conn.agent
            .history
            .add_message(&zorai_protocol::AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: format!("message-{index}"),
                thread_id: thread_id.to_string(),
                created_at: index,
                role: "user".to_string(),
                content: format!("db message {index}"),
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
            .expect("persist message");
    }

    conn.framed
        .send(ClientMessage::GetAgentThread {
            thread_id: thread_id.to_string(),
            include_deleted: false,
        })
        .await
        .expect("request agent db thread detail");

    match conn.recv().await {
        DaemonMessage::AgentDbThreadDetail { messages_json, .. } => {
            let messages: Vec<zorai_protocol::AgentDbMessage> =
                serde_json::from_str(&messages_json).expect("decode messages");
            assert_eq!(
                messages.len(),
                super::super::AGENT_DB_THREAD_DETAIL_MESSAGE_WINDOW
            );
            assert_eq!(
                messages.first().map(|message| message.content.as_str()),
                Some("db message 136")
            );
            assert_eq!(
                messages.last().map(|message| message.content.as_str()),
                Some("db message 199")
            );
        }
        other => panic!("expected agent db thread detail, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn list_agent_messages_without_limit_reads_bounded_message_tail_from_db() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "agent-db-message-list-tail";

    conn.agent
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("main".to_string()),
            title: "Agent DB message list tail".to_string(),
            created_at: 1,
            updated_at: 200,
            message_count: 200,
            total_tokens: 0,
            last_preview: "listed db message 199".to_string(),
            metadata_json: None,
        })
        .await
        .expect("persist thread");
    for index in 0..200 {
        conn.agent
            .history
            .add_message(&zorai_protocol::AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: format!("listed-message-{index}"),
                thread_id: thread_id.to_string(),
                created_at: index,
                role: "user".to_string(),
                content: format!("listed db message {index}"),
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
            .expect("persist message");
    }

    conn.framed
        .send(ClientMessage::ListAgentMessages {
            thread_id: thread_id.to_string(),
            limit: None,
            include_deleted: false,
        })
        .await
        .expect("request agent db message list");

    match conn.recv().await {
        DaemonMessage::AgentDbThreadDetail { messages_json, .. } => {
            let messages: Vec<zorai_protocol::AgentDbMessage> =
                serde_json::from_str(&messages_json).expect("decode messages");
            assert_eq!(
                messages.len(),
                super::super::AGENT_DB_THREAD_DETAIL_MESSAGE_WINDOW
            );
            assert_eq!(
                messages.first().map(|message| message.content.as_str()),
                Some("listed db message 136")
            );
            assert_eq!(
                messages.last().map(|message| message.content.as_str()),
                Some("listed db message 199")
            );
        }
        other => panic!("expected agent db thread detail, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn list_agent_threads_reads_bounded_recent_threads_from_db() {
    let mut conn = spawn_test_connection().await;
    let expected_limit = 128;
    let total_threads = expected_limit + 5;

    for index in 0..total_threads {
        conn.agent
            .history
            .create_thread(&zorai_protocol::AgentDbThread {
                id: format!("agent-db-list-thread-{index:03}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("main".to_string()),
                title: format!("Agent DB list thread {index:03}"),
                created_at: index as i64,
                updated_at: index as i64,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await
            .expect("persist thread");
    }

    conn.framed
        .send(ClientMessage::ListAgentThreads)
        .await
        .expect("request agent db thread list");

    match conn.recv().await {
        DaemonMessage::AgentDbThreadList { threads_json } => {
            let threads: Vec<zorai_protocol::AgentDbThread> =
                serde_json::from_str(&threads_json).expect("decode threads");
            assert_eq!(threads.len(), expected_limit);
            assert_eq!(
                threads.first().map(|thread| thread.id.as_str()),
                Some("agent-db-list-thread-132")
            );
            assert_eq!(
                threads.last().map(|thread| thread.id.as_str()),
                Some("agent-db-list-thread-005")
            );
        }
        other => panic!("expected agent db thread list, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_list_threads_with_explicit_limit_reads_bounded_recent_threads_from_db() {
    let mut conn = spawn_test_connection().await;
    let expected_limit = 128;
    let total_threads = expected_limit + 5;

    for index in 0..total_threads {
        conn.agent
            .history
            .create_thread(&zorai_protocol::AgentDbThread {
                id: format!("agent-list-thread-{index:03}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("main".to_string()),
                title: format!("Agent list thread {index:03}"),
                created_at: index as i64,
                updated_at: index as i64,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await
            .expect("persist thread");
    }

    conn.framed
        .send(ClientMessage::AgentListThreads {
            limit: Some(expected_limit),
            offset: None,
            include_internal: false,
            agent_filter: None,
        })
        .await
        .expect("request agent thread list");

    match conn.recv().await {
        DaemonMessage::AgentThreadList { threads_json } => {
            let threads: Vec<crate::agent::types::AgentThread> =
                serde_json::from_str(&threads_json).expect("decode threads");
            assert_eq!(threads.len(), expected_limit);
            assert_eq!(
                threads.first().map(|thread| thread.id.as_str()),
                Some("agent-list-thread-132")
            );
            assert_eq!(
                threads.last().map(|thread| thread.id.as_str()),
                Some("agent-list-thread-005")
            );
        }
        other => panic!("expected agent thread list, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_filtered_thread_list_honors_client_limit() {
    let mut conn = spawn_test_connection().await;
    for index in 0..12 {
        conn.agent
            .history
            .create_thread(&zorai_protocol::AgentDbThread {
                id: format!("agent-filtered-thread-{index:03}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("svarog".to_string()),
                title: format!("Svarog thread {index:03}"),
                created_at: index,
                updated_at: index,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await
            .expect("persist filtered thread");
    }

    conn.framed
        .send(ClientMessage::AgentListThreads {
            limit: Some(5),
            offset: Some(0),
            include_internal: false,
            agent_filter: Some("svarog".to_string()),
        })
        .await
        .expect("request bounded filtered threads");

    match conn.recv().await {
        DaemonMessage::AgentThreadList { threads_json } => {
            let threads: Vec<crate::agent::types::AgentThread> =
                serde_json::from_str(&threads_json).expect("decode threads");
            assert_eq!(threads.len(), 5);
        }
        other => panic!("expected agent thread list, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn agent_list_threads_without_limit_reads_all_thread_summaries_from_db() {
    let mut conn = spawn_test_connection().await;
    let total_threads = 133;

    for index in 0..total_threads {
        conn.agent
            .history
            .create_thread(&zorai_protocol::AgentDbThread {
                id: format!("agent-list-all-thread-{index:03}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("main".to_string()),
                title: format!("Agent list all thread {index:03}"),
                created_at: index as i64,
                updated_at: index as i64,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await
            .expect("persist thread");
    }

    conn.framed
        .send(ClientMessage::AgentListThreads {
            limit: None,
            offset: None,
            include_internal: false,
            agent_filter: None,
        })
        .await
        .expect("request all agent thread summaries");

    match conn.recv().await {
        DaemonMessage::AgentThreadList { threads_json } => {
            let threads: Vec<crate::agent::types::AgentThread> =
                serde_json::from_str(&threads_json).expect("decode threads");
            assert_eq!(threads.len(), total_threads);
            assert_eq!(
                threads.first().map(|thread| thread.id.as_str()),
                Some("agent-list-all-thread-132")
            );
            assert_eq!(
                threads.last().map(|thread| thread.id.as_str()),
                Some("agent-list-all-thread-000")
            );
        }
        other => panic!("expected agent thread list, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn list_transcript_index_reads_bounded_recent_entries_from_db() {
    let mut conn = spawn_test_connection().await;
    let expected_limit = 128;
    let total_entries = expected_limit + 5;

    for index in 0..total_entries {
        conn.agent
            .history
            .upsert_transcript_index(&zorai_protocol::TranscriptIndexEntry {
                id: format!("transcript-index-{index:03}"),
                pane_id: Some("pane".to_string()),
                workspace_id: None,
                surface_id: None,
                filename: format!("transcript-{index:03}.txt"),
                reason: None,
                captured_at: index as i64,
                size_bytes: None,
                preview: None,
            })
            .await
            .expect("persist transcript index entry");
    }

    conn.framed
        .send(ClientMessage::ListTranscriptIndex { workspace_id: None })
        .await
        .expect("request transcript index");

    match conn.recv().await {
        DaemonMessage::TranscriptIndexEntries { entries_json } => {
            let entries: Vec<zorai_protocol::TranscriptIndexEntry> =
                serde_json::from_str(&entries_json).expect("decode transcript entries");
            assert_eq!(entries.len(), expected_limit);
            assert_eq!(
                entries.first().map(|entry| entry.id.as_str()),
                Some("transcript-index-132")
            );
            assert_eq!(
                entries.last().map(|entry| entry.id.as_str()),
                Some("transcript-index-005")
            );
        }
        other => panic!("expected transcript index entries, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn list_snapshot_index_reads_bounded_recent_entries_from_db() {
    let mut conn = spawn_test_connection().await;
    let expected_limit = 128;
    let total_entries = expected_limit + 5;

    for index in 0..total_entries {
        conn.agent
            .history
            .upsert_snapshot_index(&zorai_protocol::SnapshotIndexEntry {
                snapshot_id: format!("snapshot-index-{index:03}"),
                workspace_id: None,
                session_id: None,
                kind: "manual".to_string(),
                label: None,
                path: format!("/tmp/snapshot-{index:03}.json"),
                created_at: index as i64,
                details_json: None,
            })
            .await
            .expect("persist snapshot index entry");
    }

    conn.framed
        .send(ClientMessage::ListSnapshotIndex { workspace_id: None })
        .await
        .expect("request snapshot index");

    match conn.recv().await {
        DaemonMessage::SnapshotIndexEntries { entries_json } => {
            let entries: Vec<zorai_protocol::SnapshotIndexEntry> =
                serde_json::from_str(&entries_json).expect("decode snapshot entries");
            assert_eq!(entries.len(), expected_limit);
            assert_eq!(
                entries.first().map(|entry| entry.snapshot_id.as_str()),
                Some("snapshot-index-132")
            );
            assert_eq!(
                entries.last().map(|entry| entry.snapshot_id.as_str()),
                Some("snapshot-index-005")
            );
        }
        other => panic!("expected snapshot index entries, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn list_snapshots_reads_bounded_recent_entries_from_db() {
    let mut conn = spawn_test_connection().await;
    let expected_limit = 128;
    let total_entries = expected_limit + 5;

    for index in 0..total_entries {
        conn.agent
            .history
            .upsert_snapshot_index(&zorai_protocol::SnapshotIndexEntry {
                snapshot_id: format!("snapshot-list-{index:03}"),
                workspace_id: None,
                session_id: None,
                kind: "manual".to_string(),
                label: Some(format!("Snapshot {index:03}")),
                path: format!("/tmp/snapshot-list-{index:03}.json"),
                created_at: index as i64,
                details_json: None,
            })
            .await
            .expect("persist snapshot index entry");
    }

    conn.framed
        .send(ClientMessage::ListSnapshots { workspace_id: None })
        .await
        .expect("request snapshot list");

    match conn.recv().await {
        DaemonMessage::SnapshotList { snapshots } => {
            assert_eq!(snapshots.len(), expected_limit);
            assert_eq!(
                snapshots
                    .first()
                    .map(|snapshot| snapshot.snapshot_id.as_str()),
                Some("snapshot-list-132")
            );
            assert_eq!(
                snapshots
                    .last()
                    .map(|snapshot| snapshot.snapshot_id.as_str()),
                Some("snapshot-list-005")
            );
        }
        other => panic!("expected snapshot list, got {other:?}"),
    }

    conn.shutdown().await;
}

#[test]
fn cap_scrollback_for_ipc_keeps_recent_tail() {
    let tail = b"recent tail".to_vec();
    let mut data = vec![b'x'; zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024];
    data.extend_from_slice(&tail);

    let session_id = uuid::Uuid::nil();
    let (capped, truncated) = super::super::cap_scrollback_for_ipc(session_id, data);
    assert!(truncated);
    assert!(capped.ends_with(&tail));
    assert!(zorai_protocol::daemon_message_fits_ipc(
        &DaemonMessage::Scrollback {
            id: session_id,
            data: capped,
        }
    ));
}

#[test]
fn cap_history_search_result_for_ipc_drops_oversized_hits() {
    let huge_hit = zorai_protocol::HistorySearchHit {
        id: "hit-huge".to_string(),
        kind: "session".to_string(),
        title: "Huge".to_string(),
        excerpt: "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        path: None,
        timestamp: 1,
        score: 1.0,
    };
    let small_hit = zorai_protocol::HistorySearchHit {
        id: "hit-small".to_string(),
        kind: "session".to_string(),
        title: "Small".to_string(),
        excerpt: "recent excerpt".to_string(),
        path: None,
        timestamp: 2,
        score: 0.9,
    };

    let (summary, hits, truncated) = super::super::cap_history_search_result_for_ipc(
        "query",
        "summary".to_string(),
        vec![small_hit.clone(), huge_hit],
    );
    assert!(truncated);
    assert_eq!(summary, "summary");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, small_hit.id);
    assert!(zorai_protocol::daemon_message_fits_ipc(
        &DaemonMessage::HistorySearchResult {
            query: "query".to_string(),
            summary,
            hits,
        }
    ));
}

#[test]
fn cap_agent_thread_list_for_ipc_drops_oversized_entries() {
    let small_thread = crate::agent::types::AgentThread {
        id: "thread-small".to_string(),
        agent_name: None,
        title: "Small".to_string(),
        messages: Vec::new(),
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
    };
    let huge_thread = crate::agent::types::AgentThread {
        id: "thread-huge".to_string(),
        agent_name: None,
        title: "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        messages: Vec::new(),
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at: 1,
        updated_at: 1,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };

    let (threads, truncated) =
        super::super::cap_agent_thread_list_for_ipc(vec![small_thread.clone(), huge_thread]);
    assert!(truncated);
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].id, small_thread.id);
    let threads_json = serde_json::to_string(&threads).expect("serialize capped thread list");
    assert!(zorai_protocol::daemon_message_fits_ipc(
        &DaemonMessage::AgentThreadList { threads_json }
    ));
}

#[test]
fn cap_plugin_api_call_result_for_ipc_truncates_large_result() {
    let (result, truncated) = super::super::cap_plugin_api_call_result_for_ipc(
        Some("op-plugin-huge"),
        "plugin",
        "endpoint",
        true,
        "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        None,
    );

    assert!(truncated);
    assert!(zorai_protocol::daemon_message_fits_ipc(
        &DaemonMessage::PluginApiCallResult {
            operation_id: Some("op-plugin-huge".to_string()),
            plugin_name: "plugin".to_string(),
            endpoint_name: "endpoint".to_string(),
            success: true,
            result,
            error_type: None,
        }
    ));
}

#[tokio::test]
async fn get_todos_truncates_oversized_payload_for_ipc() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-todos-huge";
    let huge_content = "x".repeat(zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024);

    conn.agent.thread_todos.write().await.insert(
        thread_id.to_string(),
        vec![crate::agent::types::TodoItem {
            id: "todo-huge".to_string(),
            content: huge_content,
            status: crate::agent::types::TodoStatus::Pending,
            position: 0,
            step_index: None,
            created_at: 1,
            updated_at: 1,
        }],
    );

    conn.framed
        .send(ClientMessage::AgentGetTodos {
            thread_id: thread_id.to_string(),
        })
        .await
        .expect("request todos");

    match conn.recv().await {
        DaemonMessage::AgentTodoDetail {
            thread_id: returned_thread_id,
            todos_json,
        } => {
            assert_eq!(returned_thread_id, thread_id);
            let todos: Vec<crate::agent::types::TodoItem> =
                serde_json::from_str(&todos_json).expect("decode todo detail");
            assert!(
                todos.is_empty(),
                "oversized todo item should be dropped to fit IPC"
            );
        }
        other => panic!("expected todo detail, got {other:?}"),
    }

    conn.shutdown().await;
}
