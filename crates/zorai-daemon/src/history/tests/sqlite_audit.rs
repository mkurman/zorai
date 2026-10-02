use super::*;
use crate::history::schema_helpers::table_has_column_sync;

#[test]
fn table_has_column_detects_generated_columns() -> Result<()> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch(
        "CREATE TABLE agent_threads (
            id           TEXT PRIMARY KEY,
            metadata_json TEXT,
            pinned       INTEGER GENERATED ALWAYS AS (
                CASE WHEN metadata_json IS NOT NULL THEN 1 ELSE 0 END
            ) VIRTUAL
        );",
    )?;

    assert!(table_has_column_sync(&conn, "agent_threads", "pinned")?);
    Ok(())
}

/// FOUN-01: WAL journal mode is active after HistoryStore construction.
#[tokio::test]
async fn wal_mode_enabled() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mode: String = store
        .conn
        .call(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(mode.to_lowercase(), "wal");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-06: All 5 WAL pragmas are applied on connection open.
#[tokio::test]
async fn wal_pragmas_applied() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let pragmas = store
        .conn
        .call(|conn| {
            let journal_mode: String =
                conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
            let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
            let wal_autocheckpoint: i64 =
                conn.query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
            let busy_timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
            let cache_size: i64 = conn.query_row("PRAGMA cache_size", [], |row| row.get(0))?;
            let mmap_size: i64 = conn.query_row("PRAGMA mmap_size", [], |row| row.get(0))?;
            Ok((
                journal_mode,
                synchronous,
                foreign_keys,
                wal_autocheckpoint,
                busy_timeout,
                cache_size,
                mmap_size,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(pragmas.0.to_lowercase(), "wal");
    assert_eq!(pragmas.1, 1);
    assert_eq!(pragmas.2, 1);
    assert_eq!(pragmas.3, 1000);
    assert_eq!(pragmas.4, 5000);
    assert_eq!(
        pragmas.5, -4096,
        "page cache must stay at 4 MiB per connection"
    );
    assert_eq!(
        pragmas.6, 16_777_216,
        "mmap window must stay at 16 MiB per connection"
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn read_connection_uses_wal_and_query_only_mode() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let read_pragmas = store
        .read_conn
        .call(|conn: &mut rusqlite::Connection| {
            let journal_mode: String =
                conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            let query_only: i64 = conn.query_row("PRAGMA query_only", [], |row| row.get(0))?;
            let busy_timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
            Ok((journal_mode, query_only, busy_timeout))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(read_pragmas.0.to_lowercase(), "wal");
    assert_eq!(read_pragmas.1, 1);
    assert_eq!(read_pragmas.2, 5000);

    let write_query_only: i64 = store
        .conn
        .call(|conn: &mut rusqlite::Connection| {
            conn.query_row("PRAGMA query_only", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(write_query_only, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_messages_have_cursor_friendly_thread_created_id_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let indexed_columns = store
        .conn
        .call(|conn| {
            let mut stmt = conn.prepare("PRAGMA index_info('idx_messages_thread_created_id')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        indexed_columns,
        vec![
            "thread_id".to_string(),
            "created_at".to_string(),
            "id".to_string()
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_adds_visible_thread_list_index_after_deleted_at_migration() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_deleted_at, index_sql) = store
        .conn
        .call(|conn| {
            let has_deleted_at = table_has_column_sync(conn, "agent_threads", "deleted_at")?;
            let index_sql = conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_threads_visible_updated'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((has_deleted_at, index_sql))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_deleted_at);
    assert!(index_sql.contains("updated_at DESC"));
    assert!(index_sql.contains("id"));
    assert!(index_sql.contains("WHERE deleted_at IS NULL"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_handles_existing_generated_pinned_thread_column() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT,
                pinned         INTEGER GENERATED ALWAYS AS (
                    CASE WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) AND (
                        json_extract(metadata_json, '$.pinned') = 1
                        OR json_extract(metadata_json, '$.pinnedThread') = 1
                    ) THEN 1 ELSE 0 END
                ) VIRTUAL
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_pinned, index_sql) = store
        .conn
        .call(|conn| {
            let has_pinned = table_has_column_sync(conn, "agent_threads", "pinned")?;
            let index_sql = conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'idx_threads_pinned_active_updated'",
                [],
                |row| row.get::<_, String>(0),
            )?;
            Ok((has_pinned, index_sql))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_pinned);
    assert!(index_sql.contains("ON agent_threads(pinned, updated_at DESC)"));
    assert!(index_sql.contains("WHERE deleted_at IS NULL"));

    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-02: HistoryStore can perform a basic async roundtrip through .call().
#[tokio::test]
async fn async_connection_roundtrip() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread = AgentDbThread {
        id: "test-thread-1".to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Test Thread".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    let loaded = store.get_thread("test-thread-1").await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.title, "Test Thread");
    assert_eq!(loaded.agent_name, Some("test-agent".to_string()));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn has_thread_id_checks_existence_without_hydrating_thread_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread = AgentDbThread {
        id: "thread-exists-fast".to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Existence check".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_threads SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["thread-exists-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(store.has_thread_id("thread-exists-fast").await?);
    assert!(!store.has_thread_id("thread-missing-fast").await?);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_created_at_selects_timestamp_without_hydrating_thread_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread = AgentDbThread {
        id: "thread-created-at-fast".to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Created at check".to_string(),
        created_at: 4242,
        updated_at: 5000,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_threads SET title = x'ff' WHERE id = ?1",
                params!["thread-created-at-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store.thread_created_at("thread-created-at-fast").await?,
        Some(4242)
    );
    assert_eq!(store.thread_created_at("thread-missing-fast").await?, None);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_metadata_json_selects_metadata_without_hydrating_thread_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread = AgentDbThread {
        id: "thread-metadata-fast".to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Metadata check".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: Some("{\"mode\":\"visible\"}".to_string()),
    };
    store.create_thread(&thread).await?;
    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_threads SET title = x'ff' WHERE id = ?1",
                params!["thread-metadata-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store.thread_metadata_json("thread-metadata-fast").await?,
        Some("{\"mode\":\"visible\"}".to_string())
    );
    assert_eq!(
        store.thread_metadata_json("thread-missing-fast").await?,
        None
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_delegate_payload_context_selects_title_and_recent_messages_only() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-delegate-context-fast";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Delegate Context".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for index in 0..10 {
        store
            .add_message(&AgentDbMessage {
                id: format!("delegate-message-{index}"),
                thread_id: thread_id.to_string(),
                created_at: 1100 + index,
                role: if index % 2 == 0 { "assistant" } else { "user" }.to_string(),
                content: format!("message {index}"),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: Some(index),
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_threads SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["thread-delegate-context-fast"],
            )?;
            conn.execute(
                "UPDATE agent_messages SET metadata_json = x'ff' WHERE thread_id = ?1",
                params!["thread-delegate-context-fast"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let context = store
        .thread_delegate_payload_context(thread_id, 8)
        .await?
        .expect("thread context should be selected");

    assert_eq!(context.title, "Delegate Context");
    assert_eq!(context.messages.len(), 8);
    assert_eq!(context.messages[0].content, "message 2");
    assert_eq!(context.messages[7].content, "message 9");
    assert_eq!(context.messages[0].role, "assistant");
    assert_eq!(context.messages[7].role, "user");
    assert!(store
        .thread_delegate_payload_context("thread-missing-fast", 8)
        .await?
        .is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn has_non_heartbeat_user_message_after_uses_sql_exists() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for (id, title) in [
        ("normal-before", "Normal before"),
        ("normal-after", "Normal after"),
        ("heartbeat-title", "Heartbeat check: ignored"),
        ("heartbeat-message", "Normal title but heartbeat content"),
    ] {
        store
            .create_thread(&AgentDbThread {
                id: id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: title.to_string(),
                created_at: 100,
                updated_at: 200,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, thread_id, created_at, content) in [
        (
            "normal-before-message",
            "normal-before",
            900,
            "before cutoff",
        ),
        ("normal-after-message", "normal-after", 1100, "after cutoff"),
        (
            "heartbeat-title-message",
            "heartbeat-title",
            1200,
            "after cutoff from title heartbeat",
        ),
        (
            "heartbeat-content-marker",
            "heartbeat-message",
            800,
            "HEARTBEAT SYNTHESIS should mark the thread hidden",
        ),
        (
            "heartbeat-content-after",
            "heartbeat-message",
            1300,
            "after cutoff but heartbeat thread",
        ),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "user".to_string(),
                content: content.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute("UPDATE agent_threads SET created_at = 'not-an-integer'", [])?;
            conn.execute("UPDATE agent_messages SET metadata_json = x'ff'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(
        store.has_non_heartbeat_user_message_after(1000).await?,
        "normal user message after cutoff should be selected"
    );

    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_messages SET deleted_at = 1400 WHERE id = ?1",
                params!["normal-after-message"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(
        !store.has_non_heartbeat_user_message_after(1000).await?,
        "heartbeat threads should not count after the normal message is deleted"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_thread_id_by_message_timestamp_uses_persisted_messages() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for (id, updated_at) in [
        ("thread-old-message", 9_000),
        ("thread-latest-message", 100),
        ("thread-empty-newer", 10_000),
    ] {
        store
            .create_thread(&AgentDbThread {
                id: id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Thread {id}"),
                created_at: 100,
                updated_at,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, thread_id, created_at) in [
        ("old-message", "thread-old-message", 1000),
        ("latest-message", "thread-latest-message", 2000),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "user".to_string(),
                content: id.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute("UPDATE agent_threads SET created_at = 'not-an-integer'", [])?;
            conn.execute("UPDATE agent_threads SET title = x'ff'", [])?;
            conn.execute("UPDATE agent_messages SET metadata_json = x'ff'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .latest_thread_id_by_message_timestamp()
            .await?
            .as_deref(),
        Some("thread-latest-message"),
        "latest persisted message timestamp should choose the active thread"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_thread_context_hint_selects_newest_non_empty_preview_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for (id, updated_at, preview, deleted_at) in [
        ("thread-old-preview", 100, "old preview", None),
        ("thread-empty-preview", 300, "   ", None),
        ("thread-deleted-preview", 400, "deleted preview", Some(500)),
        ("thread-new-preview", 200, "new preview", None),
    ] {
        store
            .create_thread(&AgentDbThread {
                id: id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Thread {id}"),
                created_at: 1,
                updated_at,
                message_count: if preview.trim().is_empty() { 0 } else { 1 },
                total_tokens: 0,
                last_preview: preview.to_string(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
        if let Some(deleted_at) = deleted_at {
            let id = id.to_string();
            store
                .conn
                .call(move |conn| {
                    conn.execute(
                        "UPDATE agent_threads SET deleted_at = ?2 WHERE id = ?1",
                        params![id, deleted_at],
                    )?;
                    Ok(())
                })
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }
    }

    store
        .conn
        .call(|conn| {
            conn.execute(
                "INSERT INTO agent_messages (id, thread_id, created_at, role, content)
                 VALUES ('poison-message', 'thread-old-preview', 'not-a-time', x'ff', x'ff')",
                [],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store.latest_thread_context_hint().await?,
        Some((
            "thread-new-preview".to_string(),
            "new preview".to_string(),
            200
        )),
        "latest context restoration hint should be selected from thread summary columns"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_has_message_substring_checks_live_message_rows_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in ["thread-hidden-marker", "thread-visible-marker"] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Marker {thread_id}"),
                created_at: 1,
                updated_at: 3,
                message_count: 1,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await?;
    }

    store
        .add_message(&AgentDbMessage {
            id: "marker-hidden".to_string(),
            thread_id: "thread-hidden-marker".to_string(),
            created_at: 2,
            role: "system".to_string(),
            content: "Route as persona_id_marker weles-governance".to_string(),
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
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "marker-deleted".to_string(),
            thread_id: "thread-visible-marker".to_string(),
            created_at: 2,
            role: "system".to_string(),
            content: "Route as persona_id_marker weles-governance".to_string(),
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
        .await?;
    assert_eq!(
        store
            .delete_messages("thread-visible-marker", &["marker-deleted"])
            .await?,
        1
    );

    let markers = vec!["PERSONA_ID_MARKER WELES-GOVERNANCE".to_string()];
    assert!(
        store
            .thread_has_message_substring("thread-hidden-marker", &markers)
            .await?,
        "helper should find case-insensitive markers in non-deleted messages"
    );
    assert!(
        !store
            .thread_has_message_substring("thread-visible-marker", &markers)
            .await?,
        "helper should ignore deleted marker messages"
    );
    assert!(
        !store
            .thread_has_message_substring("thread-hidden-marker", &[])
            .await?,
        "empty marker lists should not match"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_non_empty_message_content_for_thread_ids_uses_sql_projection() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in [
        "thread-excerpt-fast",
        "thread-empty-fast",
        "thread-unrequested-excerpt-fast",
    ] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Excerpt {thread_id}"),
                created_at: 100,
                updated_at: 100,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, thread_id, created_at, content) in [
        (
            "excerpt-old",
            "thread-excerpt-fast",
            1000,
            "old visible text",
        ),
        ("excerpt-blank", "thread-excerpt-fast", 2000, "   \n\t  "),
        (
            "excerpt-latest",
            "thread-excerpt-fast",
            3000,
            "latest visible text",
        ),
        (
            "unrequested-latest",
            "thread-unrequested-excerpt-fast",
            4000,
            "should not be returned",
        ),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "assistant".to_string(),
                content: content.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute("UPDATE agent_threads SET created_at = 'not-an-integer'", [])?;
            conn.execute("UPDATE agent_threads SET title = x'ff'", [])?;
            conn.execute("UPDATE agent_messages SET metadata_json = x'ff'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let requested = vec![
        "thread-empty-fast".to_string(),
        "thread-excerpt-fast".to_string(),
    ];
    let excerpts = store
        .latest_non_empty_message_content_for_thread_ids(&requested)
        .await?;

    assert_eq!(excerpts.len(), 1);
    assert_eq!(
        excerpts.get("thread-excerpt-fast").map(String::as_str),
        Some("latest visible text")
    );
    assert!(
        !excerpts.contains_key("thread-empty-fast"),
        "thread with no non-empty messages should not get a fabricated excerpt"
    );
    assert!(
        store
            .latest_non_empty_message_content_for_thread_ids(&Vec::<String>::new())
            .await?
            .is_empty(),
        "empty input should avoid scanning all messages"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_recovery_prompt_content_falls_back_to_system_seed() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store
        .create_thread(&AgentDbThread {
            id: "thread-goal-system-seed".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: "Goal thread".to_string(),
            created_at: 100,
            updated_at: 100,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "goal-system-seed".to_string(),
            thread_id: "thread-goal-system-seed".to_string(),
            created_at: 1000,
            role: "system".to_string(),
            content: "Continue the current goal step: Select candidate".to_string(),
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
        .await?;

    assert_eq!(
        store
            .latest_user_message_content("thread-goal-system-seed")
            .await?,
        None
    );
    assert_eq!(
        store
            .latest_recovery_prompt_content("thread-goal-system-seed")
            .await?,
        Some("Continue the current goal step: Select candidate".to_string())
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn gateway_approval_ids_for_thread_extracts_ids_in_sql_order() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in ["thread-gateway-approval", "thread-other-gateway-approval"] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Thread {thread_id}"),
                created_at: 100,
                updated_at: 100,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, thread_id, created_at, content) in [
        (
            "approval-old-message",
            "thread-gateway-approval",
            1000,
            "Approval required.\nApproval ID: approval-old\nRisk: low",
        ),
        (
            "approval-latest-message",
            "thread-gateway-approval",
            2000,
            "Approval required.\nApproval ID:\tapproval-latest\nRisk: high",
        ),
        (
            "approval-space-message",
            "thread-gateway-approval",
            1500,
            "Approval ID: approval-middle extra words",
        ),
        (
            "approval-other-thread-message",
            "thread-other-gateway-approval",
            3000,
            "Approval ID: approval-other",
        ),
        (
            "approval-no-id-message",
            "thread-gateway-approval",
            2500,
            "Approval ID: \n\t   ",
        ),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "assistant".to_string(),
                content: content.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_messages SET deleted_at = 1400 WHERE id = ?1",
                params!["approval-space-message"],
            )?;
            conn.execute("UPDATE agent_threads SET title = x'ff'", [])?;
            conn.execute("UPDATE agent_messages SET metadata_json = x'ff'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store
            .gateway_approval_ids_for_thread("thread-gateway-approval")
            .await?,
        vec!["approval-latest".to_string(), "approval-old".to_string()],
        "approval ids should be extracted and ordered from persisted messages"
    );
    assert!(
        store
            .gateway_approval_ids_for_thread("thread-missing-gateway-approval")
            .await?
            .is_empty(),
        "missing threads should not fabricate approval ids"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn gateway_turn_auto_send_projection_uses_latest_turn_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in [
        "thread-gateway-auto-send",
        "thread-gateway-send-used",
        "thread-gateway-missing-response",
    ] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Thread {thread_id}"),
                created_at: 100,
                updated_at: 100,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, thread_id, created_at, role, content, tool_calls_json, metadata_json) in [
        (
            "auto-old-user",
            "thread-gateway-auto-send",
            1000,
            "user",
            "old request",
            None,
            None,
        ),
        (
            "auto-old-assistant",
            "thread-gateway-auto-send",
            1010,
            "assistant",
            "old response",
            None,
            None,
        ),
        (
            "auto-old-tool",
            "thread-gateway-auto-send",
            1020,
            "tool",
            "sent old response",
            None,
            Some(r#"{"tool_name":"send_discord_message"}"#),
        ),
        (
            "auto-latest-user",
            "thread-gateway-auto-send",
            2000,
            "user",
            "latest request",
            None,
            None,
        ),
        (
            "auto-ack",
            "thread-gateway-auto-send",
            2010,
            "assistant",
            "On it.",
            None,
            None,
        ),
        (
            "auto-earlier-tool",
            "thread-gateway-auto-send",
            2020,
            "tool",
            "sent ack",
            None,
            Some(r#"{"tool_name":"send_discord_message"}"#),
        ),
        (
            "auto-final",
            "thread-gateway-auto-send",
            2030,
            "assistant",
            "Final answer for gateway",
            None,
            None,
        ),
        (
            "used-user",
            "thread-gateway-send-used",
            3000,
            "user",
            "post an update",
            None,
            None,
        ),
        (
            "used-assistant",
            "thread-gateway-send-used",
            3010,
            "assistant",
            "",
            Some(
                r#"[{"id":"call-send","function":{"name":"send_discord_message","arguments":"{}"}}]"#,
            ),
            None,
        ),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: role.to_string(),
                content: content.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: tool_calls_json.map(ToOwned::to_owned),
                metadata_json: metadata_json.map(ToOwned::to_owned),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute("UPDATE agent_threads SET title = x'ff'", [])?;
            conn.execute(
                "UPDATE agent_messages SET metadata_json = x'ff' WHERE id = ?1",
                params!["auto-final"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let projection = store
        .gateway_turn_auto_send_projection("thread-gateway-auto-send")
        .await?
        .expect("projection should exist for latest gateway turn");
    assert!(
        !projection.used_send_tool,
        "send tools before the latest assistant response should not suppress auto-send"
    );
    assert_eq!(
        projection.latest_assistant_response.as_deref(),
        Some("Final answer for gateway")
    );

    let used_projection = store
        .gateway_turn_auto_send_projection("thread-gateway-send-used")
        .await?
        .expect("send-tool projection should exist");
    assert!(
        used_projection.used_send_tool,
        "assistant send tool calls should suppress auto-send"
    );

    assert!(
        store
            .gateway_turn_auto_send_projection("thread-gateway-missing-response")
            .await?
            .is_none(),
        "threads without messages should not fabricate auto-send state"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_message_count_counts_visible_messages_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in ["thread-message-count", "thread-message-count-empty"] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Thread {thread_id}"),
                created_at: 100,
                updated_at: 100,
                message_count: 99,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (id, created_at) in [
        ("count-user-1", 1000),
        ("count-assistant-1", 1010),
        ("count-deleted-1", 1020),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: "thread-message-count".to_string(),
                created_at,
                role: "user".to_string(),
                content: id.to_string(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_messages SET deleted_at = 1500 WHERE id = ?1",
                params!["count-deleted-1"],
            )?;
            conn.execute(
                "UPDATE agent_threads SET title = x'ff', message_count = 99",
                [],
            )?;
            conn.execute("UPDATE agent_messages SET metadata_json = x'ff'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        store.thread_message_count("thread-message-count").await?,
        Some(2),
        "message count should be computed by SQL over non-deleted messages"
    );
    assert_eq!(
        store
            .thread_message_count("thread-message-count-empty")
            .await?,
        Some(0),
        "existing empty threads should return zero"
    );
    assert_eq!(
        store
            .thread_message_count("thread-message-count-missing")
            .await?,
        None,
        "missing threads should not fabricate a count"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_ids_with_unanswered_tool_calls_filters_requested_threads_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for thread_id in [
        "thread-unanswered-fast",
        "thread-answered-fast",
        "thread-unrequested-fast",
    ] {
        store
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: None,
                title: format!("Tool call check {thread_id}"),
                created_at: 100,
                updated_at: 100,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    for (thread_id, call_id, created_at) in [
        ("thread-unanswered-fast", "call-missing", 1000),
        ("thread-answered-fast", "call-done", 2000),
        ("thread-unrequested-fast", "call-unrequested", 3000),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: format!("{thread_id}-assistant"),
                thread_id: thread_id.to_string(),
                created_at,
                role: "assistant".to_string(),
                content: String::new(),
                provider: Some("provider-that-should-not-be-read".to_string()),
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: Some(format!(
                    r#"[{{"id":"{call_id}","function":{{"name":"read_file","arguments":"{{}}"}}}}]"#
                )),
                metadata_json: Some("{\"ignored\":true}".to_string()),
            })
            .await?;
    }

    store
        .add_message(&AgentDbMessage {
            id: "thread-answered-fast-tool".to_string(),
            thread_id: "thread-answered-fast".to_string(),
            created_at: 2010,
            role: "tool".to_string(),
            content: "done".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(r#"{"tool_call_id":"call-done"}"#.to_string()),
        })
        .await?;

    store
        .conn
        .call(|conn| {
            conn.execute("UPDATE agent_threads SET created_at = 'not-an-integer'", [])?;
            conn.execute("UPDATE agent_threads SET title = x'ff'", [])?;
            conn.execute(
                "UPDATE agent_messages SET metadata_json = x'ff' WHERE role = 'assistant'",
                [],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let requested = vec![
        "thread-answered-fast".to_string(),
        "thread-unanswered-fast".to_string(),
    ];
    let blocked = store
        .thread_ids_with_unanswered_tool_calls(&requested)
        .await?;

    assert_eq!(blocked, vec!["thread-unanswered-fast".to_string()]);
    assert!(
        store
            .thread_ids_with_unanswered_tool_calls(&Vec::<String>::new())
            .await?
            .is_empty(),
        "empty input should avoid scanning all persisted messages"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_has_unanswered_tool_calls_uses_sql_window() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-unanswered-tool-fast";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: "Unanswered tool call projection".to_string(),
            created_at: 100,
            updated_at: 100,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: Some("{\"ignored\":true}".to_string()),
        })
        .await?;

    store
        .add_message(&AgentDbMessage {
            id: "tool-user".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1000,
            role: "user".to_string(),
            content: "inspect files".to_string(),
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
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "tool-assistant".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1010,
            role: "assistant".to_string(),
            content: String::new(),
            provider: Some("provider-that-should-not-be-read".to_string()),
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: Some(
                r#"[{"id":"call-a","function":{"name":"read_file","arguments":"{}"}},{"id":"call-b","function":{"name":"search_files","arguments":"{}"}}]"#
                    .to_string(),
            ),
            metadata_json: Some("{\"ignored\":true}".to_string()),
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "tool-result-a".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1020,
            role: "tool".to_string(),
            content: "file contents".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(r#"{"tool_call_id":"call-a","toolCallId":"call-a"}"#.to_string()),
        })
        .await?;

    store
        .conn
        .call(|conn| {
            conn.execute(
                "UPDATE agent_threads SET created_at = 'not-an-integer' WHERE id = ?1",
                params!["thread-unanswered-tool-fast"],
            )?;
            conn.execute(
                "UPDATE agent_messages SET metadata_json = x'ff' WHERE id = ?1",
                params!["tool-assistant"],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(
        store.thread_has_unanswered_tool_calls(thread_id).await?,
        "missing call-b result should be detected"
    );

    store
        .add_message(&AgentDbMessage {
            id: "tool-result-b".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1030,
            role: "tool".to_string(),
            content: "search results".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(r#"{"toolCallId":"call-b"}"#.to_string()),
        })
        .await?;

    assert!(
        !store.thread_has_unanswered_tool_calls(thread_id).await?,
        "both contiguous tool results should satisfy the assistant call"
    );

    store
        .add_message(&AgentDbMessage {
            id: "late-assistant".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1040,
            role: "assistant".to_string(),
            content: String::new(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: Some(
                r#"[{"id":"call-late","function":{"name":"read_file","arguments":"{}"}}]"#
                    .to_string(),
            ),
            metadata_json: None,
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "late-user-boundary".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1050,
            role: "user".to_string(),
            content: "boundary".to_string(),
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
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "late-tool-result".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1060,
            role: "tool".to_string(),
            content: "too late".to_string(),
            provider: None,
            model: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(r#"{"tool_call_id":"call-late"}"#.to_string()),
        })
        .await?;

    assert!(
        store.thread_has_unanswered_tool_calls(thread_id).await?,
        "a non-tool boundary before the tool result should leave the call unanswered"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn filtered_thread_list_applies_persisted_context_filters_before_limit() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for (id, title, updated_at, message_count) in [
        ("concierge", "Concierge", 50, 1),
        ("dm:svarog:weles", "Internal DM", 40, 1),
        ("playground:visible:weles", "Participant Playground", 30, 1),
        ("heartbeat", "HEARTBEAT SYNTHESIS nightly", 20, 1),
        ("empty", "No messages yet", 10, 0),
        ("visible", "Visible work", 5, 2),
    ] {
        store
            .create_thread(&AgentDbThread {
                id: id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("Svarog".to_string()),
                title: title.to_string(),
                created_at: updated_at,
                updated_at,
                message_count,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await?;
    }

    let rows = store
        .list_threads_filtered(&AgentThreadListQuery {
            excluded_ids: vec!["concierge".to_string()],
            hidden_id_prefixes: vec!["dm:".to_string(), "playground:".to_string()],
            title_excluded_prefixes: vec![
                "HEARTBEAT SYNTHESIS".to_string(),
                "Heartbeat check:".to_string(),
            ],
            min_message_count: Some(1),
            limit: Some(1),
            ..AgentThreadListQuery::default()
        })
        .await?;

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["visible"]
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn limited_transcript_index_reads_only_requested_recent_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for index in 0..5 {
        store
            .upsert_transcript_index(&TranscriptIndexEntry {
                id: format!("transcript-{index}"),
                pane_id: None,
                workspace_id: None,
                surface_id: None,
                filename: format!("transcript-{index}.jsonl"),
                reason: None,
                captured_at: index,
                size_bytes: Some(0),
                preview: None,
            })
            .await?;
    }

    let rows = store.list_transcript_index_limited(None, Some(2)).await?;

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["transcript-4", "transcript-3"]
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn matching_transcript_index_filters_query_before_limit_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store
        .upsert_transcript_index(&TranscriptIndexEntry {
            id: "newest-unrelated".to_string(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            filename: "other.jsonl".to_string(),
            reason: None,
            captured_at: 300,
            size_bytes: Some(0),
            preview: Some("no match".to_string()),
        })
        .await?;
    store
        .upsert_transcript_index(&TranscriptIndexEntry {
            id: "matched-preview".to_string(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            filename: "session.jsonl".to_string(),
            reason: None,
            captured_at: 200,
            size_bytes: Some(0),
            preview: Some("contains rust target".to_string()),
        })
        .await?;
    store
        .upsert_transcript_index(&TranscriptIndexEntry {
            id: "matched-filename".to_string(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            filename: "rust-build.jsonl".to_string(),
            reason: None,
            captured_at: 100,
            size_bytes: Some(0),
            preview: None,
        })
        .await?;

    let rows = store.list_transcript_index_matching("rust", 1).await?;

    assert_eq!(
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        vec!["matched-preview"]
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_migrates_legacy_agent_messages_before_deleted_at_index() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT
            );
            CREATE TABLE agent_messages (
                id              TEXT PRIMARY KEY,
                thread_id       TEXT NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
                created_at      INTEGER NOT NULL,
                role            TEXT NOT NULL,
                content         TEXT NOT NULL DEFAULT '',
                provider        TEXT,
                model           TEXT,
                input_tokens    INTEGER,
                output_tokens   INTEGER,
                total_tokens    INTEGER,
                cost_usd        REAL,
                reasoning       TEXT,
                tool_calls_json TEXT,
                metadata_json   TEXT
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_deleted_at, index_columns) = store
        .conn
        .call(|conn| {
            let has_deleted_at = table_has_column_sync(conn, "agent_messages", "deleted_at")?;
            let mut stmt =
                conn.prepare("PRAGMA index_info('idx_messages_thread_deleted_created')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            let index_columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((has_deleted_at, index_columns))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_deleted_at);
    assert_eq!(
        index_columns,
        vec![
            "thread_id".to_string(),
            "deleted_at".to_string(),
            "created_at".to_string(),
            "id".to_string()
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_message_cost_round_trips_through_history() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "cost-thread";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Cost".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "m-cost".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1010,
            role: "assistant".to_string(),
            content: "priced".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(11),
            output_tokens: Some(7),
            total_tokens: Some(18),
            cost_usd: Some(0.0123),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await?;

    let loaded = store.list_messages(thread_id, Some(10)).await?;
    assert_eq!(loaded[0].cost_usd, Some(0.0123));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replace_thread_snapshot_replaces_messages_without_losing_thread_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "snapshot-thread";
    let thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Original Thread".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "old body".to_string(),
        metadata_json: None,
    };
    let old_message = AgentDbMessage {
        id: "old-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 1001,
        role: "assistant".to_string(),
        content: "old body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store.add_message(&old_message).await?;

    let refreshed_thread = AgentDbThread {
        title: "Updated Thread".to_string(),
        updated_at: 2000,
        message_count: 1,
        total_tokens: 2,
        last_preview: "new body".to_string(),
        ..thread.clone()
    };
    let new_message = AgentDbMessage {
        id: "new-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 2001,
        role: "assistant".to_string(),
        content: "new body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(2),
        total_tokens: Some(2),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };

    store
        .replace_thread_snapshot(&refreshed_thread, &[new_message.clone()])
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should still exist after snapshot replacement");
    assert_eq!(loaded_thread.title, "Updated Thread");

    let loaded_messages = store.list_messages(thread_id, Some(10)).await?;
    assert_eq!(loaded_messages.len(), 1);
    assert_eq!(loaded_messages[0].id, new_message.id);
    assert_eq!(loaded_messages[0].content, new_message.content);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replace_thread_snapshot_removes_pruned_messages_from_sqlite_search() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "snapshot-search-prune-thread";
    let thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Snapshot Search Prune".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "obsolete marker body".to_string(),
        metadata_json: None,
    };
    let old_message = AgentDbMessage {
        id: "old-search-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 1001,
        role: "assistant".to_string(),
        content: "obsolete marker body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store.add_message(&old_message).await?;

    let refreshed_thread = AgentDbThread {
        updated_at: 2000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "fresh marker body".to_string(),
        ..thread
    };
    let new_message = AgentDbMessage {
        id: "new-search-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 2001,
        role: "assistant".to_string(),
        content: "fresh marker body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };

    store
        .replace_thread_snapshot(&refreshed_thread, &[new_message])
        .await?;

    let mut hits = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        hits = store.search("obsolete marker", 5).await?.1;
        if hits.iter().all(|hit| hit.id != old_message.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        hits.iter().all(|hit| hit.id != old_message.id),
        "pruned message remained searchable: {hits:?}"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replace_thread_snapshot_does_not_regress_to_stale_snapshot() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-stale-snapshot-guard";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Stale Snapshot Guard".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 0,
        last_preview: "older".to_string(),
        metadata_json: None,
    };
    let older_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "older 1".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "older 2".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];
    store
        .replace_thread_snapshot(&base_thread, &older_messages)
        .await?;

    let newer_thread = AgentDbThread {
        updated_at: 300,
        message_count: 4,
        last_preview: "newer 4".to_string(),
        ..base_thread.clone()
    };
    let mut newer_messages = older_messages.clone();
    newer_messages.push(AgentDbMessage {
        id: "m3".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 210,
        role: "user".to_string(),
        content: "newer 3".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    });
    newer_messages.push(AgentDbMessage {
        id: "m4".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 220,
        role: "assistant".to_string(),
        content: "newer 4".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    });
    store
        .replace_thread_snapshot(&newer_thread, &newer_messages)
        .await?;

    let stale_thread = AgentDbThread {
        updated_at: 250,
        message_count: 2,
        last_preview: "stale older".to_string(),
        ..base_thread
    };
    store
        .replace_thread_snapshot(&stale_thread, &older_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain persisted");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.updated_at, 300);
    assert_eq!(loaded_thread.message_count, 4);
    assert_eq!(loaded_thread.last_preview, "newer 4");
    assert_eq!(loaded_messages.len(), 4);
    assert_eq!(
        loaded_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["older 1", "older 2", "newer 3", "newer 4"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reconcile_thread_snapshot_updates_changed_messages_without_pruning_missing_ones(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-reconcile-snapshot";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Reconcile Thread".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 5,
        last_preview: "old assistant".to_string(),
        metadata_json: Some("{\"phase\":\"old\"}".to_string()),
    };
    let initial_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "hello".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(1),
            output_tokens: Some(0),
            total_tokens: Some(1),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some("{\"v\":1}".to_string()),
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "old assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(2),
            output_tokens: Some(3),
            total_tokens: Some(5),
            cost_usd: None,
            reasoning: Some("old reasoning".to_string()),
            tool_calls_json: None,
            metadata_json: Some("{\"v\":1}".to_string()),
        },
    ];
    store
        .reconcile_thread_snapshot(&base_thread, &initial_messages)
        .await?;

    let updated_thread = AgentDbThread {
        updated_at: 300,
        message_count: 2,
        total_tokens: 9,
        last_preview: "new assistant".to_string(),
        metadata_json: Some("{\"phase\":\"new\"}".to_string()),
        ..base_thread
    };
    let updated_messages = vec![
        initial_messages[0].clone(),
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "new assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(4),
            output_tokens: Some(5),
            total_tokens: Some(9),
            cost_usd: None,
            reasoning: Some("new reasoning".to_string()),
            tool_calls_json: None,
            metadata_json: Some("{\"v\":2}".to_string()),
        },
    ];
    store
        .reconcile_thread_snapshot(&updated_thread, &updated_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should exist after reconcile");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.message_count, 2);
    assert_eq!(loaded_thread.last_preview, "new assistant");
    assert_eq!(
        loaded_thread.metadata_json.as_deref(),
        Some("{\"phase\":\"new\"}")
    );
    assert_eq!(loaded_messages.len(), 2);
    assert_eq!(loaded_messages[1].content, "new assistant");
    assert_eq!(
        loaded_messages[1].reasoning.as_deref(),
        Some("new reasoning")
    );
    assert_eq!(loaded_messages[1].total_tokens, Some(9));
    assert_eq!(
        loaded_messages[1].metadata_json.as_deref(),
        Some("{\"v\":2}")
    );

    let pruned_thread = AgentDbThread {
        updated_at: 400,
        message_count: 1,
        total_tokens: 1,
        last_preview: "hello".to_string(),
        metadata_json: Some("{\"phase\":\"pruned\"}".to_string()),
        ..updated_thread
    };
    store
        .reconcile_thread_snapshot(&pruned_thread, &[initial_messages[0].clone()])
        .await?;

    let pruned_messages = store.list_messages(thread_id, None).await?;
    assert_eq!(
        pruned_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2"],
        "reconcile snapshots may be partial and must not tombstone omitted messages"
    );
    assert_eq!(store.restore_messages(thread_id, &["m2"]).await?, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reconcile_thread_snapshot_keeps_existing_prefix_for_new_suffix_snapshot() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-reconcile-new-suffix";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Suffix Snapshot".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 0,
        last_preview: "old assistant".to_string(),
        metadata_json: None,
    };
    let initial_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "old prompt".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "old assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];
    store
        .reconcile_thread_snapshot(&base_thread, &initial_messages)
        .await?;

    let suffix_thread = AgentDbThread {
        updated_at: 300,
        message_count: 1,
        last_preview: "new prompt".to_string(),
        ..base_thread
    };
    let suffix_messages = vec![AgentDbMessage {
        id: "m3".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 310,
        role: "user".to_string(),
        content: "new prompt".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    }];
    store
        .reconcile_thread_snapshot(&suffix_thread, &suffix_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should exist after suffix reconcile");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.message_count, 3);
    assert_eq!(loaded_thread.last_preview, "new prompt");
    assert_eq!(
        loaded_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn delete_messages_soft_deletes_and_restore_makes_visible_again() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-soft-delete-messages";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Soft Delete Messages".to_string(),
            created_at: 100,
            updated_at: 100,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, content) in [
        ("m1", 110, "keep visible"),
        ("m2", 120, "trash marker searchable"),
        ("m3", 130, "latest deleted preview"),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "user".to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: Some(1),
                output_tokens: Some(0),
                total_tokens: Some(1),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }

    assert_eq!(store.delete_messages(thread_id, &["m2", "m3"]).await?, 2);

    let visible = store.list_messages(thread_id, None).await?;
    assert_eq!(
        visible
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let with_trash = store.list_messages_with_deleted(thread_id, None).await?;
    assert_eq!(
        with_trash
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );
    let (window, total_count, loaded_start, loaded_end) =
        store.list_message_window(thread_id, 10, 0).await?;
    assert_eq!(total_count, 1);
    assert_eq!((loaded_start, loaded_end), (0, 1));
    assert_eq!(
        window
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let recent = store.list_recent_messages(thread_id, 10).await?;
    assert_eq!(
        recent
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let after_cursor = store
        .list_messages_after_cursor(thread_id, None, None)
        .await?;
    assert_eq!(
        after_cursor
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain after soft delete");
    assert_eq!(thread.message_count, 1);
    assert_eq!(thread.last_preview, "keep visible");

    let mut hits = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        hits = store.search("trash marker searchable", 5).await?.1;
        if hits.iter().all(|hit| hit.id != "m2") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        hits.iter().all(|hit| hit.id != "m2"),
        "soft-deleted message remained searchable: {hits:?}"
    );

    assert_eq!(store.restore_messages(thread_id, &["m2", "m3"]).await?, 2);
    let restored = store.list_messages(thread_id, None).await?;
    assert_eq!(
        restored
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );
    let thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain after restore");
    assert_eq!(thread.message_count, 3);
    assert_eq!(thread.last_preview, "latest deleted preview");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn gui_pagination_counts_contiguous_tool_rows_as_one_slot() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-collapsed-tool-window";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Collapsed tools".to_string(),
            created_at: 1,
            updated_at: 1,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    let rows = [
        ("user-old", "user", "old question"),
        ("assistant-old", "assistant", "old answer"),
        ("user-new", "user", "new question"),
        ("tool-1", "tool", "requested"),
        ("tool-2", "tool", "result"),
        ("tool-3", "tool", "requested"),
        ("tool-4", "tool", "result"),
        ("assistant-new", "assistant", "new answer"),
    ];
    for (index, (id, role, content)) in rows.into_iter().enumerate() {
        let metadata_json = if role == "tool" {
            Some(
                serde_json::json!({
                    "toolCallId": format!("call-{}", (index - 3) / 2),
                    "toolName": "test_tool",
                    "toolStatus": if content == "result" { "done" } else { "requested" },
                })
                .to_string(),
            )
        } else {
            None
        };
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at: index as i64 + 1,
                role: role.to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json,
            })
            .await?;
    }

    let (messages, total, start, end) = store
        .list_message_window_collapsing_tool_runs(thread_id, 3, 0)
        .await?;

    assert_eq!(total, 8);
    assert_eq!(start, 2);
    assert_eq!(end, 8);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "user-new",
            "tool-1",
            "tool-2",
            "tool-3",
            "tool-4",
            "assistant-new"
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_messages_with_limit_returns_latest_messages_in_chronological_order() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "limited-latest-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Latest limited slice".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for index in 0..8 {
        store
            .add_message(&AgentDbMessage {
                id: format!("msg-{index}"),
                thread_id: thread_id.to_string(),
                created_at: 1_000 + index,
                role: "user".to_string(),
                content: format!("message-{index}"),
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
            .await?;
    }

    let loaded_messages = store.list_messages(thread_id, Some(3)).await?;
    let contents = loaded_messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["message-5", "message-6", "message-7"]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_assistant_message_selects_newest_non_empty_assistant_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "latest-assistant-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Latest assistant".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, role, content) in [
        ("user-latest", 1_300, "user", "newer user request"),
        ("assistant-old", 1_100, "assistant", "old answer"),
        ("assistant-empty", 1_400, "assistant", "   "),
        ("tool-latest", 1_500, "tool", "newer tool output"),
        ("assistant-new", 1_200, "assistant", "new answer"),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: role.to_string(),
                content: content.to_string(),
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
            .await?;
    }

    let latest = store
        .latest_assistant_message(thread_id)
        .await?
        .expect("assistant message should be selected");
    assert_eq!(latest.id, "assistant-new");
    assert_eq!(latest.content, "new answer");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_participant_assistant_message_selects_author_metadata_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "latest-participant-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Latest participant".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, content, metadata_json) in [
        ("assistant-main", 1_100, "main answer", None),
        (
            "assistant-empty",
            1_300,
            " ",
            Some(serde_json::json!({
                "author_agent_id": "agent-empty",
                "author_agent_name": "Empty Agent",
            })),
        ),
        (
            "assistant-participant",
            1_200,
            "participant answer",
            Some(serde_json::json!({
                "author_agent_id": "agent-participant",
                "author_agent_name": "Participant Agent",
            })),
        ),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "assistant".to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: metadata_json
                    .map(|value| serde_json::to_string(&value))
                    .transpose()?,
            })
            .await?;
    }

    let latest = store
        .latest_participant_assistant_message(thread_id)
        .await?
        .expect("participant assistant message should be selected");
    assert_eq!(latest.0, "agent-participant");
    assert_eq!(latest.1.as_deref(), Some("Participant Agent"));
    assert_eq!(latest.2, "participant answer");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn latest_visible_main_assistant_message_timestamp_selects_latest_visible_row_in_sql(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "latest-visible-main-assistant-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Latest visible main assistant".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, role, content, metadata_json) in [
        ("main-old", 1_100, "assistant", "main answer", None),
        (
            "participant-newer",
            1_200,
            "assistant",
            "participant answer",
            Some(serde_json::json!({"author_agent_id": "weles"})),
        ),
        (
            "hidden-delegate",
            1_300,
            "assistant",
            "hidden delegate chatter",
            Some(serde_json::json!({"tool_name": "internal_delegate"})),
        ),
        ("tool-newest", 1_400, "tool", "tool output", None),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: role.to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: metadata_json
                    .map(|value| serde_json::to_string(&value))
                    .transpose()?,
            })
            .await?;
    }

    let participant_newer = store
        .latest_visible_main_assistant_message_timestamp(thread_id, &["weles".to_string()])
        .await?;
    assert_eq!(participant_newer, None);

    store
        .add_message(&AgentDbMessage {
            id: "main-newest".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1_500,
            role: "assistant".to_string(),
            content: "fresh main answer".to_string(),
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
        .await?;

    let main_newer = store
        .latest_visible_main_assistant_message_timestamp(thread_id, &["weles".to_string()])
        .await?;
    assert_eq!(main_newer, Some(1_500));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_user_pacing_counts_recent_user_rows_and_averages_last_gaps_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "user-pacing-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "User pacing".to_string(),
            created_at: 1_000,
            updated_at: 5_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, role) in [
        ("user-old", 1_000, "user"),
        ("assistant-ignored", 1_500, "assistant"),
        ("user-middle", 2_000, "user"),
        ("user-recent-a", 4_000, "user"),
        ("user-recent-b", 5_000, "user"),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: role.to_string(),
                content: id.to_string(),
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
            .await?;
    }

    let pacing = store.thread_user_pacing(thread_id, 5_000, 1_000, 5).await?;

    assert_eq!(pacing.recent_message_count, 2);
    assert_eq!(pacing.avg_gap_secs, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn visible_continuation_obsolete_predicate_filters_progress_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "visible-continuation-obsolete-thread";
    let queued_at = 2_000;

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Continuation obsolete".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    store
        .add_message(&AgentDbMessage {
            id: "tool-after-queue".to_string(),
            thread_id: thread_id.to_string(),
            created_at: queued_at + 10,
            role: "tool".to_string(),
            content: "tool made progress".to_string(),
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
        .await?;

    store
        .add_message(&AgentDbMessage {
            id: "other-assistant-after-queue".to_string(),
            thread_id: thread_id.to_string(),
            created_at: queued_at + 20,
            role: "assistant".to_string(),
            content: "other agent response".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(
                serde_json::json!({
                    "author_agent_id": "other-agent",
                })
                .to_string(),
            ),
        })
        .await?;

    assert!(
        !store
            .visible_continuation_obsoleted_by_progress(thread_id, queued_at as u64, "target-agent")
            .await?
    );

    store
        .add_message(&AgentDbMessage {
            id: "target-assistant-after-queue".to_string(),
            thread_id: thread_id.to_string(),
            created_at: queued_at + 30,
            role: "assistant".to_string(),
            content: "target agent response".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some(
                serde_json::json!({
                    "author_agent_id": "target-agent",
                })
                .to_string(),
            ),
        })
        .await?;

    assert!(
        store
            .visible_continuation_obsoleted_by_progress(thread_id, queued_at as u64, "target-agent")
            .await?
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_structural_memory_bulk_loader_filters_thread_ids_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    for (thread_id, language_hint) in [
        ("thread-1", "rust"),
        ("thread-2", "typescript"),
        ("thread-3", "python"),
    ] {
        store
            .upsert_thread_structural_memory(
                thread_id,
                &serde_json::json!({
                    "language_hints": [language_hint],
                }),
                1_000,
            )
            .await?;
    }
    store.delete_thread_structural_memory("thread-2").await?;

    let rows = store
        .list_thread_structural_memory_for_threads(&[
            "thread-1".to_string(),
            "thread-2".to_string(),
        ])
        .await?;

    assert_eq!(
        rows.iter()
            .map(|row| row.thread_id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-1"]
    );
    assert_eq!(rows[0].state_json["language_hints"][0], "rust");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_active_context_window_starts_at_latest_compaction_artifact() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "active-context-window";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: "Active context".to_string(),
            created_at: 1,
            updated_at: 1,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for index in 0..6 {
        let metadata_json = (index == 3).then(|| {
            serde_json::json!({
                "message_kind": "compaction_artifact",
                "compaction_payload": "active summary"
            })
            .to_string()
        });
        store
            .add_message(&AgentDbMessage {
                id: format!("m{index}"),
                thread_id: thread_id.to_string(),
                created_at: index,
                role: "assistant".to_string(),
                content: format!("message-{index}"),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json,
            })
            .await?;
    }

    let (messages, loaded_start, loaded_end) = store.list_active_context_window(thread_id).await?;

    assert_eq!((loaded_start, loaded_end), (3, 6));
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m3", "m4", "m5"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_message_token_totals_sum_visible_message_token_columns() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "token-total-thread";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: "Token totals".to_string(),
            created_at: 1,
            updated_at: 1,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, input_tokens, output_tokens) in [
        ("m1", Some(7), Some(3)),
        ("m2", Some(11), None),
        ("m3", None, Some(13)),
        ("m4", Some(100), Some(200)),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at: 1,
                role: "assistant".to_string(),
                content: id.to_string(),
                provider: None,
                model: None,
                input_tokens,
                output_tokens,
                total_tokens: input_tokens
                    .zip(output_tokens)
                    .map(|(input, output)| input + output),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }
    assert_eq!(store.delete_messages(thread_id, &["m4"]).await?, 1);

    assert_eq!(
        store.thread_message_token_totals(thread_id).await?,
        (18, 16)
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn thread_message_cost_total_sums_all_visible_costed_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-cost-total";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: None,
            title: "Cost total".to_string(),
            created_at: 1,
            updated_at: 1,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, cost_usd) in [("m1", Some(0.12)), ("m2", None), ("m3", Some(0.03))] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at: 1,
                role: "assistant".to_string(),
                content: id.to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }

    let total = store
        .thread_message_cost_total(thread_id)
        .await?
        .expect("costed rows should produce a total");
    assert!((total - 0.15).abs() < 1e-12);
    assert_eq!(
        store.thread_message_cost_total("missing-thread").await?,
        None
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-01 + FOUN-02: Concurrent reads and writes do not produce "database is locked" errors.
#[tokio::test]
async fn concurrent_read_write() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut handles = Vec::new();
    for i in 0..8 {
        let store_clone = store.clone();
        handles.push(tokio::spawn(async move {
            let thread = AgentDbThread {
                id: format!("concurrent-thread-{i}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("test-agent".to_string()),
                title: format!("Concurrent Thread {i}"),
                created_at: 1000 + i as i64,
                updated_at: 1000 + i as i64,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            };
            store_clone.create_thread(&thread).await?;
            let loaded = store_clone.list_threads().await?;
            assert!(!loaded.is_empty());
            Ok::<(), anyhow::Error>(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    let all_threads = store.list_threads().await?;
    assert_eq!(all_threads.len(), 8);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn ensure_column_adds_user_action_to_action_audit() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let has = store
        .conn
        .call(|conn| Ok(table_has_column_sync(conn, "action_audit", "user_action")?))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(has, "user_action column should exist after init_schema");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_migrates_legacy_causal_traces_before_family_index() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE causal_traces (
                id                    TEXT PRIMARY KEY,
                thread_id             TEXT,
                goal_run_id           TEXT,
                task_id               TEXT,
                decision_type         TEXT NOT NULL,
                selected_json         TEXT NOT NULL,
                rejected_options_json TEXT,
                context_hash          TEXT,
                causal_factors_json   TEXT,
                outcome_json          TEXT NOT NULL,
                model_used            TEXT,
                created_at            INTEGER NOT NULL
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_trace_family, index_columns) = store
        .conn
        .call(|conn| {
            let has_trace_family = table_has_column_sync(conn, "causal_traces", "trace_family")?;
            let mut stmt = conn.prepare("PRAGMA index_info('idx_causal_traces_family')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            let index_columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((has_trace_family, index_columns))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_trace_family);
    assert_eq!(
        index_columns,
        vec!["trace_family".to_string(), "created_at".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn dismiss_audit_entry_sets_user_action() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let entry = AuditEntryRow {
        id: "test-dismiss-1".to_string(),
        timestamp: 1000,
        action_type: "heartbeat".to_string(),
        summary: "Test entry".to_string(),
        explanation: None,
        confidence: None,
        confidence_band: None,
        causal_trace_id: None,
        thread_id: None,
        goal_run_id: None,
        task_id: None,
        raw_data_json: None,
    };
    store.insert_action_audit(&entry).await?;
    store.dismiss_audit_entry("test-dismiss-1").await?;

    let user_action: Option<String> = store
        .conn
        .call(|conn| {
            conn.query_row(
                "SELECT user_action FROM action_audit WHERE id = ?1",
                ["test-dismiss-1"],
                |row| row.get(0),
            )
            .map_err(|e| e.into())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(user_action.as_deref(), Some("dismissed"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn count_dismissals_by_type_returns_correct_counts() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for i in 0..3 {
        let entry = AuditEntryRow {
            id: format!("hb-{}", i),
            timestamp: 1000 + i,
            action_type: "heartbeat".to_string(),
            summary: format!("HB entry {}", i),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: None,
        };
        store.insert_action_audit(&entry).await?;
    }
    store.dismiss_audit_entry("hb-0").await?;
    store.dismiss_audit_entry("hb-1").await?;

    let esc_entry = AuditEntryRow {
        id: "esc-0".to_string(),
        timestamp: 2000,
        action_type: "escalation".to_string(),
        summary: "Escalation".to_string(),
        explanation: None,
        confidence: None,
        confidence_band: None,
        causal_trace_id: None,
        thread_id: None,
        goal_run_id: None,
        task_id: None,
        raw_data_json: None,
    };
    store.insert_action_audit(&esc_entry).await?;
    store.dismiss_audit_entry("esc-0").await?;

    let counts = store.count_dismissals_by_type(0).await?;
    assert_eq!(counts.get("heartbeat").copied(), Some(2));
    assert_eq!(counts.get("escalation").copied(), Some(1));

    let shown = store.count_shown_by_type(0).await?;
    assert_eq!(shown.get("heartbeat").copied(), Some(3));
    assert_eq!(shown.get("escalation").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}
