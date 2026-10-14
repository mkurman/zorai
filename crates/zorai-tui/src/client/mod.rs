#![allow(dead_code)]

#[path = "mod_parts/close_request_queue_for_test.rs"]
mod close_request_queue_for_test;
pub(crate) use close_request_queue_for_test::*;

#[derive(Debug, Default)]
pub(crate) struct ThreadDetailChunkBuffer {
    pub(crate) thread_id: Option<String>,
    pub(crate) bytes: Vec<u8>,
}

mod daemon_message_kind_to_handle_connection_to_handle_daemon_message;
mod dispatch_match;
mod handle_activity_profile_gateway_daemon_messages;
mod handle_thread_workspace_and_provider_daemon_messages;
mod is_internal_agent_thread_to_request_git_diff;
mod request_agent_status_to_defer_operator_profile_question_to_get_operator;

#[path = "mod_parts/get_string_lossy.rs"]
mod get_string_lossy;
#[path = "mod_parts/data_query_preview.rs"]
mod data_query_preview;

#[cfg(test)]
pub(crate) use handle_activity_profile_gateway_daemon_messages::*;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use zorai_protocol::ClientMessage;

    fn drain_request(rx: &mut mpsc::UnboundedReceiver<ClientMessage>) -> ClientMessage {
        rx.try_recv().expect("expected queued client message")
    }

    #[path = "bootstrap_rearms_after_successful_connection_cycle_to_daemon_bootstrap.rs"]
    mod bootstrap_rearms_after_successful_connection_cycle_to_daemon_bootstrap;
    #[path = "daemon_collaboration_sessions_reply_emits_client_event_to_workspace.rs"]
    mod daemon_collaboration_sessions_reply_emits_client_event_to_workspace;
    #[path = "tui_thread_handoff_bridge.rs"]
    mod tui_thread_handoff_bridge;
    #[path = "whatsapp_link_methods_send_expected_protocol_messages_to_resolve_task.rs"]
    mod whatsapp_link_methods_send_expected_protocol_messages_to_resolve_task;

    #[tokio::test]
    async fn dispatch_client_event_does_not_panic_when_receiver_dropped() {
        let (tx, rx) = mpsc::channel::<ClientEvent>(1);
        drop(rx);
        super::dispatch_client_event(&tx, ClientEvent::Error("test".to_string()), "test_context")
            .await;
    }

    #[tokio::test]
    async fn dispatch_client_event_delivers_to_receiver() {
        let (tx, mut rx) = mpsc::channel::<ClientEvent>(1);
        super::dispatch_client_event(&tx, ClientEvent::Error("hello".to_string()), "test_context")
            .await;
        match rx.try_recv() {
            Ok(ClientEvent::Error(msg)) => assert_eq!(msg, "hello"),
            other => panic!("expected ClientEvent::Error('hello'), got {:?}", other),
        }
    }

    async fn dispatch_for_test(
        message: zorai_protocol::DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) -> bool {
        let mut thread_detail_chunks = None;
        DaemonClient::handle_daemon_message(message, event_tx, &mut thread_detail_chunks).await
    }

    #[tokio::test]
    async fn agent_tier_changed_routes_to_tier_changed_client_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cont = dispatch_for_test(
            zorai_protocol::DaemonMessage::AgentTierChanged {
                previous_tier: "newcomer".to_string(),
                new_tier: "trusted".to_string(),
                reason: "auto-promote".to_string(),
            },
            &event_tx,
        )
        .await;
        assert!(cont);
        match event_rx.recv().await.expect("expected ClientEvent emitted") {
            ClientEvent::TierChanged { new_tier } => assert_eq!(new_tier, "trusted"),
            other => panic!("expected TierChanged, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn semantic_index_repair_result_routes_to_repaired_client_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cont = dispatch_for_test(
            zorai_protocol::DaemonMessage::SemanticIndexRepairResult {
                result_json: serde_json::json!({
                    "removed_vector_index": true,
                    "cleared_completions": 42,
                    "cleared_deletions": 7,
                    "reset_failed_jobs": 3,
                })
                .to_string(),
            },
            &event_tx,
        )
        .await;
        assert!(cont);
        match event_rx.recv().await.expect("expected ClientEvent emitted") {
            ClientEvent::SemanticIndexRepaired { summary } => {
                assert!(
                    summary.contains("repair"),
                    "summary missing 'repair': {summary}"
                );
                assert!(
                    summary.contains("42"),
                    "summary missing cleared count: {summary}"
                );
            }
            other => panic!("expected SemanticIndexRepaired, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn agent_prompt_queue_routes_to_prompt_queue_client_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cont = dispatch_for_test(
            zorai_protocol::DaemonMessage::AgentPromptQueue {
                thread_id: Some("thread-1".to_string()),
                prompts: vec![zorai_protocol::QueuedPromptRecord {
                    id: "prompt-1".to_string(),
                    thread_id: "thread-1".to_string(),
                    content: "later".to_string(),
                    content_blocks_json: None,
                    created_at: 1,
                    position: 0,
                }],
            },
            &event_tx,
        )
        .await;
        assert!(cont);
        match event_rx.recv().await.expect("expected PromptQueue") {
            ClientEvent::PromptQueue { thread_id, prompts } => {
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(prompts.len(), 1);
                assert_eq!(prompts[0].content, "later");
            }
            other => panic!("expected PromptQueue, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn unhandled_daemon_message_does_not_panic_dispatcher() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let cont = dispatch_for_test(zorai_protocol::DaemonMessage::Pong, &event_tx).await;
        assert!(
            cont,
            "dispatcher must continue running after an unhandled variant"
        );
    }

    #[tokio::test]
    async fn semantic_index_repair_with_unparseable_payload_emits_error_event() {
        let (event_tx, mut event_rx) = mpsc::channel(8);
        dispatch_for_test(
            zorai_protocol::DaemonMessage::SemanticIndexRepairResult {
                result_json: "not-valid-json".to_string(),
            },
            &event_tx,
        )
        .await;
        match event_rx.recv().await.expect("expected ClientEvent emitted") {
            ClientEvent::Error(msg) => {
                assert!(msg.contains("semantic"), "msg missing 'semantic': {msg}");
            }
            other => panic!("expected Error, got {:?}", other),
        }
    }
}
