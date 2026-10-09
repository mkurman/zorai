use super::*;
use crate::history::schema_helpers::table_has_column_sync;

#[tokio::test]
async fn init_schema_adds_event_trigger_registry_table() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS event_triggers;
                ",
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let status = store
        .conn
        .call(|conn| {
            let has_event_family = table_has_column_sync(conn, "event_triggers", "event_family")?;
            let has_event_kind = table_has_column_sync(conn, "event_triggers", "event_kind")?;
            let has_agent_id = table_has_column_sync(conn, "event_triggers", "agent_id")?;
            let has_target_state = table_has_column_sync(conn, "event_triggers", "target_state")?;
            let has_cooldown_secs = table_has_column_sync(conn, "event_triggers", "cooldown_secs")?;
            let has_risk_label = table_has_column_sync(conn, "event_triggers", "risk_label")?;
            let has_notification_kind =
                table_has_column_sync(conn, "event_triggers", "notification_kind")?;
            let has_prompt_template =
                table_has_column_sync(conn, "event_triggers", "prompt_template")?;
            let has_tool_name = table_has_column_sync(conn, "event_triggers", "tool_name")?;
            let has_tool_payload_json =
                table_has_column_sync(conn, "event_triggers", "tool_payload_json")?;
            let trigger_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_event_triggers_family_kind_enabled'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((
                has_event_family,
                has_event_kind,
                has_agent_id,
                has_target_state,
                has_cooldown_secs,
                has_risk_label,
                has_notification_kind,
                has_prompt_template,
                has_tool_name,
                has_tool_payload_json,
                trigger_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert!(status.2);
    assert!(status.3);
    assert!(status.4);
    assert!(status.5);
    assert!(status.6);
    assert!(status.7);
    assert!(status.8);
    assert!(status.9);
    assert_eq!(
        status.10.as_deref(),
        Some("idx_event_triggers_family_kind_enabled")
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn event_trigger_registry_round_trips_weles_health_trigger() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let row = EventTriggerRow {
        id: "trigger-weles-degraded".to_string(),
        event_family: "health".to_string(),
        event_kind: "weles_health".to_string(),
        agent_id: Some("weles".to_string()),
        target_state: Some("degraded".to_string()),
        thread_id: None,
        enabled: true,
        cooldown_secs: 900,
        risk_label: "medium".to_string(),
        notification_kind: "weles_health_degraded".to_string(),
        prompt_template: Some("Investigate {state}: {reason}".to_string()),
        tool_name: Some("run_workflow_pack".to_string()),
        tool_payload_json: Some(
            serde_json::json!({
                "pack_name": "watch-monitor",
                "watch_source": "event"
            })
            .to_string(),
        ),
        title_template: "WELES review degraded".to_string(),
        body_template: "WELES health changed to {state}: {reason}".to_string(),
        created_at: 100,
        updated_at: 120,
        last_fired_at: Some(150),
        max_retries: 3,
    };

    store.upsert_event_trigger(&row).await?;

    let rows = store
        .list_event_triggers(Some("health"), Some("weles_health"))
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "trigger-weles-degraded");
    assert_eq!(rows[0].agent_id.as_deref(), Some("weles"));
    assert_eq!(rows[0].target_state.as_deref(), Some("degraded"));
    assert!(rows[0].enabled);
    assert_eq!(rows[0].cooldown_secs, 900);
    assert_eq!(rows[0].risk_label, "medium");
    assert_eq!(rows[0].notification_kind, "weles_health_degraded");
    assert_eq!(
        rows[0].prompt_template.as_deref(),
        Some("Investigate {state}: {reason}")
    );
    assert_eq!(rows[0].tool_name.as_deref(), Some("run_workflow_pack"));
    assert_eq!(
        rows[0].tool_payload_json.as_deref(),
        Some(
            serde_json::json!({
                "pack_name": "watch-monitor",
                "watch_source": "event"
            })
            .to_string()
            .as_str()
        )
    );
    assert_eq!(rows[0].last_fired_at, Some(150));

    store
        .record_event_trigger_fired("trigger-weles-degraded", 222)
        .await?;
    let rows = store
        .list_event_triggers(Some("health"), Some("weles_health"))
        .await?;
    assert_eq!(rows[0].last_fired_at, Some(222));

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_event_triggers_for_fire_filters_enabled_state_and_thread_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    for mut row in [
        EventTriggerRow {
            id: "global-match".to_string(),
            event_family: "health".to_string(),
            event_kind: "subagent_health".to_string(),
            agent_id: Some("weles".to_string()),
            target_state: None,
            thread_id: None,
            enabled: true,
            cooldown_secs: 0,
            risk_label: "medium".to_string(),
            notification_kind: "global".to_string(),
            prompt_template: None,
            tool_name: None,
            tool_payload_json: None,
            title_template: "global".to_string(),
            body_template: "global".to_string(),
            created_at: 100,
            updated_at: 100,
            last_fired_at: None,
            max_retries: 3,
        },
        EventTriggerRow {
            id: "exact-match".to_string(),
            event_family: "health".to_string(),
            event_kind: "subagent_health".to_string(),
            agent_id: Some("weles".to_string()),
            target_state: Some("stuck".to_string()),
            thread_id: Some("thread-a".to_string()),
            enabled: true,
            cooldown_secs: 0,
            risk_label: "medium".to_string(),
            notification_kind: "exact".to_string(),
            prompt_template: None,
            tool_name: None,
            tool_payload_json: None,
            title_template: "exact".to_string(),
            body_template: "exact".to_string(),
            created_at: 100,
            updated_at: 120,
            last_fired_at: None,
            max_retries: 3,
        },
        EventTriggerRow {
            id: "state-mismatch".to_string(),
            event_family: "health".to_string(),
            event_kind: "subagent_health".to_string(),
            agent_id: Some("weles".to_string()),
            target_state: Some("healthy".to_string()),
            thread_id: None,
            enabled: true,
            cooldown_secs: 0,
            risk_label: "low".to_string(),
            notification_kind: "state-mismatch".to_string(),
            prompt_template: None,
            tool_name: None,
            tool_payload_json: None,
            title_template: "state-mismatch".to_string(),
            body_template: "state-mismatch".to_string(),
            created_at: 100,
            updated_at: 130,
            last_fired_at: None,
            max_retries: 3,
        },
        EventTriggerRow {
            id: "thread-mismatch".to_string(),
            event_family: "health".to_string(),
            event_kind: "subagent_health".to_string(),
            agent_id: Some("weles".to_string()),
            target_state: None,
            thread_id: Some("thread-b".to_string()),
            enabled: true,
            cooldown_secs: 0,
            risk_label: "low".to_string(),
            notification_kind: "thread-mismatch".to_string(),
            prompt_template: None,
            tool_name: None,
            tool_payload_json: None,
            title_template: "thread-mismatch".to_string(),
            body_template: "thread-mismatch".to_string(),
            created_at: 100,
            updated_at: 140,
            last_fired_at: None,
            max_retries: 3,
        },
    ] {
        store.upsert_event_trigger(&row).await?;
        row.enabled = false;
        row.id = format!("disabled-{}", row.id);
        row.updated_at += 1;
        store.upsert_event_trigger(&row).await?;
    }

    let rows = store
        .list_event_triggers_for_fire("health", "subagent_health", Some("stuck"), Some("thread-a"))
        .await?;

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["exact-match", "global-match"]
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn sample_trigger_fire(id: &str, fired_at_ms: u64) -> TriggerFireHistoryRow {
    TriggerFireHistoryRow {
        id: id.to_string(),
        trigger_id: format!("trigger-{id}"),
        event_family: "routine".to_string(),
        event_kind: "daily_brief".to_string(),
        status: "succeeded".to_string(),
        fired_at_ms,
        completed_at_ms: Some(fired_at_ms + 1),
        retry_count: 0,
        error_message: None,
        created_task_id: None,
        notice_id: None,
        payload_json: "{}".to_string(),
    }
}

#[tokio::test]
async fn list_trigger_fire_history_since_filters_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_trigger_fire_history(&sample_trigger_fire("old", 100))
        .await?;
    store
        .insert_trigger_fire_history(&sample_trigger_fire("newer", 300))
        .await?;
    store
        .insert_trigger_fire_history(&sample_trigger_fire("newest", 500))
        .await?;

    let rows = store.list_trigger_fire_history_since(250, 1).await?;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "newest");
    assert!(rows.iter().all(|row| row.fired_at_ms >= 250));

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn count_recent_trigger_fire_history_filters_status_and_limit_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    for (id, status, fired_at_ms) in [
        ("failed-old", "failed", 100),
        ("failed-middle", "failed", 200),
        ("failed-new", "failed", 300),
        ("succeeded-new", "succeeded", 400),
    ] {
        let mut row = sample_trigger_fire(id, fired_at_ms);
        row.trigger_id = "trigger-shared".to_string();
        row.status = status.to_string();
        store.insert_trigger_fire_history(&row).await?;
    }

    let count = store
        .count_recent_trigger_fire_history("trigger-shared", "failed", 2)
        .await?;

    assert_eq!(count, 2);

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn suppressed_trigger_fire_retention_drops_only_old_suppressed_fires() -> Result<()> {
    let (store, _root) = make_test_store().await?;
    let now_ms = super::audit::retention_cutoff_ms(0) as u64;
    let old_ms = now_ms - 30 * 86_400 * 1000;
    let old_suppressed = super::audit::RETENTION_DELETE_BATCH as usize + 5;

    for index in 0..old_suppressed {
        let mut fire = sample_trigger_fire(&format!("old-suppressed-{index}"), old_ms);
        fire.status = "suppressed".to_string();
        store.insert_trigger_fire_history(&fire).await?;
    }
    let mut recent_suppressed = sample_trigger_fire("recent-suppressed", now_ms);
    recent_suppressed.status = "suppressed".to_string();
    store.insert_trigger_fire_history(&recent_suppressed).await?;
    let mut old_failed = sample_trigger_fire("old-failed", old_ms);
    old_failed.status = "failed".to_string();
    store.insert_trigger_fire_history(&old_failed).await?;

    let deleted = store.prune_suppressed_trigger_fire_history(7).await?;

    assert_eq!(
        deleted, old_suppressed,
        "every expired suppressed fire must go, even when the backlog spans several delete batches"
    );
    let remaining = store.list_trigger_fire_history(None, None, 100).await?;
    let mut remaining_ids = remaining.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
    remaining_ids.sort();
    assert_eq!(
        remaining_ids,
        vec!["old-failed", "recent-suppressed"],
        "failed fires feed the trigger circuit breaker and recent suppressions are still diagnostic, so both must survive"
    );
    Ok(())
}
