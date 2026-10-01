#![allow(dead_code)]

use super::*;

/// Number of dedicated reader connections for the *background* pool. Each
/// tokio-rusqlite Connection runs on its own background thread, so this is
/// the maximum number of read queries that can execute in parallel under
/// WAL mode for sync loops, indexers, and supervision ticks.
const READ_POOL_SIZE: usize = 8;
/// Reader connections reserved exclusively for user-interactive paths
/// (concierge welcome, thread load, message list). Kept separate so a flood
/// of background reads can never queue ahead of a UI-facing query.
const INTERACTIVE_READ_POOL_SIZE: usize = 4;

async fn apply_sqlite_connection_pragmas(
    conn: &tokio_rusqlite::Connection,
    query_only: bool,
) -> Result<()> {
    conn.call(move |conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "wal_autocheckpoint", "1000")?;
        // Cap the WAL file after checkpoints so a long-lived reader snapshot
        // cannot leave a multi-GB `-wal` file behind forever (observed at
        // 12 GB on 2026-08-30). Only takes effect when a checkpoint actually
        // runs to the current WAL end (see the TRUNCATE checkpoint loop in
        // `run_wal_checkpoint_loop`).
        conn.pragma_update(None, "journal_size_limit", "268435456")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;
        conn.pragma_update(None, "cache_size", super::db::SQLITE_PAGE_CACHE_KIB)?;
        conn.pragma_update(None, "mmap_size", super::db::SQLITE_MMAP_BYTES)?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "query_only", if query_only { "ON" } else { "OFF" })?;
        Ok(())
    })
    .await
    .context("failed to apply SQLite connection pragmas")?;
    Ok(())
}

impl HistoryStore {
    async fn open_for_root(root: &Path) -> Result<Self> {
        let history_dir = root.join("history");
        let skill_dir = root.join("skills").join("generated");
        let telemetry_dir = root.join("semantic-logs");
        let worm_dir = telemetry_dir.join("worm");

        std::fs::create_dir_all(&history_dir)?;
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::create_dir_all(&telemetry_dir)?;
        std::fs::create_dir_all(&worm_dir)?;

        let db_path = history_dir.join("command-history.db");
        let offloaded_payloads_dir = root.join("offloaded-payloads");

        let backend = std::env::var("ZORAI_DB_BACKEND")
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default();
        let use_remote_replica = backend == "remote-replica" || backend == "remote";
        let use_libsql = use_remote_replica || backend == "libsql" || backend == "local-libsql";

        let (
            conn,
            embedding_writer_conn,
            read_conn,
            interactive_read_conn,
            conn_db,
            embedding_writer_db,
            read_db,
            interactive_read_db,
        ): (
            tokio_rusqlite::Connection,
            tokio_rusqlite::Connection,
            super::ReadPool,
            super::ReadPool,
            std::sync::Arc<dyn db::DbConn>,
            std::sync::Arc<dyn db::DbConn>,
            std::sync::Arc<dyn db::DbConn>,
            std::sync::Arc<dyn db::DbConn>,
        ) = if use_libsql {
            let libsql_conn: std::sync::Arc<dyn db::DbConn> = if use_remote_replica {
                let sync_url = std::env::var("ZORAI_DB_SYNC_URL").map_err(|_| {
                    anyhow::anyhow!(
                        "ZORAI_DB_BACKEND=remote-replica requires a sync URL (ZORAI_DB_SYNC_URL or config db_sync_url)"
                    )
                })?;
                let auth_token = zorai_protocol::ZoraiConfig::db_auth_token().unwrap_or_default();
                std::sync::Arc::new(
                    db::libsql::LibsqlConn::open_remote_replica(&db_path, sync_url, auth_token)
                        .await?,
                )
            } else {
                std::sync::Arc::new(db::libsql::LibsqlConn::open_local(&db_path).await?)
            };
            let placeholder = tokio_rusqlite::Connection::open_in_memory()
                .await
                .context("failed to open in-memory placeholder connection for libSQL mode")?;
            (
                placeholder.clone(),
                placeholder.clone(),
                super::ReadPool::new(vec![placeholder.clone()]),
                super::ReadPool::new(vec![placeholder]),
                libsql_conn.clone(),
                libsql_conn.clone(),
                libsql_conn.clone(),
                libsql_conn,
            )
        } else {
            let conn = tokio_rusqlite::Connection::open(&db_path)
                .await
                .context("failed to open SQLite connection via tokio-rusqlite")?;
            apply_sqlite_connection_pragmas(&conn, false).await?;

            let mut read_conns = Vec::with_capacity(READ_POOL_SIZE);
            for _ in 0..READ_POOL_SIZE {
                let read_conn = tokio_rusqlite::Connection::open(&db_path)
                    .await
                    .context("failed to open read SQLite connection via tokio-rusqlite")?;
                apply_sqlite_connection_pragmas(&read_conn, true).await?;
                read_conns.push(read_conn);
            }
            let read_conn = super::ReadPool::new(read_conns);

            let mut interactive_read_conns = Vec::with_capacity(INTERACTIVE_READ_POOL_SIZE);
            for _ in 0..INTERACTIVE_READ_POOL_SIZE {
                let conn = tokio_rusqlite::Connection::open(&db_path)
                    .await
                    .context("failed to open interactive reader SQLite connection")?;
                apply_sqlite_connection_pragmas(&conn, true).await?;
                interactive_read_conns.push(conn);
            }
            let interactive_read_conn = super::ReadPool::new(interactive_read_conns);

            let embedding_writer_conn = tokio_rusqlite::Connection::open(&db_path)
                .await
                .context("failed to open embedding writer SQLite connection")?;
            apply_sqlite_connection_pragmas(&embedding_writer_conn, false).await?;

            let conn_db: std::sync::Arc<dyn db::DbConn> = std::sync::Arc::new(
                db::sqlite::SqliteWriteConn::new(conn.clone(), db_path.clone()),
            );
            let embedding_writer_db: std::sync::Arc<dyn db::DbConn> = std::sync::Arc::new(
                db::sqlite::SqliteWriteConn::new(embedding_writer_conn.clone(), db_path.clone()),
            );
            let read_db: std::sync::Arc<dyn db::DbConn> =
                std::sync::Arc::new(db::sqlite::SqliteReadConn::new(read_conn.clone()));
            let interactive_read_db: std::sync::Arc<dyn db::DbConn> = std::sync::Arc::new(
                db::sqlite::SqliteReadConn::new(interactive_read_conn.clone()),
            );

            (
                conn,
                embedding_writer_conn,
                read_conn,
                interactive_read_conn,
                conn_db,
                embedding_writer_db,
                read_db,
                interactive_read_db,
            )
        };

        super::schema::init_schema_on_connection(
            &mut db::ConnExecutor(&*conn_db),
            &offloaded_payloads_dir,
        )
        .await?;

        let store = Self {
            conn,
            embedding_writer_conn,
            read_conn,
            interactive_read_conn,
            conn_db,
            embedding_writer_db,
            read_db,
            interactive_read_db,
            caches: std::sync::Arc::new(super::HistoryCaches::new()),
            skill_dir,
            telemetry_dir,
            worm_dir,
        };
        Ok(store)
    }

    pub fn data_dir(&self) -> &Path {
        self.skill_dir.parent().unwrap_or(self.skill_dir.as_path())
    }

    pub(crate) fn data_root(&self) -> &Path {
        self.skill_dir
            .parent()
            .and_then(Path::parent)
            .unwrap_or(self.skill_dir.as_path())
    }

    pub(crate) fn offloaded_payloads_dir(&self) -> PathBuf {
        self.data_root().join("offloaded-payloads")
    }

    pub(crate) fn offloaded_payload_path(&self, thread_id: &str, payload_id: &str) -> PathBuf {
        self.offloaded_payloads_dir()
            .join(thread_id)
            .join(format!("{payload_id}.txt"))
    }

    pub(crate) fn tool_output_preview_path(
        &self,
        thread_id: &str,
        goal_run_id: Option<&str>,
        tool_name: &str,
        timestamp: u64,
    ) -> PathBuf {
        let safe_tool_name = sanitize_tool_output_segment(tool_name);
        let preview_dir = zorai_protocol::thread_previews_dir(self.data_root(), thread_id);
        match goal_run_id {
            Some(goal_run_id) => preview_dir.join(format!(
                "{}-{}-{}.txt",
                safe_tool_name,
                sanitize_tool_output_segment(goal_run_id),
                timestamp
            )),
            None => preview_dir.join(format!("{safe_tool_name}-{timestamp}.txt")),
        }
    }

    pub async fn new() -> Result<Self> {
        let base = zorai_protocol::ensure_zorai_data_dir()?;
        Self::open_for_root(&base).await
    }

    pub async fn list_agent_config_items(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let rows = self
            .read_db
            .query(
                "SELECT key_path, value_json FROM agent_config_items WHERE deleted_at IS NULL ORDER BY length(key_path) ASC, key_path ASC",
                db::Params::None,
            )
            .await?;
        let mut items = Vec::new();
        for row in &rows {
            let key_path = row.get::<String>(0)?;
            let value_json = row.get::<String>(1)?;
            let value = serde_json::from_str::<serde_json::Value>(&value_json)?;
            items.push((key_path, value));
        }
        Ok(items)
    }

    pub async fn replace_agent_config_items(
        &self,
        items: &[(String, serde_json::Value)],
    ) -> Result<()> {
        let items: Vec<(String, String)> = items
            .iter()
            .map(|(k, v)| Ok((k.clone(), serde_json::to_string(v)?)))
            .collect::<Result<Vec<_>>>()?;
        let mut txn = self.conn_db.transaction().await?;
        let now = now_ts() as i64;
        txn.execute(
            "UPDATE agent_config_items SET deleted_at = ?1 WHERE deleted_at IS NULL",
            db::db_params![now],
        )
        .await?;
        for (key_path, value_json) in &items {
            txn.execute(
                "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                db::db_params![key_path.clone(), value_json.clone(), now],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn upsert_agent_config_item(
        &self,
        key_path: &str,
        value: &serde_json::Value,
    ) -> Result<()> {
        let key_path = key_path.to_string();
        let value_json = serde_json::to_string(value)?;
        let mut txn = self.conn_db.transaction().await?;
        let prefix = format!("{key_path}/%");
        let now = now_ts() as i64;
        txn.execute(
            "UPDATE agent_config_items SET deleted_at = ?3 \
             WHERE deleted_at IS NULL AND (key_path = ?1 OR key_path LIKE ?2 OR ?1 LIKE key_path || '/%')",
            db::db_params![key_path.clone(), prefix, now],
        )
        .await?;
        txn.execute(
            "INSERT OR REPLACE INTO agent_config_items (key_path, value_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
            db::db_params![key_path.clone(), value_json.clone(), now],
        )
        .await?;
        txn.execute(
            "INSERT INTO agent_config_updates (id, key_path, value_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
            db::db_params![uuid::Uuid::new_v4().to_string(), key_path, value_json, now],
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn load_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
    ) -> Result<Option<ProviderAuthStateRow>> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        let row = self
            .read_db
            .query_opt(
                "SELECT provider_id, auth_mode, state_json, updated_at
                 FROM provider_auth_state
                 WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
                db::db_params![provider_id, auth_mode],
            )
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let state_json = row.get::<String>(2)?;
        let parsed = serde_json::from_str::<serde_json::Value>(&state_json)?;
        Ok(Some(ProviderAuthStateRow {
            provider_id: row.get(0)?,
            auth_mode: row.get(1)?,
            state_json: parsed,
            updated_at: row.get(3)?,
        }))
    }

    pub async fn save_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
        state: &serde_json::Value,
    ) -> Result<()> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        let state_json = serde_json::to_string(state)?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO provider_auth_state
                 (provider_id, auth_mode, state_json, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
                db::db_params![provider_id, auth_mode, state_json, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_provider_auth_state(
        &self,
        provider_id: &str,
        auth_mode: &str,
    ) -> Result<()> {
        let provider_id = provider_id.to_string();
        let auth_mode = auth_mode.to_string();
        self.conn_db
            .execute(
                "UPDATE provider_auth_state SET deleted_at = ?3 WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
                db::db_params![provider_id, auth_mode, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn new_test_store(root: &Path) -> Result<Self> {
        Self::open_for_root(root).await
    }

    pub(crate) async fn open_for_data_root(root: &Path) -> Result<Self> {
        Self::open_for_root(root).await
    }

    pub async fn record_managed_finish(&self, record: &ManagedHistoryRecord) -> Result<()> {
        let timestamp = now_ts() as i64;
        let excerpt = format!(
            "exit={:?} duration_ms={:?} snapshot={} rationale={}",
            record.exit_code,
            record.duration_ms,
            record.snapshot_path.as_deref().unwrap_or("none"),
            record.rationale
        );

        let execution_id = record.execution_id.clone();
        let command = record.command.clone();
        let rationale = record.rationale.clone();
        let snapshot_path = record.snapshot_path.clone();
        let excerpt_clone = excerpt.clone();
        let record = record;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO history_entries (id, kind, title, excerpt, content, path, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    execution_id.clone(),
                    "managed-command",
                    command.clone(),
                    excerpt_clone.clone(),
                    format!("{}\n{}", command, rationale),
                    snapshot_path,
                    timestamp,
                ],
            )
            .await?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO history_fts (id, title, excerpt, content) VALUES (?1, ?2, ?3, ?4)",
                db::db_params![execution_id, command, excerpt_clone, rationale],
            )
            .await?;

        self.append_telemetry(
            "operational",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "session_id": record.session_id,
                "workspace_id": record.workspace_id,
                "command": record.command,
                "exit_code": record.exit_code,
                "duration_ms": record.duration_ms,
                "snapshot": record.snapshot_path,
            }),
        )
        .await?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "source": record.source,
                "rationale": record.rationale,
            }),
        )
        .await?;

        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu();
        self.append_telemetry(
            "contextual",
            json!({
                "timestamp": timestamp,
                "execution_id": record.execution_id,
                "total_memory": system.total_memory(),
                "used_memory": system.used_memory(),
                "cpu_usage": system.global_cpu_info().cpu_usage(),
            }),
        )
        .await?;

        Ok(())
    }

    pub async fn get_managed_finish(
        &self,
        execution_id: &str,
    ) -> Result<Option<ManagedCommandFinishedRecord>> {
        let execution_id = execution_id.to_string();
        let Some(row) = self
            .read_db
            .query_opt(
                "SELECT title, excerpt, path FROM history_entries WHERE id = ?1 AND kind = 'managed-command'",
                db::db_params![execution_id],
            )
            .await?
        else {
            return Ok(None);
        };
        let command: String = row.get(0)?;
        let excerpt: String = row.get(1)?;
        let snapshot_path: Option<String> = row.get(2)?;
        let (exit_code, duration_ms) = parse_managed_history_excerpt(&excerpt);
        Ok(Some(ManagedCommandFinishedRecord {
            command,
            exit_code,
            duration_ms,
            snapshot_path,
        }))
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        self.search_with_sqlite_fts(query, limit).await
    }

    async fn search_with_sqlite_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let query = query.to_string();
        let rows = self
            .read_db
            .query(
                "SELECT history_entries.id, history_entries.kind, history_entries.title, history_entries.excerpt, history_entries.path, history_entries.timestamp, bm25(history_fts) \
                 FROM history_fts JOIN history_entries ON history_entries.id = history_fts.id \
                 WHERE history_fts MATCH ?1 ORDER BY bm25(history_fts) LIMIT ?2",
                db::db_params![query.clone(), limit as i64],
            )
            .await?;
        let hits = rows
            .iter()
            .filter_map(|row| {
                Some(HistorySearchHit {
                    id: row.get(0).ok()?,
                    kind: row.get(1).ok()?,
                    title: row.get(2).ok()?,
                    excerpt: row.get(3).ok()?,
                    path: row.get(4).ok()?,
                    timestamp: row.get::<i64>(5).ok()? as u64,
                    score: row.get(6).ok()?,
                })
            })
            .collect::<Vec<_>>();
        let summary = if hits.is_empty() {
            format!("No prior runs matched '{query}'.")
        } else {
            format!("Found {} historical matches for '{query}'.", hits.len())
        };
        Ok((summary, hits))
    }

    pub async fn generate_skill(
        &self,
        query: Option<&str>,
        title: Option<&str>,
    ) -> Result<(String, String)> {
        let title = title.unwrap_or("Recovered Workflow").trim();
        let (summary, hits) = self
            .search(query.unwrap_or("*"), 8)
            .await
            .unwrap_or_else(|_| ("No history available.".to_string(), Vec::new()));
        let safe_name = title
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        let path = self.skill_dir.join(format!(
            "{}.md",
            if safe_name.is_empty() {
                "recovered-workflow"
            } else {
                &safe_name
            }
        ));
        let mut body = format!(
            "# {}\n\n## Summary\n{}\n\n## Retrieved Steps\n",
            title, summary
        );
        for hit in &hits {
            body.push_str(&format!("- {}\n", hit.title));
            body.push_str(&format!("  {}\n", hit.excerpt));
        }
        if hits.is_empty() {
            body.push_str("- No matching executions were available.\n");
        }
        std::fs::write(&path, body)
            .with_context(|| format!("failed to write {}", path.display()))?;
        self.register_skill_document(&path).await?;
        Ok((title.to_string(), path.to_string_lossy().into_owned()))
    }
}

/// Periodically syncs an embedded libSQL remote replica against its server.
/// A no-op for the SQLite engine and local libSQL (their `conn_db.sync()`
/// returns `Ok(())`), so it is harmless to always spawn. Syncs once at startup,
/// on each interval tick, and once more on shutdown.
pub(crate) async fn run_db_sync_loop(
    conn_db: std::sync::Arc<dyn db::DbConn>,
    interval_secs: u64,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if let Err(e) = conn_db.sync().await {
        tracing::warn!(error = %e, "initial database replica sync failed");
    }
    let interval = std::time::Duration::from_secs(interval_secs.max(1));
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                if let Err(e) = conn_db.sync().await {
                    tracing::warn!(error = %e, "periodic database replica sync failed");
                }
            }
            _ = shutdown.changed() => {
                if let Err(e) = conn_db.sync().await {
                    tracing::warn!(error = %e, "final database replica sync on shutdown failed");
                }
                return;
            }
        }
    }
}

/// Periodically runs `PRAGMA wal_checkpoint(TRUNCATE)` on the primary
/// writer connection. Autocheckpoint (1000 pages) cannot run while any
/// reader holds a snapshot older than the WAL end, and zorai keeps 12+
/// long-lived read connections — so under sustained multi-thread load the
/// WAL grew to 12 GB and never shrank (2026-08-30 investigation). TRUNCATE
/// checkpoints block new writers only for the final rename/commit step and
/// retry harmlessly on `SQLITE_BUSY`, so they are safe alongside live
/// traffic. `journal_size_limit` (set per connection) bounds the file size
/// after each successful truncation.
pub(crate) async fn run_wal_checkpoint_loop(
    conn: tokio_rusqlite::Connection,
    interval: std::time::Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                match conn
                    .call(|c| Ok(c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?))
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::debug!(error = %e, "periodic WAL truncate checkpoint skipped");
                    }
                }
            }
            _ = shutdown.changed() => {
                if let Err(e) = conn
                    .call(|c| Ok(c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?))
                    .await
                {
                    tracing::debug!(error = %e, "shutdown WAL truncate checkpoint failed");
                }
                return;
            }
        }
    }
}

fn sanitize_tool_output_segment(raw: &str) -> String {
    raw.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => ch,
            _ => '_',
        })
        .collect()
}

fn parse_managed_history_excerpt(excerpt: &str) -> (Option<i32>, Option<u64>) {
    let Some(rest) = excerpt.strip_prefix("exit=") else {
        return (None, None);
    };
    let Some((exit_raw, remaining)) = rest.split_once(" duration_ms=") else {
        return (None, None);
    };
    let duration_raw = remaining
        .split_once(" snapshot=")
        .map(|(value, _)| value)
        .unwrap_or(remaining);

    (
        parse_debug_option_i32(exit_raw),
        parse_debug_option_u64(duration_raw),
    )
}

fn parse_debug_option_i32(raw: &str) -> Option<i32> {
    raw.strip_prefix("Some(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<i32>().ok())
}

fn parse_debug_option_u64(raw: &str) -> Option<u64> {
    raw.strip_prefix("Some(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<u64>().ok())
}
