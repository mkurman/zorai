use super::tests_part2_support::*;
use super::*;
#[tokio::test]
async fn agent_statistics_query_returns_daemon_snapshot_payload() {
    let mut conn = spawn_test_connection().await;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;
    let thread_id = "statistics-query-thread";

    conn.agent
        .history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Statistics query".to_string(),
            created_at: now_ms - 2_000,
            updated_at: now_ms,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await
        .expect("seed thread");
    conn.agent
        .history
        .add_message(&zorai_protocol::AgentDbMessage {
            id: "statistics-query-message".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 1_000,
            role: "assistant".to_string(),
            content: "priced".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(21),
            output_tokens: Some(13),
            total_tokens: Some(34),
            cost_usd: Some(0.12),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("seed message");

    conn.framed
        .send(ClientMessage::AgentStatisticsQuery {
            window: zorai_protocol::AgentStatisticsWindow::All,
            session_limit: None,
            session_offset: None,
            sessions_only: false,
        })
        .await
        .expect("request statistics");

    match conn.recv().await {
        DaemonMessage::AgentStatisticsResponse { statistics_json } => {
            let snapshot: zorai_protocol::AgentStatisticsSnapshot =
                serde_json::from_str(&statistics_json).expect("decode statistics snapshot");
            assert_eq!(snapshot.window, zorai_protocol::AgentStatisticsWindow::All);
            assert_eq!(snapshot.totals.total_tokens, 34);
            assert_eq!(snapshot.providers.len(), 1);
            assert_eq!(snapshot.providers[0].provider, "openai");
            assert_eq!(snapshot.top_models_by_cost[0].model, "gpt-5.4-mini");
        }
        other => panic!("expected statistics response, got {other:?}"),
    }

    conn.shutdown().await;
}
