use super::*;

fn now_millis_i64() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadMessagePinState {
    pub message_chars: usize,
    pub pinned_for_compaction: bool,
    pub counts_toward_pinned_chars: bool,
    pub current_pinned_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadUserPacing {
    pub recent_message_count: u32,
    pub avg_gap_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MessageFeedbackState {
    pub role: String,
    pub reaction: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, content: &str) -> AgentDbMessage {
        AgentDbMessage {
            id: id.to_string(),
            thread_id: "thread-1".to_string(),
            created_at: 100,
            role: "user".to_string(),
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
        }
    }

    #[test]
    fn message_fixture_sets_thread_id() {
        assert_eq!(message("m1", "body").thread_id, "thread-1");
    }
}

impl HistoryStore {
    /// Returns the highest `created_at` already persisted for this thread,
    /// or `None` if the thread has no rows yet. Used by
    /// `persist_thread_snapshot` to skip rebuilding `AgentDbMessage` (with
    /// metadata_json / tool_calls_json serialization) for messages that are
    /// already on disk. Without this, persisting a 3,500-message thread
    /// after a single new turn re-encoded every old message's JSON twice
    /// (once on the Rust side, once via `INSERT OR REPLACE` in
    /// `reconcile_thread_snapshot`).
    pub(crate) async fn thread_latest_persisted_message_ts(
        &self,
        thread_id: &str,
    ) -> Result<Option<i64>> {
        let row = self
            .interactive_read_db
            .query_opt(
                "SELECT MAX(created_at) FROM agent_messages \
                 WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| row.get::<Option<i64>>(0))
            .transpose()
            .map(Option::flatten)
    }

    pub async fn reconcile_thread_snapshot(
        &self,
        thread: &AgentDbThread,
        messages: &[AgentDbMessage],
    ) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&thread.id);
        let mut txn = self.conn_db.transaction().await?;
        // Use the thread row's full message count, not `messages.len()` —
        // the caller filters the message batch by a cutoff timestamp, so a
        // re-persist that only carries metadata changes has `messages` near
        // zero and would spuriously trip the stale-snapshot gate even though
        // the conversation has not shrunk.
        let incoming_message_count = thread.message_count;
        let incoming_latest_created_at = messages
            .iter()
            .map(|message| message.created_at)
            .max()
            .unwrap_or(thread.updated_at);

        let existing_snapshot = txn
            .query_opt(
                "SELECT
                    updated_at,
                    message_count,
                    COALESCE((
                        SELECT MAX(created_at)
                        FROM agent_messages
                        WHERE thread_id = ?1 AND deleted_at IS NULL
                    ), 0)
                 FROM agent_threads
                 WHERE id = ?1",
                db::db_params![thread.id.clone()],
            )
            .await?
            .map(|row| -> anyhow::Result<(i64, i64, i64)> {
                Ok((row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?))
            })
            .transpose()?;

        if let Some((existing_updated_at, existing_message_count, existing_latest_created_at)) =
            existing_snapshot
        {
            let stale_snapshot = existing_updated_at > thread.updated_at
                || (existing_updated_at == thread.updated_at
                    && (existing_message_count > incoming_message_count
                        || existing_latest_created_at > incoming_latest_created_at));
            if stale_snapshot {
                tracing::debug!(
                    thread_id = %thread.id,
                    existing_updated_at,
                    incoming_updated_at = thread.updated_at,
                    existing_message_count,
                    incoming_message_count,
                    existing_latest_created_at,
                    incoming_latest_created_at,
                    "skipping stale thread snapshot persistence"
                );
                return Ok(());
            }
        }

        txn.execute(
            "INSERT INTO agent_threads \
             (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
             ON CONFLICT(id) DO UPDATE SET \
                workspace_id = excluded.workspace_id, \
                surface_id = excluded.surface_id, \
                pane_id = excluded.pane_id, \
                agent_name = excluded.agent_name, \
                title = excluded.title, \
                created_at = excluded.created_at, \
                updated_at = excluded.updated_at, \
                message_count = excluded.message_count, \
                total_tokens = excluded.total_tokens, \
                last_preview = excluded.last_preview, \
                metadata_json = excluded.metadata_json",
            db::db_params![
                thread.id.clone(),
                thread.workspace_id.clone(),
                thread.surface_id.clone(),
                thread.pane_id.clone(),
                thread.agent_name.clone(),
                thread.title.clone(),
                thread.created_at,
                thread.updated_at,
                thread.message_count,
                thread.total_tokens,
                thread.last_preview.clone(),
                thread.metadata_json.clone(),
            ],
        )
        .await?;

        let cutoff_ts = existing_snapshot
            .map(|(_, _, latest)| latest)
            .unwrap_or(i64::MIN);
        let mut written = 0usize;
        for message in messages {
            if message.created_at < cutoff_ts {
                continue;
            }
            txn.execute(
                "INSERT OR REPLACE INTO agent_messages \
                 (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
                db::db_params![
                    message.id.clone(),
                    message.thread_id.clone(),
                    message.created_at,
                    message.role.clone(),
                    message.content.clone(),
                    message.provider.clone(),
                    message.model.clone(),
                    message.input_tokens,
                    message.output_tokens,
                    message.total_tokens,
                    message.cost_usd,
                    message.reasoning.clone(),
                    message.tool_calls_json.clone(),
                    message.metadata_json.clone(),
                ],
            )
            .await?;
            embedding_queue::enqueue_message_embedding_job_exec(
                &mut *txn,
                message,
                thread.workspace_id.as_deref(),
                now_ts() as i64,
            )
            .await?;
            written += 1;
        }
        tracing::debug!(
            thread_id = %thread.id,
            in_memory_count = messages.len(),
            written_count = written,
            cutoff_ts,
            "thread snapshot persisted (incremental)"
        );

        if messages.is_empty() {
            txn.execute(
                "UPDATE agent_threads SET message_count = ?2, total_tokens = ?3, last_preview = ?4, updated_at = ?5 WHERE id = ?1",
                db::db_params![
                    thread.id.clone(),
                    thread.message_count,
                    thread.total_tokens,
                    thread.last_preview.clone(),
                    thread.updated_at,
                ],
            )
            .await?;
        } else {
            refresh_thread_stats_exec(&mut *txn, &thread.id).await?;
        }

        txn.commit().await?;
        Ok(())
    }

    pub async fn create_thread(&self, thread: &AgentDbThread) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&thread.id);
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO agent_threads \
                 (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                db::db_params![
                    thread.id.clone(),
                    thread.workspace_id.clone(),
                    thread.surface_id.clone(),
                    thread.pane_id.clone(),
                    thread.agent_name.clone(),
                    thread.title.clone(),
                    thread.created_at,
                    thread.updated_at,
                    thread.message_count,
                    thread.total_tokens,
                    thread.last_preview.clone(),
                    thread.metadata_json.clone(),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn replace_thread_snapshot(
        &self,
        thread: &AgentDbThread,
        messages: &[AgentDbMessage],
    ) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&thread.id);
        let mut txn = self.conn_db.transaction().await?;
        // Use the thread row's full message count, not `messages.len()` —
        // the caller filters the message batch by a cutoff timestamp, so a
        // re-persist that only carries metadata changes has `messages` near
        // zero and would spuriously trip the stale-snapshot gate even though
        // the conversation has not shrunk.
        let incoming_message_count = thread.message_count;
        let incoming_latest_created_at = messages
            .iter()
            .map(|message| message.created_at)
            .max()
            .unwrap_or(thread.updated_at);

        let existing_snapshot = txn
            .query_opt(
                "SELECT
                    updated_at,
                    message_count,
                    COALESCE((
                        SELECT MAX(created_at)
                        FROM agent_messages
                        WHERE thread_id = ?1 AND deleted_at IS NULL
                    ), 0)
                 FROM agent_threads
                 WHERE id = ?1",
                db::db_params![thread.id.clone()],
            )
            .await?
            .map(|row| -> anyhow::Result<(i64, i64, i64)> {
                Ok((row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?))
            })
            .transpose()?;

        if let Some((existing_updated_at, existing_message_count, existing_latest_created_at)) =
            existing_snapshot
        {
            let stale_snapshot = existing_updated_at > thread.updated_at
                || (existing_updated_at == thread.updated_at
                    && (existing_message_count > incoming_message_count
                        || existing_latest_created_at > incoming_latest_created_at));
            if stale_snapshot {
                tracing::debug!(
                    thread_id = %thread.id,
                    existing_updated_at,
                    incoming_updated_at = thread.updated_at,
                    existing_message_count,
                    incoming_message_count,
                    existing_latest_created_at,
                    incoming_latest_created_at,
                    "skipping stale thread snapshot replace"
                );
                return Ok(());
            }
        }

        let existing_ids = txn
            .query(
                "SELECT id FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread.id.clone()],
            )
            .await?
            .iter()
            .map(|row| row.get::<String>(0))
            .collect::<anyhow::Result<std::collections::HashSet<_>>>()?;

        let incoming_ids = messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<std::collections::HashSet<_>>();
        let stale_ids = existing_ids
            .into_iter()
            .filter(|id| !incoming_ids.contains(id))
            .collect::<Vec<_>>();

        txn.execute(
            "INSERT INTO agent_threads \
             (id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) \
             ON CONFLICT(id) DO UPDATE SET \
                workspace_id = excluded.workspace_id, \
                surface_id = excluded.surface_id, \
                pane_id = excluded.pane_id, \
                agent_name = excluded.agent_name, \
                title = excluded.title, \
                created_at = excluded.created_at, \
                updated_at = excluded.updated_at, \
                message_count = excluded.message_count, \
                total_tokens = excluded.total_tokens, \
                last_preview = excluded.last_preview, \
                metadata_json = excluded.metadata_json",
            db::db_params![
                thread.id.clone(),
                thread.workspace_id.clone(),
                thread.surface_id.clone(),
                thread.pane_id.clone(),
                thread.agent_name.clone(),
                thread.title.clone(),
                thread.created_at,
                thread.updated_at,
                thread.message_count,
                thread.total_tokens,
                thread.last_preview.clone(),
                thread.metadata_json.clone(),
            ],
        )
        .await?;

        if !stale_ids.is_empty() {
            let placeholders = std::iter::repeat_n("?", stale_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "UPDATE agent_messages SET deleted_at = ? WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({placeholders})"
            );
            let mut values = vec![
                db::Value::Integer(now_millis_i64()),
                db::Value::Text(thread.id.clone()),
            ];
            for id in &stale_ids {
                values.push(db::Value::Text(id.clone()));
            }
            txn.execute(&sql, db::Params::Positional(values)).await?;
            for id in &stale_ids {
                embedding_queue::queue_embedding_deletion_exec(
                    &mut *txn,
                    "agent_message",
                    id,
                    now_ts() as i64,
                )
                .await?;
            }
        }

        for message in messages {
            txn.execute(
                "INSERT OR REPLACE INTO agent_messages \
                 (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
                db::db_params![
                    message.id.clone(),
                    message.thread_id.clone(),
                    message.created_at,
                    message.role.clone(),
                    message.content.clone(),
                    message.provider.clone(),
                    message.model.clone(),
                    message.input_tokens,
                    message.output_tokens,
                    message.total_tokens,
                    message.cost_usd,
                    message.reasoning.clone(),
                    message.tool_calls_json.clone(),
                    message.metadata_json.clone(),
                ],
            )
            .await?;
            embedding_queue::enqueue_message_embedding_job_exec(
                &mut *txn,
                message,
                thread.workspace_id.as_deref(),
                now_ts() as i64,
            )
            .await?;
        }

        if messages.is_empty() {
            txn.execute(
                "UPDATE agent_threads SET message_count = ?2, total_tokens = ?3, last_preview = ?4, updated_at = ?5 WHERE id = ?1",
                db::db_params![
                    thread.id.clone(),
                    thread.message_count,
                    thread.total_tokens,
                    thread.last_preview.clone(),
                    thread.updated_at,
                ],
            )
            .await?;
        } else {
            refresh_thread_stats_exec(&mut *txn, &thread.id).await?;
        }

        txn.commit().await?;
        Ok(())
    }

    pub async fn delete_thread(&self, id: &str) -> Result<()> {
        self.caches.thread_metadata_json.invalidate(&id.to_string());
        let deleted_at = now_millis_i64();
        let mut txn = self.conn_db.transaction().await?;
        let message_ids = txn
            .query(
                "SELECT id FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![id],
            )
            .await?
            .iter()
            .map(|row| row.get::<String>(0))
            .collect::<anyhow::Result<Vec<_>>>()?;
        embedding_queue::queue_embedding_deletions_exec(
            &mut *txn,
            "agent_message",
            &message_ids,
            now_ts() as i64,
        )
        .await?;
        txn.execute(
            "UPDATE agent_threads SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            db::db_params![id, deleted_at],
        )
        .await?;
        txn.execute(
            "UPDATE agent_messages SET deleted_at = ?2 WHERE thread_id = ?1 AND deleted_at IS NULL",
            db::db_params![id, deleted_at],
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_threads(&self) -> Result<Vec<AgentDbThread>> {
        let rows = self
            .interactive_read_db
            .query(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                 FROM agent_threads WHERE deleted_at IS NULL ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .filter_map(|row| map_agent_thread_db(row).ok())
            .map(Ok)
            .collect()
    }

    pub(crate) async fn list_threads_filtered(
        &self,
        query: &AgentThreadListQuery,
    ) -> Result<Vec<AgentDbThread>> {
        let mut sql = String::from(
            "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
             FROM agent_threads t WHERE t.deleted_at IS NULL",
        );
        let mut values = Vec::<db::Value>::new();
        {
            if let Some(created_after) = query.created_after {
                sql.push_str(" AND t.created_at >= ?");
                values.push(db::Value::Integer(created_after));
            }
            if let Some(created_before) = query.created_before {
                sql.push_str(" AND t.created_at <= ?");
                values.push(db::Value::Integer(created_before));
            }
            if let Some(updated_after) = query.updated_after {
                sql.push_str(" AND t.updated_at >= ?");
                values.push(db::Value::Integer(updated_after));
            }
            if let Some(updated_before) = query.updated_before {
                sql.push_str(" AND t.updated_at <= ?");
                values.push(db::Value::Integer(updated_before));
            }
            if let Some(title_query) = query
                .title_query
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                sql.push_str(" AND instr(lower(t.title), lower(?)) > 0");
                values.push(db::Value::Text(title_query.to_string()));
            }
            for prefix in &query.title_excluded_prefixes {
                sql.push_str(" AND t.title NOT LIKE ?");
                values.push(db::Value::Text(format!("{prefix}%")));
            }
            if let Some(min_message_count) = query.min_message_count {
                sql.push_str(" AND t.message_count >= ?");
                values.push(db::Value::Integer(min_message_count.max(0)));
            }
            if let Some(pinned) = query.pinned {
                sql.push_str(" AND t.pinned = ?");
                values.push(db::Value::Integer(if pinned { 1 } else { 0 }));
            }
            if !query.agent_names.is_empty() || query.include_empty_agent_name {
                let mut clauses = Vec::new();
                if query.include_empty_agent_name {
                    clauses.push("(t.agent_name IS NULL OR trim(t.agent_name) = '')".to_string());
                }
                if !query.agent_names.is_empty() {
                    let placeholders = std::iter::repeat_n("?", query.agent_names.len())
                        .collect::<Vec<_>>()
                        .join(", ");
                    clauses.push(format!("lower(trim(t.agent_name)) IN ({placeholders})"));
                    values.extend(
                        query
                            .agent_names
                            .iter()
                            .map(|name| db::Value::Text(name.trim().to_ascii_lowercase())),
                    );
                }
                sql.push_str(" AND (");
                sql.push_str(&clauses.join(" OR "));
                sql.push(')');
            }
            if !query.excluded_ids.is_empty() {
                let placeholders = std::iter::repeat_n("?", query.excluded_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                sql.push_str(&format!(" AND t.id NOT IN ({placeholders})"));
                values.extend(
                    query
                        .excluded_ids
                        .iter()
                        .map(|id| db::Value::Text(id.to_string())),
                );
            }
            if !query.include_internal {
                for prefix in &query.hidden_id_prefixes {
                    sql.push_str(" AND t.id NOT LIKE ?");
                    values.push(db::Value::Text(format!("{prefix}%")));
                }
                if !query.hidden_message_substrings.is_empty() {
                    sql.push_str(
                        " AND NOT EXISTS (
                                SELECT 1 FROM agent_messages m
                                WHERE m.thread_id = t.id
                                  AND m.deleted_at IS NULL
                                  AND (",
                    );
                    let clauses = std::iter::repeat_n(
                        "instr(lower(substr(m.content, 1, 240)), lower(?)) > 0",
                        query.hidden_message_substrings.len(),
                    )
                    .collect::<Vec<_>>()
                    .join(" OR ");
                    sql.push_str(&clauses);
                    sql.push_str("))");
                    values.extend(
                        query
                            .hidden_message_substrings
                            .iter()
                            .map(|substring| db::Value::Text(substring.to_string())),
                    );
                }
            }

            sql.push_str(" ORDER BY t.updated_at DESC, t.id ASC");
            if let Some(limit) = query.limit {
                sql.push_str(" LIMIT ?");
                values.push(db::Value::Integer(limit as i64));
                if query.offset > 0 {
                    sql.push_str(" OFFSET ?");
                    values.push(db::Value::Integer(query.offset as i64));
                }
            } else if query.offset > 0 {
                sql.push_str(" LIMIT -1 OFFSET ?");
                values.push(db::Value::Integer(query.offset as i64));
            }
        }
        let rows = self
            .interactive_read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_agent_thread_db(row).ok())
            .collect())
    }

    pub async fn get_thread(&self, id: &str) -> Result<Option<AgentDbThread>> {
        let row = self
            .interactive_read_db
            .query_opt(
                "SELECT id, workspace_id, surface_id, pane_id, agent_name, title, created_at, updated_at, message_count, total_tokens, last_preview, metadata_json \
                 FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id],
            )
            .await?;
        row.map(|row| map_agent_thread_db(&row)).transpose()
    }

    pub(crate) async fn thread_recall_match_rows(
        &self,
        tokens: &[String],
        thread_limit: usize,
    ) -> Result<Vec<ThreadRecallMatchRow>> {
        let tokens = tokens
            .iter()
            .map(|token| token.trim().to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let thread_limit = thread_limit.max(1) as i64;
        let (sql, values) = {
            let message_clauses = std::iter::repeat_n("lower(m.content) LIKE ?", tokens.len())
                .collect::<Vec<_>>()
                .join(" OR ");
            let exists_clauses =
                std::iter::repeat_n("lower(candidate.content) LIKE ?", tokens.len())
                    .collect::<Vec<_>>()
                    .join(" OR ");
            let title_clauses = std::iter::repeat_n("lower(t.title) LIKE ?", tokens.len())
                .collect::<Vec<_>>()
                .join(" OR ");
            let sql = format!(
                "WITH matched_threads AS (
                         SELECT t.id
                           FROM agent_threads t
                          WHERE t.deleted_at IS NULL
                            AND (
                                  {title_clauses}
                                  OR EXISTS (
                                      SELECT 1
                                        FROM agent_messages candidate
                                       WHERE candidate.thread_id = t.id
                                         AND candidate.deleted_at IS NULL
                                         AND ({exists_clauses})
                                  )
                            )
                          ORDER BY t.updated_at DESC, t.id ASC
                          LIMIT ?
                     )
                     SELECT t.id,
                            t.title,
                            t.updated_at,
                            t.message_count,
                            t.metadata_json,
                            m.role,
                            m.content
                       FROM matched_threads mt
                       JOIN agent_threads t ON t.id = mt.id
                       LEFT JOIN agent_messages m
                         ON m.thread_id = t.id
                        AND m.deleted_at IS NULL
                        AND ({message_clauses})
                      ORDER BY t.updated_at DESC, t.id ASC, m.created_at DESC, m.rowid DESC"
            );
            let mut values = Vec::<db::Value>::new();
            values.extend(
                tokens
                    .iter()
                    .map(|token| db::Value::Text(format!("%{token}%"))),
            );
            values.extend(
                tokens
                    .iter()
                    .map(|token| db::Value::Text(format!("%{token}%"))),
            );
            values.push(db::Value::Integer(thread_limit));
            values.extend(
                tokens
                    .iter()
                    .map(|token| db::Value::Text(format!("%{token}%"))),
            );
            (sql, values)
        };

        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter()
            .map(|row| {
                Ok(ThreadRecallMatchRow {
                    thread_id: row.get(0)?,
                    title: row.get(1)?,
                    updated_at: row.get::<i64>(2)?.max(0) as u64,
                    message_count: row.get::<i64>(3)?.max(0) as u32,
                    metadata_json: row.get(4)?,
                    message_role: row.get(5)?,
                    message_content: row.get(6)?,
                })
            })
            .collect()
    }

    pub(crate) async fn has_thread_id(&self, id: &str) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT 1 FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL LIMIT 1",
                db::db_params![id],
            )
            .await?;
        Ok(row.is_some())
    }

    pub(crate) async fn thread_created_at(&self, id: &str) -> Result<Option<u64>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT created_at FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id],
            )
            .await?;
        Ok(row
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .map(|created_at| created_at.max(0) as u64))
    }

    pub(crate) async fn thread_metadata_json(&self, id: &str) -> Result<Option<String>> {
        let id_owned = id.to_string();
        if let Some(cached) = self.caches.thread_metadata_json.get(&id_owned) {
            return Ok(cached);
        }
        let value: Option<String> = self
            .read_db
            .query_opt(
                "SELECT metadata_json FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id_owned.as_str()],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?
            .flatten();
        self.caches
            .thread_metadata_json
            .insert(id_owned, value.clone());
        Ok(value)
    }

    pub(crate) async fn thread_delegate_payload_context(
        &self,
        id: &str,
        message_limit: usize,
    ) -> Result<Option<ThreadDelegatePayloadContext>> {
        let message_limit = message_limit.min(i64::MAX as usize) as i64;
        let Some(title) = self
            .read_db
            .query_opt(
                "SELECT title FROM agent_threads WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id],
            )
            .await?
            .map(|row| row.get::<String>(0))
            .transpose()?
        else {
            return Ok(None);
        };

        let messages = if message_limit == 0 {
            Vec::new()
        } else {
            let rows = self
                .read_db
                .query(
                    "SELECT role, content FROM (\
                     SELECT role, content, created_at, rowid \
                     FROM agent_messages \
                     WHERE thread_id = ?1 AND deleted_at IS NULL \
                     ORDER BY created_at DESC, rowid DESC \
                     LIMIT ?2\
                     ) ORDER BY created_at ASC, rowid ASC",
                    db::db_params![id, message_limit],
                )
                .await?;
            rows.iter()
                .map(|row| {
                    Ok(ThreadDelegatePayloadMessageRef {
                        role: row.get(0)?,
                        content: row.get(1)?,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };

        Ok(Some(ThreadDelegatePayloadContext { title, messages }))
    }

    pub(crate) async fn has_non_heartbeat_user_message_after(
        &self,
        timestamp_ms: u64,
    ) -> Result<bool> {
        let timestamp_ms = timestamp_ms.min(i64::MAX as u64) as i64;
        let row = self
            .read_db
            .query_opt(
                "SELECT EXISTS (
                    SELECT 1
                    FROM agent_messages message
                    JOIN agent_threads thread
                      ON thread.id = message.thread_id
                     AND thread.deleted_at IS NULL
                    WHERE message.role = 'user'
                      AND message.deleted_at IS NULL
                      AND message.created_at > ?1
                      AND thread.title NOT GLOB 'HEARTBEAT SYNTHESIS*'
                      AND thread.title NOT GLOB 'Heartbeat check:*'
                      AND NOT EXISTS (
                        SELECT 1
                        FROM agent_messages heartbeat
                        WHERE heartbeat.thread_id = message.thread_id
                          AND heartbeat.role = 'user'
                          AND heartbeat.deleted_at IS NULL
                          AND (
                            heartbeat.content GLOB 'HEARTBEAT SYNTHESIS*'
                            OR heartbeat.content GLOB 'Heartbeat check:*'
                          )
                      )
                    LIMIT 1
                )",
                db::db_params![timestamp_ms],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub(crate) async fn thread_has_unanswered_tool_calls(&self, id: &str) -> Result<bool> {
        let owned = vec![id.to_string()];
        let unanswered = self.thread_ids_with_unanswered_tool_calls(&owned).await?;
        Ok(!unanswered.is_empty())
    }

    pub(crate) async fn thread_ids_with_unanswered_tool_calls(
        &self,
        thread_ids: &[String],
    ) -> Result<Vec<String>> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let chunk_size = sqlite_in_chunk_size(1);
        let mut unanswered: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for chunk in thread_ids.chunks(chunk_size) {
            let placeholders = std::iter::repeat("?")
                .take(chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT
                     thread_id,
                     role,
                     CASE
                         WHEN role = 'assistant'
                              AND typeof(tool_calls_json) = 'text'
                              AND json_valid(tool_calls_json)
                              AND json_array_length(tool_calls_json) > 0
                         THEN tool_calls_json
                         ELSE NULL
                     END AS tool_calls_json,
                     CASE
                         WHEN role = 'tool'
                              AND typeof(metadata_json) = 'text'
                              AND json_valid(metadata_json)
                         THEN COALESCE(
                             json_extract(metadata_json, '$.tool_call_id'),
                             json_extract(metadata_json, '$.toolCallId')
                         )
                         ELSE NULL
                     END AS tool_call_id
                 FROM agent_messages
                 WHERE thread_id IN ({placeholders})
                   AND deleted_at IS NULL
                 ORDER BY thread_id ASC, created_at ASC, id ASC"
            );
            let values = chunk
                .iter()
                .cloned()
                .map(db::Value::Text)
                .collect::<Vec<_>>();
            let rows = self
                .read_db
                .query(&sql, db::Params::Positional(values))
                .await?;

            let mut current_thread: Option<String> = None;
            let mut buffer: Vec<(String, Option<String>, Option<String>)> = Vec::new();
            for row in rows.iter() {
                let thread_id = row.get::<String>(0)?;
                let role = row.get::<String>(1)?;
                let tool_calls_json = row.get::<Option<String>>(2)?;
                let tool_call_id = row.get::<Option<String>>(3)?;
                if current_thread.as_deref() != Some(thread_id.as_str()) {
                    if let Some(prev_id) = current_thread.take() {
                        if scan_messages_for_unanswered_tool_calls(&buffer)
                            && seen.insert(prev_id.clone())
                        {
                            unanswered.push(prev_id);
                        }
                    }
                    current_thread = Some(thread_id);
                    buffer.clear();
                }
                buffer.push((role, tool_calls_json, tool_call_id));
            }
            if let Some(prev_id) = current_thread.take() {
                if scan_messages_for_unanswered_tool_calls(&buffer) && seen.insert(prev_id.clone())
                {
                    unanswered.push(prev_id);
                }
            }
        }
        Ok(unanswered)
    }

    pub(crate) async fn latest_thread_id_by_message_timestamp(&self) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT m.thread_id
                 FROM agent_messages m
                 JOIN agent_threads t
                   ON t.id = m.thread_id AND t.deleted_at IS NULL
                 WHERE m.deleted_at IS NULL
                 ORDER BY m.created_at DESC, m.id DESC
                 LIMIT 1",
                db::Params::None,
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    pub(crate) async fn latest_thread_context_hint(&self) -> Result<Option<(String, String, u64)>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT id, trim(last_preview), updated_at
                 FROM agent_threads
                 WHERE deleted_at IS NULL
                   AND message_count > 0
                   AND trim(last_preview) <> ''
                 ORDER BY updated_at DESC, id ASC
                 LIMIT 1",
                db::Params::None,
            )
            .await?;
        row.map(|row| -> anyhow::Result<(String, String, u64)> {
            let updated_at = row.get::<i64>(2)?.max(0) as u64;
            Ok((row.get(0)?, row.get(1)?, updated_at))
        })
        .transpose()
    }

    pub(crate) async fn thread_has_message_substring(
        &self,
        thread_id: &str,
        substrings: &[String],
    ) -> Result<bool> {
        if substrings.is_empty() {
            return Ok(false);
        }

        let substrings = substrings
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<Vec<_>>();
        let predicates = std::iter::repeat_n("instr(lower(content), ?) > 0", substrings.len())
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT EXISTS(
                 SELECT 1
                 FROM agent_messages
                 WHERE thread_id = ?
                   AND deleted_at IS NULL
                   AND ({predicates})
                 LIMIT 1
             )"
        );
        let mut values = vec![db::Value::Text(thread_id.to_string())];
        values.extend(substrings.into_iter().map(db::Value::Text));
        let row = self
            .interactive_read_db
            .query_opt(&sql, db::Params::Positional(values))
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub(crate) async fn latest_non_empty_message_content_for_thread_ids(
        &self,
        thread_ids: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        if thread_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let chunk_size = sqlite_in_chunk_size(1);
        let mut messages = std::collections::HashMap::new();
        for chunk in thread_ids.chunks(chunk_size) {
            let placeholders = std::iter::repeat("?")
                .take(chunk.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "WITH ranked_messages AS (
                    SELECT
                        message.thread_id,
                        message.content,
                        ROW_NUMBER() OVER (
                            PARTITION BY message.thread_id
                            ORDER BY message.created_at DESC, message.rowid DESC
                        ) AS row_number
                    FROM agent_messages message
                    JOIN agent_threads thread
                      ON thread.id = message.thread_id
                     AND thread.deleted_at IS NULL
                    WHERE message.thread_id IN ({placeholders})
                      AND message.deleted_at IS NULL
                      AND TRIM(
                        message.content,
                        char(9) || char(10) || char(11) || char(12) || char(13) || char(32)
                      ) != ''
                 )
                 SELECT thread_id, content
                 FROM ranked_messages
                 WHERE row_number = 1"
            );
            let values = chunk
                .iter()
                .cloned()
                .map(db::Value::Text)
                .collect::<Vec<_>>();
            let rows = self
                .read_db
                .query(&sql, db::Params::Positional(values))
                .await?;
            for row in rows.iter() {
                messages.insert(row.get::<String>(0)?, row.get::<String>(1)?);
            }
        }
        Ok(messages)
    }

    pub(crate) async fn gateway_approval_ids_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<String>> {
        let whitespace = "char(9) || char(10) || char(11) || char(12) || char(13) || char(32)";
        let sql = format!(
            "WITH matching_messages AS (
                        SELECT
                            message.created_at,
                            message.rowid,
                            TRIM(
                                substr(
                                    message.content,
                                    instr(message.content, 'Approval ID:') + length('Approval ID:')
                                ),
                                {whitespace}
                            ) AS remainder
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND instr(message.content, 'Approval ID:') > 0
                     ),
                     normalized_messages AS (
                        SELECT
                            created_at,
                            rowid,
                            replace(
                                replace(
                                    replace(
                                        replace(
                                            replace(remainder, char(9), ' '),
                                            char(10),
                                            ' '
                                        ),
                                        char(11),
                                        ' '
                                    ),
                                    char(12),
                                    ' '
                                ),
                                char(13),
                                ' '
                            ) AS remainder
                        FROM matching_messages
                        WHERE remainder != ''
                     )
                     SELECT
                        CASE
                            WHEN instr(remainder, ' ') > 0
                            THEN substr(remainder, 1, instr(remainder, ' ') - 1)
                            ELSE remainder
                        END AS approval_id
                     FROM normalized_messages
                     WHERE approval_id != ''
                     ORDER BY created_at DESC, rowid DESC"
        );
        let rows = self.read_db.query(&sql, db::db_params![thread_id]).await?;
        rows.iter().map(|row| row.get::<String>(0)).collect()
    }

    pub(crate) async fn gateway_turn_auto_send_projection(
        &self,
        thread_id: &str,
    ) -> Result<Option<GatewayTurnAutoSendProjection>> {
        let row = self
            .read_db
            .query_opt(
                "WITH turn_start AS (
                        SELECT message.created_at, message.rowid AS row_id
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND message.role = 'user'
                        ORDER BY message.created_at DESC, message.rowid DESC
                        LIMIT 1
                     ),
                     turn_messages AS (
                        SELECT message.*, message.rowid AS row_id
                        FROM agent_messages message
                        JOIN agent_threads thread
                          ON thread.id = message.thread_id
                         AND thread.deleted_at IS NULL
                        JOIN turn_start start
                        WHERE message.thread_id = ?1
                          AND message.deleted_at IS NULL
                          AND (
                            message.created_at > start.created_at
                            OR (
                                message.created_at = start.created_at
                                AND message.rowid >= start.row_id
                            )
                          )
                     ),
                     latest_assistant AS (
                        SELECT
                            message.created_at,
                            message.row_id,
                            message.tool_calls_json
                        FROM turn_messages message
                        WHERE message.role = 'assistant'
                          AND (
                            message.content != ''
                            OR (
                                message.tool_calls_json IS NOT NULL
                                AND json_valid(message.tool_calls_json)
                                AND json_array_length(message.tool_calls_json) > 0
                            )
                          )
                        ORDER BY message.created_at DESC, message.row_id DESC
                        LIMIT 1
                     ),
                     latest_response AS (
                        SELECT message.content
                        FROM turn_messages message
                        WHERE message.role = 'assistant'
                          AND message.content != ''
                        ORDER BY message.created_at DESC, message.row_id DESC
                        LIMIT 1
                     ),
                     assistant_send_call AS (
                        SELECT 1
                        FROM latest_assistant assistant,
                             json_each(
                                CASE
                                    WHEN assistant.tool_calls_json IS NOT NULL
                                     AND json_valid(assistant.tool_calls_json)
                                    THEN assistant.tool_calls_json
                                    ELSE '[]'
                                END
                             ) call
                        WHERE substr(
                            COALESCE(json_extract(call.value, '$.function.name'), ''),
                            1,
                            5
                        ) = 'send_'
                        LIMIT 1
                     ),
                     send_tool_after_latest_assistant AS (
                        SELECT 1
                        FROM turn_messages message
                        JOIN latest_assistant assistant
                        WHERE message.role = 'tool'
                          AND (
                            message.created_at > assistant.created_at
                            OR (
                                message.created_at = assistant.created_at
                                AND message.row_id > assistant.row_id
                            )
                          )
                          AND substr(
                            COALESCE(
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.tool_name'
                                ),
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.toolName'
                                ),
                                ''
                            ),
                            1,
                            5
                          ) = 'send_'
                        LIMIT 1
                     ),
                     send_tool_without_assistant AS (
                        SELECT 1
                        FROM turn_messages message
                        WHERE NOT EXISTS (SELECT 1 FROM latest_assistant)
                          AND message.role = 'tool'
                          AND substr(
                            COALESCE(
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.tool_name'
                                ),
                                json_extract(
                                    CASE
                                        WHEN message.metadata_json IS NOT NULL
                                         AND json_valid(message.metadata_json)
                                        THEN message.metadata_json
                                        ELSE '{}'
                                    END,
                                    '$.toolName'
                                ),
                                ''
                            ),
                            1,
                            5
                          ) = 'send_'
                        LIMIT 1
                     )
                     SELECT
                        CASE
                            WHEN EXISTS (SELECT 1 FROM assistant_send_call)
                              OR EXISTS (SELECT 1 FROM send_tool_after_latest_assistant)
                              OR EXISTS (SELECT 1 FROM send_tool_without_assistant)
                            THEN 1
                            ELSE 0
                        END AS used_send_tool,
                        (SELECT content FROM latest_response) AS latest_assistant_response
                     WHERE EXISTS (SELECT 1 FROM turn_start)",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<GatewayTurnAutoSendProjection> {
            Ok(GatewayTurnAutoSendProjection {
                used_send_tool: row.get::<i64>(0)? != 0,
                latest_assistant_response: row.get(1)?,
            })
        })
        .transpose()
    }

    pub(crate) async fn thread_message_count(&self, thread_id: &str) -> Result<Option<usize>> {
        let count: Option<i64> = self
            .read_db
            .query_opt(
                "SELECT CASE
                     WHEN EXISTS(
                         SELECT 1 FROM agent_threads
                         WHERE id = ?1 AND deleted_at IS NULL
                     )
                     THEN (
                         SELECT COUNT(*) FROM agent_messages
                         WHERE thread_id = ?1 AND deleted_at IS NULL
                     )
                     ELSE NULL
                 END",
                db::db_params![thread_id],
            )
            .await?
            .map(|row| row.get::<Option<i64>>(0))
            .transpose()?
            .flatten();
        Ok(count.map(|value| value.max(0) as usize))
    }

    pub async fn update_thread_title(&self, id: &str, title: &str, updated_at: i64) -> Result<()> {
        let id_owned = id.to_string();
        let title_owned = title.to_string();
        self.conn_db
            .execute(
                "UPDATE agent_threads SET title = ?2, updated_at = ?3 WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id_owned, title_owned, updated_at],
            )
            .await?;
        Ok(())
    }

    pub async fn update_thread_metadata_json(
        &self,
        id: &str,
        metadata_json: Option<String>,
    ) -> Result<bool> {
        let id_owned = id.to_string();
        self.caches.thread_metadata_json.invalidate(&id_owned);
        let affected = self
            .conn_db
            .execute(
                "UPDATE agent_threads SET metadata_json = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![id_owned, metadata_json],
            )
            .await?;
        Ok(affected > 0)
    }

    pub async fn add_message(&self, message: &AgentDbMessage) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        txn.execute(
            "INSERT OR REPLACE INTO agent_messages \
             (id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, metadata_json, deleted_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
            db::db_params![
                message.id.clone(),
                message.thread_id.clone(),
                message.created_at,
                message.role.clone(),
                message.content.clone(),
                message.provider.clone(),
                message.model.clone(),
                message.input_tokens,
                message.output_tokens,
                message.total_tokens,
                message.cost_usd,
                message.reasoning.clone(),
                message.tool_calls_json.clone(),
                message.metadata_json.clone(),
            ],
        )
        .await?;
        let workspace_id = txn
            .query_opt(
                "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                db::db_params![message.thread_id.clone()],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?
            .flatten();
        embedding_queue::enqueue_message_embedding_job_exec(
            &mut *txn,
            message,
            workspace_id.as_deref(),
            now_ts() as i64,
        )
        .await?;
        refresh_thread_stats_exec(&mut *txn, &message.thread_id).await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn update_message(&self, id: &str, patch: &AgentMessagePatch) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        let thread_id: Option<String> = txn
            .query_opt(
                "SELECT thread_id FROM agent_messages WHERE id = ?1",
                db::db_params![id],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?
            .flatten();

        if thread_id.is_none() {
            txn.commit().await?;
            return Ok(());
        }
        let content_changed = patch.content.is_some();

        txn.execute(
            "UPDATE agent_messages SET
        content = COALESCE(?2, content),
        provider = COALESCE(?3, provider),
        model = COALESCE(?4, model),
        input_tokens = COALESCE(?5, input_tokens),
        output_tokens = COALESCE(?6, output_tokens),
        total_tokens = COALESCE(?7, total_tokens),
        cost_usd = COALESCE(?8, cost_usd),
        reasoning = COALESCE(?9, reasoning),
        tool_calls_json = COALESCE(?10, tool_calls_json),
        metadata_json = COALESCE(?11, metadata_json)
     WHERE id = ?1",
            db::db_params![
                id,
                patch.content.as_deref().map(str::to_string),
                flatten_option_str(&patch.provider).map(str::to_string),
                flatten_option_str(&patch.model).map(str::to_string),
                flatten_option_i64(&patch.input_tokens),
                flatten_option_i64(&patch.output_tokens),
                flatten_option_i64(&patch.total_tokens),
                flatten_option_f64(&patch.cost_usd),
                flatten_option_str(&patch.reasoning).map(str::to_string),
                flatten_option_str(&patch.tool_calls_json).map(str::to_string),
                flatten_option_str(&patch.metadata_json).map(str::to_string),
            ],
        )
        .await?;

        if let Some(thread_id) = thread_id {
            if content_changed {
                let message = txn
                    .query_opt(
                        "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                         FROM agent_messages WHERE id = ?1",
                        db::db_params![id],
                    )
                    .await?
                    .map(|row| map_agent_message_db(&row))
                    .transpose()?;
                if let Some(message) = message {
                    let workspace_id = txn
                        .query_opt(
                            "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                            db::db_params![thread_id.clone()],
                        )
                        .await?
                        .map(|row| row.get::<Option<String>>(0))
                        .transpose()?
                        .flatten();
                    embedding_queue::enqueue_message_embedding_job_exec(
                        &mut *txn,
                        &message,
                        workspace_id.as_deref(),
                        now_ts() as i64,
                    )
                    .await?;
                }
            }
            refresh_thread_stats_exec(&mut *txn, &thread_id).await?;
        }
        txn.commit().await?;
        Ok(())
    }

    /// Delete specific messages from a thread by their IDs.
    pub async fn delete_messages(&self, thread_id: &str, message_ids: &[&str]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let thread_id = thread_id.to_string();
        let message_ids: Vec<String> = message_ids.iter().map(|s| s.to_string()).collect();
        let placeholders: Vec<String> = message_ids.iter().map(|_| "?".to_string()).collect();

        let mut txn = self.conn_db.transaction().await?;
        let mut select_values = vec![db::Value::Text(thread_id.clone())];
        select_values.extend(message_ids.iter().cloned().map(db::Value::Text));
        let select_sql = format!(
            "SELECT id FROM agent_messages WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({})",
            placeholders.join(", ")
        );
        let deleted_ids = txn
            .query(&select_sql, db::Params::Positional(select_values))
            .await?
            .iter()
            .map(|row| row.get::<String>(0))
            .collect::<anyhow::Result<Vec<_>>>()?;
        if deleted_ids.is_empty() {
            txn.commit().await?;
            return Ok(0);
        }

        let update_sql = format!(
            "UPDATE agent_messages SET deleted_at = ? WHERE thread_id = ? AND deleted_at IS NULL AND id IN ({})",
            placeholders.join(", ")
        );
        let mut update_values = vec![
            db::Value::Integer(now_millis_i64()),
            db::Value::Text(thread_id.clone()),
        ];
        update_values.extend(message_ids.iter().cloned().map(db::Value::Text));
        txn.execute(&update_sql, db::Params::Positional(update_values))
            .await?;
        for id in &deleted_ids {
            embedding_queue::queue_embedding_deletion_exec(
                &mut *txn,
                "agent_message",
                id,
                now_ts() as i64,
            )
            .await?;
        }
        refresh_thread_stats_exec(&mut *txn, &thread_id).await?;
        txn.commit().await?;
        Ok(deleted_ids.len())
    }

    pub async fn restore_messages(&self, thread_id: &str, message_ids: &[&str]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let thread_id = thread_id.to_string();
        let message_ids: Vec<String> = message_ids.iter().map(|s| s.to_string()).collect();
        let placeholders: Vec<String> = message_ids.iter().map(|_| "?".to_string()).collect();
        let mut values = vec![db::Value::Text(thread_id.clone())];
        values.extend(message_ids.iter().cloned().map(db::Value::Text));

        let mut txn = self.conn_db.transaction().await?;
        let select_sql = format!(
            "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
             FROM agent_messages WHERE thread_id = ? AND deleted_at IS NOT NULL AND id IN ({})",
            placeholders.join(", ")
        );
        let restored_messages = txn
            .query(&select_sql, db::Params::Positional(values.clone()))
            .await?
            .iter()
            .filter_map(|row| map_agent_message_db(row).ok())
            .collect::<Vec<_>>();
        if restored_messages.is_empty() {
            txn.commit().await?;
            return Ok(0);
        }

        let update_sql = format!(
            "UPDATE agent_messages SET deleted_at = NULL WHERE thread_id = ? AND deleted_at IS NOT NULL AND id IN ({})",
            placeholders.join(", ")
        );
        txn.execute(&update_sql, db::Params::Positional(values))
            .await?;
        let workspace_id = txn
            .query_opt(
                "SELECT workspace_id FROM agent_threads WHERE id = ?1",
                db::db_params![thread_id.clone()],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?
            .flatten();
        for message in &restored_messages {
            embedding_queue::enqueue_message_embedding_job_exec(
                &mut *txn,
                message,
                workspace_id.as_deref(),
                now_ts() as i64,
            )
            .await?;
        }
        refresh_thread_stats_exec(&mut *txn, &thread_id).await?;
        txn.commit().await?;
        Ok(restored_messages.len())
    }

    pub async fn list_messages(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.list_messages_with_deleted_flag(thread_id, limit, false)
            .await
    }

    pub async fn list_messages_with_deleted(
        &self,
        thread_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        self.list_messages_with_deleted_flag(thread_id, limit, true)
            .await
    }

    async fn list_messages_with_deleted_flag(
        &self,
        thread_id: &str,
        limit: Option<usize>,
        include_deleted: bool,
    ) -> Result<Vec<AgentDbMessage>> {
        let deleted_filter = if include_deleted {
            ""
        } else {
            " AND deleted_at IS NULL"
        };
        let messages = if let Some(limit) = limit {
            let limit = limit.max(1) as i64;
            let sql = format!(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages WHERE thread_id = ?1{deleted_filter} ORDER BY created_at DESC, rowid DESC LIMIT ?2",
            );
            let rows = self
                .interactive_read_db
                .query(&sql, db::db_params![thread_id, limit])
                .await?;
            let mut messages: Vec<AgentDbMessage> = rows
                .iter()
                .filter_map(|row| map_agent_message_db(row).ok())
                .collect();
            messages.reverse();
            messages
        } else {
            let sql = format!(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages WHERE thread_id = ?1{deleted_filter} ORDER BY created_at ASC, rowid ASC",
            );
            let rows = self
                .interactive_read_db
                .query(&sql, db::db_params![thread_id])
                .await?;
            rows.iter()
                .filter_map(|row| map_agent_message_db(row).ok())
                .collect()
        };
        Ok(messages)
    }

    pub async fn list_message_window_collapsing_tool_runs(
        &self,
        thread_id: &str,
        logical_limit: usize,
        offset_from_end: usize,
    ) -> Result<(Vec<AgentDbMessage>, usize, usize, usize)> {
        let total_count = self
            .interactive_read_db
            .query_opt(
                "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id],
            )
            .await?
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or(0)
            .max(0) as usize;
        let raw_end = total_count.saturating_sub(offset_from_end);
        if raw_end == 0 || logical_limit == 0 {
            return Ok((Vec::new(), total_count, raw_end, raw_end));
        }

        let rows = self
            .interactive_read_db
            .query(
                r#"WITH ordered AS (
                   SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json, rowid,
                          ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) AS raw_position,
                          LAG(role) OVER (ORDER BY created_at ASC, rowid ASC) AS previous_role
                   FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL
                 ), grouped AS (
                   SELECT *, SUM(CASE WHEN role = 'tool' AND previous_role = 'tool' THEN 0 ELSE 1 END)
                     OVER (ORDER BY raw_position ASC) AS logical_group
                   FROM ordered
                 ), selected_groups AS (
                   SELECT logical_group FROM grouped WHERE raw_position <= ?2
                   GROUP BY logical_group ORDER BY logical_group DESC LIMIT ?3
                 )
                 SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json, raw_position
                 FROM grouped WHERE raw_position <= ?2 AND logical_group IN (SELECT logical_group FROM selected_groups)
                 ORDER BY raw_position ASC"#,
                db::db_params![thread_id, raw_end as i64, logical_limit as i64],
            )
            .await?;
        let loaded_start = rows
            .first()
            .and_then(|row| row.get::<i64>(14).ok())
            .map(|position| position.saturating_sub(1) as usize)
            .unwrap_or(raw_end);
        let messages = rows
            .iter()
            .filter_map(|row| map_agent_message_db(row).ok())
            .collect();
        Ok((messages, total_count, loaded_start, raw_end))
    }

    pub async fn list_message_window(
        &self,
        thread_id: &str,
        limit: usize,
        offset_from_end: usize,
    ) -> Result<(Vec<AgentDbMessage>, usize, usize, usize)> {
        let thread_id = thread_id.to_string();
        let total_count = self
            .interactive_read_db
            .query_opt(
                "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id.as_str()],
            )
            .await?
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        let total_count = total_count.max(0) as usize;
        let end = total_count.saturating_sub(offset_from_end);
        let start = end.saturating_sub(limit);
        if start == end {
            return Ok((Vec::new(), total_count, start, end));
        }

        let rows = self
            .interactive_read_db
            .query(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, rowid ASC LIMIT ?2 OFFSET ?3",
                db::db_params![thread_id, end.saturating_sub(start) as i64, start as i64],
            )
            .await?;
        Ok((
            rows.iter()
                .filter_map(|row| map_agent_message_db(row).ok())
                .collect(),
            total_count,
            start,
            end,
        ))
    }

    /// Lean fetch for the concierge welcome: returns the thread's opening
    /// user message (first message with role='user') and the *trailing*
    /// `tail_limit` messages — but if the most recent compaction-artifact
    /// marker falls inside that tail window, the trailing slice starts at
    /// the compaction marker instead so we don't expose pre-compaction
    /// content. Designed to read at most ~6 rows total even on threads
    /// with thousands of messages — no full-table scan, no JSON blob
    /// hydration.
    pub async fn concierge_thread_context_summary(
        &self,
        thread_id: &str,
        tail_limit: usize,
    ) -> Result<(Option<(String, String)>, Vec<(String, String)>)> {
        let thread_id = thread_id.to_string();
        let tail_limit = tail_limit.max(1);
        {
            let tail_rows = self
                .interactive_read_db
                .query(
                    "SELECT role, substr(content, 1, 600), \
                            CASE WHEN metadata_json IS NOT NULL \
                                      AND json_valid(metadata_json) \
                                      AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact' \
                                 THEN 1 ELSE 0 END \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL \
                         ORDER BY created_at DESC, id DESC \
                         LIMIT ?2",
                    db::db_params![thread_id.as_str(), tail_limit as i64],
                )
                .await?;
            let mut tail_desc: Vec<(String, String, bool)> = tail_rows
                .iter()
                .filter_map(|row| {
                    match (
                        row.get::<String>(0),
                        row.get::<String>(1),
                        row.get::<i64>(2),
                    ) {
                        (Ok(role), Ok(content), Ok(kind)) => Some((role, content, kind != 0)),
                        _ => None,
                    }
                })
                .collect();

            let compaction_idx_in_tail = tail_desc.iter().position(|(_, content, is_artifact)| {
                *is_artifact
                    || content.starts_with("[Compacted earlier context]")
                    || content.starts_with("Pre-compaction context:")
            });
            if let Some(idx) = compaction_idx_in_tail {
                tail_desc.truncate(idx + 1);
            }
            tail_desc.reverse();
            let tail: Vec<(String, String)> = tail_desc
                .into_iter()
                .map(|(role, content, _)| (role, content))
                .collect();

            let opening = self
                .interactive_read_db
                .query_opt(
                    "SELECT role, substr(content, 1, 600) \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL AND role = 'user' \
                         ORDER BY created_at ASC, rowid ASC \
                         LIMIT 1",
                    db::db_params![thread_id.as_str()],
                )
                .await?
                .map(|row| -> anyhow::Result<(String, String)> {
                    Ok((row.get::<String>(0)?, row.get::<String>(1)?))
                })
                .transpose()?;

            Ok((opening, tail))
        }
    }

    pub async fn list_active_context_window(
        &self,
        thread_id: &str,
    ) -> Result<(Vec<AgentDbMessage>, usize, usize)> {
        let thread_id = thread_id.to_string();
        let total_count = self
            .interactive_read_db
            .query_opt(
                "SELECT COUNT(*) FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id.as_str()],
            )
            .await?
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        let total_count = total_count.max(0) as usize;

        let start = self
            .interactive_read_db
            .query_opt(
                "SELECT absolute_index FROM (
                    SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                           content,
                           metadata_json
                    FROM agent_messages
                    WHERE thread_id = ?1 AND deleted_at IS NULL
                 )
                 WHERE (
                    metadata_json IS NOT NULL
                    AND json_valid(metadata_json)
                    AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact'
                 )
                 OR content LIKE '[Compacted earlier context]%'
                 OR content LIKE 'Pre-compaction context:%'
                 ORDER BY absolute_index DESC
                 LIMIT 1",
                db::db_params![thread_id.as_str()],
            )
            .await?
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or(0)
            .max(0) as usize;

        if start >= total_count {
            return Ok((Vec::new(), total_count, total_count));
        }

        let rows = self
            .interactive_read_db
            .query(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, rowid ASC LIMIT ?2 OFFSET ?3",
                db::db_params![
                    thread_id,
                    total_count.saturating_sub(start) as i64,
                    start as i64
                ],
            )
            .await?;
        Ok((
            rows.iter()
                .filter_map(|row| map_agent_message_db(row).ok())
                .collect(),
            start,
            total_count,
        ))
    }

    pub async fn trim_thread_messages_to_recent_tail_by_prefix(
        &self,
        thread_id_prefix: &str,
        max_messages: usize,
    ) -> Result<usize> {
        let thread_id_prefix = thread_id_prefix.to_string();
        let max_messages = max_messages.max(1) as i64;
        let mut txn = self.conn_db.transaction().await?;
        let thread_ids = txn
            .query(
                "SELECT id
                 FROM agent_threads
                 WHERE deleted_at IS NULL
                   AND id LIKE ?1 || '%'
                   AND message_count > ?2",
                db::db_params![thread_id_prefix, max_messages],
            )
            .await?
            .iter()
            .map(|row| row.get::<String>(0))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut trimmed = 0usize;
        let deleted_at = now_millis_i64();
        for thread_id in thread_ids {
            let total_count = txn
                .query_opt(
                    "SELECT COUNT(*)
                     FROM agent_messages
                     WHERE thread_id = ?1 AND deleted_at IS NULL",
                    db::db_params![thread_id.as_str()],
                )
                .await?
                .map(|row| row.get::<i64>(0))
                .transpose()?
                .unwrap_or(0);
            if total_count <= max_messages {
                continue;
            }

            let tail_start = total_count.saturating_sub(max_messages);
            let latest_artifact_index = txn
                .query_opt(
                    "SELECT absolute_index
                     FROM (
                        SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                               content,
                               metadata_json
                        FROM agent_messages
                        WHERE thread_id = ?1 AND deleted_at IS NULL
                     )
                     WHERE (
                        metadata_json IS NOT NULL
                        AND json_valid(metadata_json)
                        AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact'
                     )
                     OR content LIKE '[Compacted earlier context]%'
                     OR content LIKE 'Pre-compaction context:%'
                     ORDER BY absolute_index DESC
                     LIMIT 1",
                    db::db_params![thread_id.as_str()],
                )
                .await?
                .map(|row| row.get::<i64>(0))
                .transpose()?;
            let retained_artifact_index = latest_artifact_index.filter(|index| *index < tail_start);
            let recent_start = if retained_artifact_index.is_some() {
                total_count.saturating_sub(max_messages.saturating_sub(1))
            } else {
                tail_start
            };

            let deleted_ids = txn
                .query(
                    "WITH ordered AS (
                        SELECT id,
                               ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index
                        FROM agent_messages
                        WHERE thread_id = ?1 AND deleted_at IS NULL
                     )
                     SELECT id
                     FROM ordered
                     WHERE absolute_index < ?2
                       AND (?3 IS NULL OR absolute_index != ?3)",
                    db::db_params![thread_id.as_str(), recent_start, retained_artifact_index],
                )
                .await?
                .iter()
                .map(|row| row.get::<String>(0))
                .collect::<anyhow::Result<Vec<_>>>()?;

            if deleted_ids.is_empty() {
                continue;
            }

            txn.execute(
                "WITH ordered AS (
                    SELECT id,
                           ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index
                    FROM agent_messages
                    WHERE thread_id = ?1 AND deleted_at IS NULL
                 )
                 UPDATE agent_messages
                 SET deleted_at = ?4
                 WHERE thread_id = ?1
                   AND deleted_at IS NULL
                   AND id IN (
                        SELECT id
                        FROM ordered
                        WHERE absolute_index < ?2
                          AND (?3 IS NULL OR absolute_index != ?3)
                   )",
                db::db_params![
                    thread_id.as_str(),
                    recent_start,
                    retained_artifact_index,
                    deleted_at
                ],
            )
            .await?;
            for message_id in &deleted_ids {
                embedding_queue::queue_embedding_deletion_exec(
                    &mut *txn,
                    "agent_message",
                    message_id,
                    now_ts() as i64,
                )
                .await?;
            }
            refresh_thread_stats_exec(&mut *txn, &thread_id).await?;
            txn.execute(
                "UPDATE agent_threads
                 SET updated_at = MAX(updated_at, ?2)
                 WHERE id = ?1",
                db::db_params![thread_id.as_str(), deleted_at],
            )
            .await?;
            trimmed = trimmed.saturating_add(1);
        }

        txn.commit().await?;
        Ok(trimmed)
    }

    pub async fn thread_message_token_totals(&self, thread_id: &str) -> Result<(u64, u64)> {
        let (input_tokens, output_tokens): (i64, i64) = self
            .interactive_read_db
            .query_opt(
                "SELECT
                    COALESCE(SUM(COALESCE(input_tokens, 0)), 0),
                    COALESCE(SUM(COALESCE(output_tokens, 0)), 0)
                 FROM agent_messages
                 WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id],
            )
            .await?
            .map(|row| -> anyhow::Result<(i64, i64)> {
                Ok((row.get::<i64>(0)?, row.get::<i64>(1)?))
            })
            .transpose()?
            .unwrap_or((0, 0));
        Ok((input_tokens.max(0) as u64, output_tokens.max(0) as u64))
    }

    pub async fn thread_message_cost_total(&self, thread_id: &str) -> Result<Option<f64>> {
        self.interactive_read_db
            .query_opt(
                "SELECT SUM(cost_usd)
                 FROM agent_messages
                 WHERE thread_id = ?1 AND deleted_at IS NULL",
                db::db_params![thread_id],
            )
            .await?
            .map(|row| row.get::<Option<f64>>(0))
            .transpose()
            .map(Option::flatten)
    }

    pub async fn list_pinned_messages_for_compaction(
        &self,
        thread_id: &str,
    ) -> Result<Vec<(usize, AgentDbMessage)>> {
        let thread_id = thread_id.to_string();
        let rows = self
            .interactive_read_db
            .query(
                "SELECT absolute_index, id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM (
                    SELECT ROW_NUMBER() OVER (ORDER BY created_at ASC, rowid ASC) - 1 AS absolute_index,
                           id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json
                    FROM agent_messages
                    WHERE thread_id = ?1 AND deleted_at IS NULL
                 )
                 WHERE metadata_json IS NOT NULL
                   AND json_valid(metadata_json)
                   AND (
                     json_extract(metadata_json, '$.pinned_for_compaction') = 1
                     OR json_extract(metadata_json, '$.pinnedForCompaction') = 1
                   )
                 ORDER BY absolute_index ASC",
                db::db_params![thread_id],
            )
            .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.get::<i64>(0)?.max(0) as usize,
                    AgentDbMessage {
                        id: row.get(1)?,
                        thread_id: row.get(2)?,
                        created_at: row.get(3)?,
                        role: row.get(4)?,
                        content: row.get(5)?,
                        provider: row.get(6)?,
                        model: row.get(7)?,
                        input_tokens: row.get(8)?,
                        output_tokens: row.get(9)?,
                        total_tokens: row.get(10)?,
                        cost_usd: row.get(11)?,
                        reasoning: row.get(12)?,
                        tool_calls_json: row.get(13)?,
                        metadata_json: row.get(14)?,
                    },
                ))
            })
            .collect()
    }

    pub async fn list_recent_messages(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<Vec<AgentDbMessage>> {
        let limit = limit.clamp(1, 1000) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                db::db_params![thread_id, limit],
            )
            .await?;
        let mut messages: Vec<AgentDbMessage> = rows
            .iter()
            .filter_map(|row| map_agent_message_db(row).ok())
            .collect();
        messages.reverse();
        Ok(messages)
    }

    pub async fn latest_user_message_content(&self, thread_id: &str) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT content \
                 FROM agent_messages \
                 WHERE thread_id = ?1 AND role = 'user' AND deleted_at IS NULL \
                 ORDER BY created_at DESC, rowid DESC \
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    pub async fn latest_recovery_prompt_content(&self, thread_id: &str) -> Result<Option<String>> {
        if let Some(content) = self.latest_user_message_content(thread_id).await? {
            if !content.trim().is_empty() {
                return Ok(Some(content));
            }
        }
        Ok(self
            .latest_non_empty_message_content_for_thread_ids(&[thread_id.to_string()])
            .await?
            .remove(thread_id)
            .filter(|content| !content.trim().is_empty()))
    }

    pub async fn latest_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<AgentDbMessage>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                 FROM agent_messages \
                 WHERE thread_id = ?1 \
                   AND role = 'assistant' \
                   AND deleted_at IS NULL \
                   AND TRIM(content) != '' \
                 ORDER BY created_at DESC, rowid DESC \
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| map_agent_message_db(&row)).transpose()
    }

    pub async fn latest_participant_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<(String, Option<String>, String)>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT \
                    COALESCE( \
                        json_extract(metadata_json, '$.author_agent_id'), \
                        json_extract(metadata_json, '$.authorAgentId') \
                    ) AS author_agent_id, \
                    COALESCE( \
                        json_extract(metadata_json, '$.author_agent_name'), \
                        json_extract(metadata_json, '$.authorAgentName') \
                    ) AS author_agent_name, \
                    TRIM(content) \
                 FROM agent_messages \
                 WHERE thread_id = ?1 \
                   AND role = 'assistant' \
                   AND deleted_at IS NULL \
                   AND TRIM(content) != '' \
                   AND metadata_json IS NOT NULL \
                   AND json_valid(metadata_json) \
                   AND COALESCE( \
                        json_extract(metadata_json, '$.author_agent_id'), \
                        json_extract(metadata_json, '$.authorAgentId') \
                   ) IS NOT NULL \
                 ORDER BY created_at DESC, rowid DESC \
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<(String, Option<String>, String)> {
            Ok((
                row.get::<String>(0)?,
                row.get::<Option<String>>(1)?,
                row.get::<String>(2)?,
            ))
        })
        .transpose()
    }

    pub async fn latest_visible_main_assistant_message_timestamp(
        &self,
        thread_id: &str,
        participant_agent_ids: &[String],
    ) -> Result<Option<u64>> {
        let thread_id = thread_id.to_string();
        let participant_agent_ids = participant_agent_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let (sql, values) = {
            let author_exclusion = if participant_agent_ids.is_empty() {
                String::new()
            } else {
                let placeholders = std::iter::repeat("?")
                    .take(participant_agent_ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    " AND ( \
                            COALESCE( \
                                json_extract(latest.valid_metadata_json, '$.author_agent_id'), \
                                json_extract(latest.valid_metadata_json, '$.authorAgentId') \
                            ) IS NULL \
                            OR lower(COALESCE( \
                                json_extract(latest.valid_metadata_json, '$.author_agent_id'), \
                                json_extract(latest.valid_metadata_json, '$.authorAgentId') \
                            )) NOT IN ({placeholders}) \
                        )"
                )
            };
            let sql = format!(
                    "SELECT latest.created_at \
                     FROM ( \
                        SELECT created_at, \
                               role, \
                               content, \
                               CASE \
                                   WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                   THEN metadata_json \
                                   ELSE '{{}}' \
                               END AS valid_metadata_json \
                        FROM agent_messages \
                        WHERE thread_id = ? \
                          AND deleted_at IS NULL \
                          AND role IN ('user', 'assistant') \
                          AND ( \
                              COALESCE( \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.tool_name' \
                                  ), \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.toolName' \
                                  ) \
                              ) IS NULL \
                              OR COALESCE( \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.tool_name' \
                                  ), \
                                  json_extract( \
                                      CASE \
                                          WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) \
                                          THEN metadata_json \
                                          ELSE '{{}}' \
                                      END, \
                                      '$.toolName' \
                                  ) \
                              ) != 'internal_delegate' \
                          ) \
                        ORDER BY created_at DESC, rowid DESC \
                        LIMIT 1 \
                     ) latest \
                     WHERE latest.role = 'assistant' \
                       AND TRIM(latest.content) != ''{author_exclusion}"
                );
            let mut values = vec![db::Value::Text(thread_id)];
            values.extend(participant_agent_ids.into_iter().map(db::Value::Text));
            (sql, values)
        };
        let row = self
            .read_db
            .query_opt(&sql, db::Params::Positional(values))
            .await?;
        Ok(row
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .map(|value| value.max(0) as u64))
    }

    pub(crate) async fn thread_user_pacing(
        &self,
        thread_id: &str,
        now_ms: u64,
        window_ms: u64,
        gap_sample_limit: usize,
    ) -> Result<ThreadUserPacing> {
        if gap_sample_limit == 0 {
            return Ok(ThreadUserPacing {
                recent_message_count: 0,
                avg_gap_secs: 0,
            });
        }

        let thread_id = thread_id.to_string();
        let cutoff_ms = now_ms.saturating_sub(window_ms).min(i64::MAX as u64) as i64;
        let gap_sample_limit = gap_sample_limit.min(i64::MAX as usize) as i64;

        let row = self
            .read_db
            .query_opt(
                "WITH recent_user_messages AS (
                    SELECT created_at
                    FROM agent_messages
                    WHERE thread_id = ?1
                      AND role = 'user'
                      AND deleted_at IS NULL
                    ORDER BY created_at DESC, rowid DESC
                    LIMIT ?3
                 ),
                 ordered_user_messages AS (
                    SELECT created_at
                    FROM recent_user_messages
                    ORDER BY created_at ASC
                 ),
                 user_gaps AS (
                    SELECT created_at - LAG(created_at) OVER (ORDER BY created_at ASC) AS gap_ms
                    FROM ordered_user_messages
                 )
                 SELECT
                    (SELECT COUNT(*)
                     FROM agent_messages
                     WHERE thread_id = ?1
                       AND role = 'user'
                       AND deleted_at IS NULL
                       AND created_at >= ?2),
                    COALESCE((SELECT CAST(AVG(gap_ms) AS INTEGER) FROM user_gaps WHERE gap_ms IS NOT NULL) / 1000, 0)",
                db::db_params![thread_id, cutoff_ms, gap_sample_limit],
            )
            .await?;
        Ok(match row {
            Some(row) => ThreadUserPacing {
                recent_message_count: row.get::<i64>(0)?.max(0) as u32,
                avg_gap_secs: row.get::<i64>(1)?.max(0) as u64,
            },
            None => ThreadUserPacing {
                recent_message_count: 0,
                avg_gap_secs: 0,
            },
        })
    }

    pub async fn visible_continuation_obsoleted_by_progress(
        &self,
        thread_id: &str,
        queued_at_ms: u64,
        target_agent_id: &str,
    ) -> Result<bool> {
        if queued_at_ms == 0 {
            return Ok(false);
        }
        let thread_id = thread_id.to_string();
        let queued_at_ms = queued_at_ms.min(i64::MAX as u64) as i64;
        let target_agent_id = target_agent_id.to_ascii_lowercase();
        let row = self
            .read_db
            .query_opt(
                "SELECT
                    EXISTS (
                        SELECT 1
                        FROM agent_messages tool
                        WHERE tool.thread_id = ?1
                          AND tool.role = 'tool'
                          AND tool.deleted_at IS NULL
                          AND tool.created_at >= ?2
                          AND TRIM(tool.content) != ''
                    )
                    AND EXISTS (
                        SELECT 1
                        FROM agent_messages assistant
                        WHERE assistant.thread_id = ?1
                          AND assistant.role = 'assistant'
                          AND assistant.deleted_at IS NULL
                          AND assistant.created_at >= ?2
                          AND TRIM(assistant.content) != ''
                          AND (
                            assistant.metadata_json IS NULL
                            OR NOT json_valid(assistant.metadata_json)
                            OR COALESCE(
                                json_extract(assistant.metadata_json, '$.author_agent_id'),
                                json_extract(assistant.metadata_json, '$.authorAgentId')
                            ) IS NULL
                            OR lower(COALESCE(
                                json_extract(assistant.metadata_json, '$.author_agent_id'),
                                json_extract(assistant.metadata_json, '$.authorAgentId')
                            )) = ?3
                          )
                    )",
                db::db_params![thread_id, queued_at_ms, target_agent_id],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub async fn latest_unanswered_assistant_message(
        &self,
        thread_id: &str,
    ) -> Result<Option<(String, u64)>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT assistant.content, assistant.created_at \
                 FROM agent_messages assistant \
                 WHERE assistant.thread_id = ?1 \
                   AND assistant.role = 'assistant' \
                   AND assistant.deleted_at IS NULL \
                   AND NOT EXISTS ( \
                       SELECT 1 \
                       FROM agent_messages user_message \
                       WHERE user_message.thread_id = assistant.thread_id \
                         AND user_message.role = 'user' \
                         AND user_message.deleted_at IS NULL \
                         AND user_message.created_at > assistant.created_at \
                   ) \
                 ORDER BY assistant.created_at DESC, assistant.rowid DESC \
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<(String, u64)> {
            Ok((row.get::<String>(0)?, row.get::<i64>(1)?.max(0) as u64))
        })
        .transpose()
    }

    pub(crate) async fn thread_message_pin_state(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<Option<ThreadMessagePinState>> {
        let row = self
            .read_db
            .query_opt(
                    "SELECT
                        length(candidate.content),
                        CASE
                            WHEN candidate.metadata_json IS NOT NULL
                             AND json_valid(candidate.metadata_json)
                             AND (
                                json_extract(candidate.metadata_json, '$.pinned_for_compaction') = 1
                                OR json_extract(candidate.metadata_json, '$.pinnedForCompaction') = 1
                             )
                            THEN 1
                            ELSE 0
                        END,
                        CASE
                            WHEN (
                                candidate.metadata_json IS NOT NULL
                                AND json_valid(candidate.metadata_json)
                                AND json_extract(candidate.metadata_json, '$.message_kind') = 'compaction_artifact'
                            )
                            OR candidate.content LIKE '[Compacted earlier context]%'
                            OR candidate.content LIKE 'Pre-compaction context:%'
                            THEN 0
                            ELSE 1
                        END,
                        COALESCE((
                            SELECT SUM(length(pinned.content))
                            FROM agent_messages pinned
                            WHERE pinned.thread_id = ?1
                              AND pinned.deleted_at IS NULL
                              AND pinned.metadata_json IS NOT NULL
                              AND json_valid(pinned.metadata_json)
                              AND (
                                json_extract(pinned.metadata_json, '$.pinned_for_compaction') = 1
                                OR json_extract(pinned.metadata_json, '$.pinnedForCompaction') = 1
                              )
                              AND NOT (
                                json_extract(pinned.metadata_json, '$.message_kind') = 'compaction_artifact'
                                OR pinned.content LIKE '[Compacted earlier context]%'
                                OR pinned.content LIKE 'Pre-compaction context:%'
                              )
                        ), 0)
                     FROM agent_messages candidate
                     WHERE candidate.thread_id = ?1
                       AND candidate.id = ?2
                       AND candidate.deleted_at IS NULL",
                db::db_params![thread_id, message_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<ThreadMessagePinState> {
            Ok(ThreadMessagePinState {
                message_chars: row.get::<i64>(0)?.max(0) as usize,
                pinned_for_compaction: row.get::<i64>(1)? != 0,
                counts_toward_pinned_chars: row.get::<i64>(2)? != 0,
                current_pinned_chars: row.get::<i64>(3)?.max(0) as usize,
            })
        })
        .transpose()
    }

    /// Returns the signed mean of recent `user_feedback` behavioral-event
    /// signals on the thread. The window is the union of the most recent
    /// 20 events and all events from the last 30 minutes — whichever yields
    /// more rows, both feed in. Returns `None` if the window is empty.
    pub(crate) async fn aggregate_user_feedback_score(
        &self,
        thread_id: &str,
    ) -> Result<Option<f64>> {
        const RECENT_EVENT_COUNT: usize = 20;
        const TIME_WINDOW_MS: i64 = 30 * 60 * 1000;
        const HARD_FETCH_LIMIT: i64 = 200;

        let thread_id = thread_id.to_string();
        let cutoff = now_millis_i64().saturating_sub(TIME_WINDOW_MS);
        let rows_data = self
            .read_db
            .query(
                "SELECT payload_json, timestamp
                 FROM agent_events
                 WHERE category = 'behavioral'
                   AND kind = 'user_feedback'
                   AND pane_id = ?1
                 ORDER BY timestamp DESC
                 LIMIT ?2",
                db::db_params![thread_id, HARD_FETCH_LIMIT],
            )
            .await?;
        let rows: Vec<(String, i64)> = rows_data
            .iter()
            .filter_map(|row| match (row.get::<String>(0), row.get::<i64>(1)) {
                (Ok(a), Ok(b)) => Some((a, b)),
                _ => None,
            })
            .collect();

        let mut signals: Vec<f64> = Vec::new();
        for (idx, (payload_json, timestamp)) in rows.iter().enumerate() {
            let in_window = idx < RECENT_EVENT_COUNT || *timestamp >= cutoff;
            if !in_window {
                break;
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload_json.as_str()) {
                // Behavioral payload wraps the recorded payload in a
                // `payload` key; signal lives at `$.payload.signal`.
                if let Some(signal) = value
                    .get("payload")
                    .and_then(|p| p.get("signal"))
                    .and_then(|s| s.as_f64())
                {
                    signals.push(signal);
                }
            }
        }
        if signals.is_empty() {
            Ok(None)
        } else {
            let mean = signals.iter().sum::<f64>() / signals.len() as f64;
            Ok(Some(mean.clamp(-1.0, 1.0)))
        }
    }

    pub(crate) async fn message_feedback_state(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> Result<Option<MessageFeedbackState>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT
                    role,
                    CASE
                        WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                        THEN COALESCE(
                            json_extract(metadata_json, '$.feedback'),
                            json_extract(metadata_json, '$.feedback.reaction')
                        )
                        ELSE NULL
                    END
                 FROM agent_messages
                 WHERE thread_id = ?1
                   AND id = ?2
                   AND deleted_at IS NULL",
                db::db_params![thread_id, message_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<MessageFeedbackState> {
            Ok(MessageFeedbackState {
                role: row.get::<String>(0)?,
                reaction: row.get::<Option<String>>(1)?,
            })
        })
        .transpose()
    }

    pub(crate) async fn message_id_at_absolute_index(
        &self,
        thread_id: &str,
        absolute_index: usize,
    ) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT id
                 FROM agent_messages
                 WHERE thread_id = ?1
                   AND deleted_at IS NULL
                 ORDER BY created_at ASC, rowid ASC
                 LIMIT 1 OFFSET ?2",
                db::db_params![thread_id, absolute_index as i64],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    pub(crate) async fn set_message_feedback(
        &self,
        thread_id: &str,
        message_id: &str,
        reaction: Option<&str>,
    ) -> Result<bool> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        let reaction = reaction.map(ToOwned::to_owned);
        let mut txn = self.conn_db.transaction().await?;
        let updated = match reaction.as_deref() {
            Some(value) => {
                txn.execute(
                    "UPDATE agent_messages
                     SET metadata_json = json_set(
                        CASE
                            WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                            THEN metadata_json
                            ELSE '{}'
                        END,
                        '$.feedback', ?3
                     )
                     WHERE thread_id = ?1
                       AND id = ?2
                       AND deleted_at IS NULL",
                    db::db_params![thread_id.as_str(), message_id, value],
                )
                .await?
            }
            None => {
                txn.execute(
                    "UPDATE agent_messages
                     SET metadata_json = CASE
                        WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                        THEN json_remove(metadata_json, '$.feedback')
                        ELSE metadata_json
                     END
                     WHERE thread_id = ?1
                       AND id = ?2
                       AND deleted_at IS NULL",
                    db::db_params![thread_id.as_str(), message_id],
                )
                .await?
            }
        };

        if updated > 0 {
            txn.execute(
                "UPDATE agent_threads
                 SET updated_at = MAX(updated_at, ?2)
                 WHERE id = ?1",
                db::db_params![thread_id.as_str(), now_millis_i64()],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(updated > 0)
    }

    pub(crate) async fn set_message_pinned_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> Result<bool> {
        let thread_id = thread_id.to_string();
        let message_id = message_id.to_string();
        let pinned_json = if pinned { "true" } else { "false" }.to_string();
        let mut txn = self.conn_db.transaction().await?;
        let updated = txn
            .execute(
                "UPDATE agent_messages
                 SET metadata_json = json_set(
                    CASE
                        WHEN metadata_json IS NOT NULL AND json_valid(metadata_json)
                        THEN metadata_json
                        ELSE '{}'
                    END,
                    '$.pinned_for_compaction', json(?3),
                    '$.pinnedForCompaction', json(?3)
                 )
                 WHERE thread_id = ?1
                   AND id = ?2
                   AND deleted_at IS NULL",
                db::db_params![thread_id.as_str(), message_id, pinned_json],
            )
            .await?;

        if updated > 0 {
            txn.execute(
                "UPDATE agent_threads
                 SET updated_at = MAX(updated_at, ?2)
                 WHERE id = ?1",
                db::db_params![thread_id.as_str(), now_millis_i64()],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(updated > 0)
    }

    pub async fn list_messages_after_cursor(
        &self,
        thread_id: &str,
        after: Option<&AgentMessageCursor>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentDbMessage>> {
        let thread_id = thread_id.to_string();
        let after = after.cloned();
        let rows = match (after.as_ref(), limit) {
            (Some(cursor), Some(limit)) => {
                self.read_db
                    .query(
                        "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
                         ORDER BY created_at ASC, id ASC LIMIT ?4",
                        db::db_params![thread_id, cursor.created_at, cursor.message_id.clone(), limit.max(1) as i64],
                    )
                    .await?
            }
            (Some(cursor), None) => {
                self.read_db
                    .query(
                        "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                         FROM agent_messages \
                         WHERE thread_id = ?1 AND deleted_at IS NULL AND (created_at > ?2 OR (created_at = ?2 AND id > ?3)) \
                         ORDER BY created_at ASC, id ASC",
                        db::db_params![thread_id, cursor.created_at, cursor.message_id.clone()],
                    )
                    .await?
            }
            (None, Some(limit)) => {
                self.read_db
                    .query(
                        "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                         FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, id ASC LIMIT ?2",
                        db::db_params![thread_id, limit.max(1) as i64],
                    )
                    .await?
            }
            (None, None) => {
                self.read_db
                    .query(
                        "SELECT id, thread_id, created_at, role, content, provider, model, input_tokens, output_tokens, total_tokens, cost_usd, reasoning, tool_calls_json, CASE WHEN metadata_json IS NULL OR json_valid(metadata_json) = 0 THEN metadata_json ELSE json_remove(metadata_json, '$.provider_final_result.response_json', '$.provider_final_result.tool_calls', '$.providerFinalResult.response_json', '$.providerFinalResult.responseJson', '$.providerFinalResult.tool_calls', '$.providerFinalResult.toolCalls') END AS metadata_json \
                         FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL ORDER BY created_at ASC, id ASC",
                        db::db_params![thread_id],
                    )
                    .await?
            }
        };
        Ok(rows
            .iter()
            .filter_map(|row| map_agent_message_db(row).ok())
            .collect())
    }

    pub async fn get_memory_distillation_progress(
        &self,
        source_thread_id: &str,
    ) -> Result<Option<MemoryDistillationProgressRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id \
                 FROM memory_distillation_progress WHERE source_thread_id = ?1",
                db::db_params![source_thread_id],
            )
            .await?;
        row.map(|row| map_memory_distillation_progress_row_db(&row))
            .transpose()
    }

    pub async fn upsert_memory_distillation_progress(
        &self,
        progress: &MemoryDistillationProgressRow,
    ) -> Result<()> {
        let last_processed_span_json = progress
            .last_processed_span
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn_db
            .execute(
                "INSERT INTO memory_distillation_progress \
                 (source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(source_thread_id) DO UPDATE SET \
                    last_processed_created_at_ms = excluded.last_processed_created_at_ms, \
                    last_processed_message_id = excluded.last_processed_message_id, \
                    last_processed_span_json = excluded.last_processed_span_json, \
                    last_run_at_ms = excluded.last_run_at_ms, \
                    updated_at_ms = excluded.updated_at_ms, \
                    agent_id = excluded.agent_id",
                db::db_params![
                    progress.source_thread_id.clone(),
                    progress.last_processed_cursor.created_at,
                    progress.last_processed_cursor.message_id.clone(),
                    last_processed_span_json,
                    progress.last_run_at_ms,
                    progress.updated_at_ms,
                    progress.agent_id.clone(),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn append_memory_distillation_log(
        &self,
        source_thread_id: &str,
        source_message_range: Option<&str>,
        source_message_span: Option<&AgentMessageSpan>,
        distilled_fact: &str,
        target_file: &str,
        category: &str,
        confidence: f64,
        created_at_ms: i64,
        applied_to_memory: bool,
        agent_id: &str,
    ) -> Result<()> {
        let source_thread_id = source_thread_id.to_string();
        let source_message_range = source_message_range.map(str::to_string);
        let source_message_span_json =
            source_message_span.map(serde_json::to_string).transpose()?;
        let distilled_fact = distilled_fact.to_string();
        let target_file = target_file.to_string();
        let category = category.to_string();
        let applied_flag = if applied_to_memory { 1_i64 } else { 0_i64 };
        let agent_id = agent_id.to_string();

        self.conn_db
            .execute(
                "INSERT INTO memory_distillation_log \
                 (source_thread_id, source_message_range, source_message_span_json, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    source_thread_id,
                    source_message_range,
                    source_message_span_json,
                    distilled_fact,
                    target_file,
                    category,
                    confidence,
                    created_at_ms,
                    applied_flag,
                    agent_id,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_memory_distillation_log(
        &self,
        limit: usize,
    ) -> Result<Vec<MemoryDistillationLogRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT id, source_thread_id, source_message_range, source_message_span_json, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id \
                 FROM memory_distillation_log ORDER BY created_at_ms DESC, id DESC LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter()
            .map(map_memory_distillation_log_row_db)
            .collect()
    }

    pub async fn list_memory_distillation_progress(
        &self,
        limit: usize,
    ) -> Result<Vec<MemoryDistillationProgressRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT source_thread_id, last_processed_created_at_ms, last_processed_message_id, last_processed_span_json, last_run_at_ms, updated_at_ms, agent_id \
                 FROM memory_distillation_progress ORDER BY updated_at_ms DESC, source_thread_id ASC LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter()
            .map(map_memory_distillation_progress_row_db)
            .collect()
    }

    pub async fn list_forge_pass_log(&self, limit: usize) -> Result<Vec<ForgePassLogRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT id, agent_id, period_start_ms, period_end_ms, traces_analyzed, patterns_found, hints_applied, hints_logged, completed_at_ms \
                 FROM forge_pass_log ORDER BY completed_at_ms DESC, id DESC LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_forge_pass_log_row_db).collect()
    }

    pub async fn list_recent_handoff_routing(
        &self,
        limit: usize,
    ) -> Result<Vec<HandoffRoutingRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT id, to_specialist_id, capability_tags_json, routing_method, routing_score, fallback_used, created_at \
                 FROM handoff_log ORDER BY created_at DESC, id DESC LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_handoff_routing_row_db).collect()
    }

    pub async fn get_worm_chain_tip(&self, kind: &str) -> Result<Option<WormChainTip>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT kind, seq, hash FROM worm_chain_tip WHERE kind = ?1",
                db::db_params![kind],
            )
            .await?;
        row.map(|row| -> anyhow::Result<WormChainTip> {
            Ok(WormChainTip {
                kind: row.get(0)?,
                seq: row.get(1)?,
                hash: row.get(2)?,
            })
        })
        .transpose()
    }

    pub async fn set_worm_chain_tip(&self, kind: &str, seq: i64, hash: &str) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO worm_chain_tip (kind, seq, hash) VALUES (?1, ?2, ?3) \
             ON CONFLICT(kind) DO UPDATE SET seq = excluded.seq, hash = excluded.hash",
                db::db_params![kind, seq, hash],
            )
            .await?;
        Ok(())
    }

    pub async fn upsert_transcript_index(&self, entry: &TranscriptIndexEntry) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO transcript_index \
                 (id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                db::db_params![
                    entry.id.clone(),
                    entry.pane_id.clone(),
                    entry.workspace_id.clone(),
                    entry.surface_id.clone(),
                    entry.filename.clone(),
                    entry.reason.clone(),
                    entry.captured_at,
                    entry.size_bytes,
                    entry.preview.clone(),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_transcript_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        self.list_transcript_index_limited(workspace_id, None).await
    }

    pub async fn list_transcript_index_limited(
        &self,
        workspace_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        let mut sql = String::from(
            "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview FROM transcript_index",
        );
        let mut values = Vec::<db::Value>::new();
        if let Some(workspace_id) = workspace_id {
            sql.push_str(" WHERE workspace_id = ?");
            values.push(db::Value::Text(workspace_id));
        }
        sql.push_str(" ORDER BY captured_at DESC");
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?");
            values.push(db::Value::Integer(limit.max(1) as i64));
        }
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter()
            .filter_map(|row| map_transcript_index_entry_db(row).ok())
            .map(Ok)
            .collect()
    }

    pub async fn list_transcript_index_matching(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TranscriptIndexEntry>> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let like = format!("%{query}%");
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, pane_id, workspace_id, surface_id, filename, reason, captured_at, size_bytes, preview \
                 FROM transcript_index \
                 WHERE lower(filename) LIKE ?1 OR lower(COALESCE(preview, '')) LIKE ?1 \
                 ORDER BY captured_at DESC \
                 LIMIT ?2",
                db::db_params![like, limit],
            )
            .await?;
        rows.iter()
            .filter_map(|row| map_transcript_index_entry_db(row).ok())
            .map(Ok)
            .collect()
    }

    pub async fn upsert_snapshot_index(&self, entry: &SnapshotIndexEntry) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO snapshot_index \
                 (snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                db::db_params![
                    entry.snapshot_id.clone(),
                    entry.workspace_id.clone(),
                    entry.session_id.clone(),
                    entry.kind.clone(),
                    entry.label.clone(),
                    entry.path.clone(),
                    entry.created_at,
                    entry.details_json.clone(),
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_snapshot_index(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        self.list_snapshot_index_ordered(workspace_id, false).await
    }

    pub async fn list_snapshot_index_ordered(
        &self,
        workspace_id: Option<&str>,
        oldest_first: bool,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        self.list_snapshot_index_ordered_limited(workspace_id, oldest_first, None)
            .await
    }

    pub(crate) async fn list_snapshot_index_ordered_limited(
        &self,
        workspace_id: Option<&str>,
        oldest_first: bool,
        limit: Option<usize>,
    ) -> Result<Vec<SnapshotIndexEntry>> {
        let workspace_id = workspace_id.map(str::to_string);
        let order = if oldest_first { "ASC" } else { "DESC" };
        let mut sql = String::from(
            "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
             FROM snapshot_index WHERE ",
        );
        let mut values = Vec::<db::Value>::new();
        if let Some(workspace_id) = workspace_id {
            sql.push_str("(workspace_id = ? OR workspace_id IS NULL) AND deleted_at IS NULL");
            values.push(db::Value::Text(workspace_id));
        } else {
            sql.push_str("deleted_at IS NULL");
        }
        sql.push_str(&format!(" ORDER BY created_at {order}"));
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?");
            values.push(db::Value::Integer(limit.max(1) as i64));
        }
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter()
            .filter_map(|row| map_snapshot_index_entry_db(row).ok())
            .map(Ok)
            .collect()
    }

    pub async fn get_snapshot_index(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<SnapshotIndexEntry>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT snapshot_id, workspace_id, session_id, kind, label, path, created_at, details_json \
                 FROM snapshot_index WHERE snapshot_id = ?1 AND deleted_at IS NULL",
                db::db_params![snapshot_id],
            )
            .await?;
        row.map(|row| map_snapshot_index_entry_db(&row)).transpose()
    }

    pub async fn delete_snapshot_index(&self, snapshot_id: &str) -> Result<bool> {
        let affected = self
            .conn_db
            .execute(
                "UPDATE snapshot_index SET deleted_at = ?2 WHERE snapshot_id = ?1 AND deleted_at IS NULL",
                db::db_params![snapshot_id, now_ts() as i64],
            )
            .await?;
        Ok(affected > 0)
    }
}

/// Linear pass that mirrors the in-memory
/// `agent::stalled_turns::history_scan::thread_has_unanswered_tool_calls`
/// algorithm, but operates on the lean projection produced by
/// `thread_ids_with_unanswered_tool_calls`'s SQL.
///
/// Each tuple is `(role, tool_calls_json_for_assistants, tool_call_id_for_tools)`,
/// in the thread's chronological order. Returns true on the first assistant
/// message whose contiguous trailing run of tool replies doesn't satisfy
/// every tool-call id it declared.
fn scan_messages_for_unanswered_tool_calls(
    messages: &[(String, Option<String>, Option<String>)],
) -> bool {
    let mut idx = 0usize;
    while idx < messages.len() {
        let (role, tool_calls_json, _) = &messages[idx];
        if role != "assistant" {
            idx += 1;
            continue;
        }
        let Some(tool_calls_json) = tool_calls_json.as_deref() else {
            idx += 1;
            continue;
        };
        let calls: Vec<serde_json::Value> = match serde_json::from_str(tool_calls_json) {
            Ok(value) => value,
            Err(_) => {
                idx += 1;
                continue;
            }
        };
        let mut expected: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(calls.len());
        for call in &calls {
            if let Some(id) = call.get("id").and_then(|value| value.as_str()) {
                if !id.is_empty() {
                    expected.insert(id.to_string());
                }
            }
        }
        if expected.is_empty() {
            idx += 1;
            continue;
        }
        let mut matched: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(expected.len());
        let mut walk = idx + 1;
        while walk < messages.len() && messages[walk].0 == "tool" {
            if let Some(tool_call_id) = messages[walk].2.as_deref() {
                if expected.contains(tool_call_id) {
                    matched.insert(tool_call_id.to_string());
                }
            }
            walk += 1;
        }
        if matched.len() != expected.len() {
            return true;
        }
        idx = walk;
    }
    false
}
