use super::*;
use zorai_protocol::{AgentDbMessage, AgentDbThread, AgentStatisticsWindow};

async fn seed_statistics_messages(
    store: &HistoryStore,
    thread_id: &str,
    now_ms: i64,
) -> Result<()> {
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Statistics".to_string(),
            created_at: now_ms - 40 * 24 * 60 * 60 * 1000,
            updated_at: now_ms,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    let day_ms = 24 * 60 * 60 * 1000;
    let hour_ms = 60 * 60 * 1000;
    let messages = vec![
        AgentDbMessage {
            id: "msg-old".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 35 * day_ms,
            role: "assistant".to_string(),
            content: "older costless".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-old".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-openai-a".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 2 * day_ms,
            role: "assistant".to_string(),
            content: "openai recent".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(100),
            output_tokens: Some(50),
            total_tokens: Some(150),
            cost_usd: Some(0.30),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-openai-b".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 2 * day_ms + hour_ms,
            role: "assistant".to_string(),
            content: "openai followup".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(20),
            output_tokens: Some(10),
            total_tokens: Some(30),
            cost_usd: Some(0.05),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-claude-a".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - hour_ms,
            role: "assistant".to_string(),
            content: "anthropic today".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-4".to_string()),
            input_tokens: Some(80),
            output_tokens: Some(20),
            total_tokens: Some(100),
            cost_usd: Some(0.40),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-claude-b".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 20 * day_ms,
            role: "assistant".to_string(),
            content: "anthropic 30d".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-4".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cost_usd: Some(0.08),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-gemini".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 10 * day_ms,
            role: "assistant".to_string(),
            content: "google 30d".to_string(),
            provider: Some("google".to_string()),
            model: Some("gemini-2.5".to_string()),
            input_tokens: Some(50),
            output_tokens: Some(50),
            total_tokens: Some(100),
            cost_usd: Some(0.20),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "msg-o3".to_string(),
            thread_id: thread_id.to_string(),
            created_at: now_ms - 3 * day_ms,
            role: "assistant".to_string(),
            content: "o3 recent".to_string(),
            provider: Some("openai".to_string()),
            model: Some("o3-mini".to_string()),
            input_tokens: Some(40),
            output_tokens: Some(60),
            total_tokens: Some(100),
            cost_usd: Some(0.25),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];

    for message in messages {
        store.add_message(&message).await?;
    }

    Ok(())
}

#[tokio::test]
async fn agent_statistics_all_time_include_provider_model_rankings_and_incomplete_cost_flag(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;
    seed_statistics_messages(&store, "thread-statistics-all", now_ms).await?;

    let snapshot = store
        .get_agent_statistics(AgentStatisticsWindow::All, None, None)
        .await?;

    assert_eq!(snapshot.window, AgentStatisticsWindow::All);
    assert!(snapshot.has_incomplete_cost_history);
    assert_eq!(snapshot.totals.input_tokens, 310);
    assert_eq!(snapshot.totals.output_tokens, 200);
    assert_eq!(snapshot.totals.total_tokens, 510);
    assert!((snapshot.totals.cost_usd - 1.28).abs() < f64::EPSILON);
    assert_eq!(snapshot.totals.provider_count, 3);
    assert_eq!(snapshot.totals.model_count, 5);

    assert_eq!(snapshot.providers.len(), 3);
    assert_eq!(snapshot.providers[0].provider, "openai");
    assert_eq!(snapshot.providers[0].total_tokens, 295);
    assert!((snapshot.providers[0].cost_usd - 0.60).abs() < f64::EPSILON);
    assert_eq!(snapshot.providers[1].provider, "anthropic");
    assert_eq!(snapshot.providers[1].total_tokens, 115);
    assert_eq!(snapshot.providers[2].provider, "google");
    assert_eq!(snapshot.providers[2].total_tokens, 100);

    assert_eq!(snapshot.models.len(), 5);
    assert_eq!(snapshot.top_models_by_tokens.len(), 5);
    assert_eq!(snapshot.top_models_by_tokens[0].provider, "openai");
    assert_eq!(snapshot.top_models_by_tokens[0].model, "gpt-5.4-mini");
    assert_eq!(snapshot.top_models_by_tokens[0].total_tokens, 180);
    assert_eq!(snapshot.top_models_by_tokens[1].provider, "anthropic");
    assert_eq!(snapshot.top_models_by_tokens[1].model, "claude-4");
    assert_eq!(snapshot.top_models_by_tokens[2].provider, "openai");
    assert_eq!(snapshot.top_models_by_tokens[2].model, "o3-mini");
    assert_eq!(snapshot.top_models_by_tokens[3].provider, "google");
    assert_eq!(snapshot.top_models_by_tokens[3].model, "gemini-2.5");
    assert_eq!(snapshot.top_models_by_tokens[4].model, "gpt-old");

    assert_eq!(snapshot.top_models_by_cost[0].provider, "anthropic");
    assert_eq!(snapshot.top_models_by_cost[0].model, "claude-4");
    assert_eq!(snapshot.top_models_by_cost[1].provider, "openai");
    assert_eq!(snapshot.top_models_by_cost[1].model, "gpt-5.4-mini");
    assert_eq!(snapshot.top_models_by_cost[2].model, "o3-mini");

    assert_eq!(
        snapshot
            .daily
            .iter()
            .map(|row| row.request_count)
            .sum::<u64>(),
        7
    );
    assert_eq!(snapshot.session_total, 1);
    assert_eq!(snapshot.session_limit, 25);
    assert_eq!(snapshot.session_offset, 0);
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].thread_id, "thread-statistics-all");
    assert_eq!(snapshot.sessions[0].request_count, 7);
    assert_eq!(snapshot.sessions[0].total_tokens, 510);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_statistics_windows_apply_expected_cutoffs() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;
    seed_statistics_messages(&store, "thread-statistics-window", now_ms).await?;

    let today = store
        .get_agent_statistics(AgentStatisticsWindow::Today, None, None)
        .await?;
    assert_eq!(today.totals.total_tokens, 100);
    assert!((today.totals.cost_usd - 0.40).abs() < f64::EPSILON);
    assert!(!today.has_incomplete_cost_history);

    let seven_days = store
        .get_agent_statistics(AgentStatisticsWindow::Last7Days, None, None)
        .await?;
    assert_eq!(seven_days.totals.input_tokens, 240);
    assert_eq!(seven_days.totals.output_tokens, 140);
    assert_eq!(seven_days.totals.total_tokens, 380);
    assert!((seven_days.totals.cost_usd - 1.00).abs() < f64::EPSILON);
    assert_eq!(seven_days.totals.provider_count, 2);
    assert_eq!(seven_days.totals.model_count, 3);

    let thirty_days = store
        .get_agent_statistics(AgentStatisticsWindow::Last30Days, None, None)
        .await?;
    assert_eq!(thirty_days.totals.input_tokens, 300);
    assert_eq!(thirty_days.totals.output_tokens, 195);
    assert_eq!(thirty_days.totals.total_tokens, 495);
    assert!((thirty_days.totals.cost_usd - 1.28).abs() < f64::EPSILON);
    assert_eq!(thirty_days.totals.provider_count, 3);
    assert_eq!(thirty_days.totals.model_count, 4);
    assert!(!thirty_days.has_incomplete_cost_history);

    assert_eq!(
        today.daily.iter().map(|row| row.request_count).sum::<u64>(),
        1
    );
    assert_eq!(
        seven_days
            .daily
            .iter()
            .map(|row| row.request_count)
            .sum::<u64>(),
        4
    );
    assert_eq!(
        thirty_days
            .daily
            .iter()
            .map(|row| row.request_count)
            .sum::<u64>(),
        6
    );
    assert_eq!(
        seven_days.sessions[0].updated_at,
        now_ms.saturating_sub(60 * 60 * 1000) as u64
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_statistics_sessions_are_sql_paginated_and_skip_deleted_threads_and_invalid_dates(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("now should be after epoch")
        .as_millis() as i64;

    for (index, thread_id) in ["session-old", "session-new", "session-deleted"]
        .into_iter()
        .enumerate()
    {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("Svarog".to_string()),
                title: thread_id.to_string(),
                created_at: now_ms - 10_000,
                updated_at: now_ms + index as i64 * 10_000,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await?;
        store
            .add_message(&AgentDbMessage {
                id: format!("message-{thread_id}"),
                thread_id: thread_id.to_string(),
                created_at: now_ms - 3_000 + index as i64 * 1_000,
                role: "assistant".to_string(),
                content: thread_id.to_string(),
                provider: Some("openai".to_string()),
                model: Some("gpt-test".to_string()),
                input_tokens: Some(1),
                output_tokens: Some(1),
                total_tokens: Some(2),
                cost_usd: Some(0.01),
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }

    store.delete_thread("session-deleted").await?;
    store
        .create_thread(&AgentDbThread {
            id: "session-invalid-date".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "invalid".to_string(),
            created_at: 0,
            updated_at: 0,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "message-invalid-date".to_string(),
            thread_id: "session-invalid-date".to_string(),
            created_at: 0,
            role: "assistant".to_string(),
            content: "invalid".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-test".to_string()),
            input_tokens: Some(1),
            output_tokens: Some(1),
            total_tokens: Some(2),
            cost_usd: Some(0.01),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await?;

    let first = store
        .get_agent_statistics(AgentStatisticsWindow::All, Some(1), Some(0))
        .await?;
    assert_eq!(first.session_total, 2);
    assert_eq!(first.session_limit, 1);
    assert_eq!(first.session_offset, 0);
    assert_eq!(first.sessions.len(), 1);
    assert_eq!(first.sessions[0].thread_id, "session-new");
    assert!(first.daily.iter().all(|row| row.day_key != "1970-01-01"));

    let second = store
        .get_agent_statistics(AgentStatisticsWindow::All, Some(1), Some(1))
        .await?;
    assert_eq!(second.sessions.len(), 1);
    assert_eq!(second.sessions[0].thread_id, "session-old");
    assert!(second.sessions[0].updated_at < first.sessions[0].updated_at);

    let page_only = store
        .get_agent_session_page(AgentStatisticsWindow::All, Some(1), Some(1))
        .await?;
    assert_eq!(page_only.sessions[0].thread_id, "session-old");
    assert_eq!(page_only.session_offset, 1);
    assert_eq!(page_only.session_total, 2);
    assert_eq!(page_only.totals.total_tokens, 0);
    assert!(page_only.providers.is_empty());
    assert!(page_only.models.is_empty());
    assert!(page_only.daily.is_empty());

    fs::remove_dir_all(root)?;
    Ok(())
}
