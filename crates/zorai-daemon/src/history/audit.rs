use super::*;

fn map_heartbeat_history_row(row: &db::Row) -> anyhow::Result<HeartbeatHistoryRow> {
    Ok(HeartbeatHistoryRow {
        id: row.get(0)?,
        cycle_timestamp: row.get(1)?,
        checks_json: row.get(2)?,
        synthesis_json: row.get(3)?,
        actionable: row.get::<i32>(4)? != 0,
        digest_text: row.get(5)?,
        llm_tokens_used: row.get(6)?,
        duration_ms: row.get(7)?,
        status: row.get(8)?,
    })
}

fn map_audit_entry_row(row: &db::Row) -> anyhow::Result<AuditEntryRow> {
    Ok(AuditEntryRow {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        action_type: row.get(2)?,
        summary: row.get(3)?,
        explanation: row.get(4)?,
        confidence: row.get(5)?,
        confidence_band: row.get(6)?,
        causal_trace_id: row.get(7)?,
        thread_id: row.get(8)?,
        goal_run_id: row.get(9)?,
        task_id: row.get(10)?,
        raw_data_json: row.get(11)?,
    })
}

async fn count_action_audit_by_type(
    store: &HistoryStore,
    sql: &str,
    since: i64,
) -> Result<std::collections::HashMap<String, u64>> {
    let rows = store.conn_db.query(sql, db::db_params![since]).await?;
    rows.iter()
        .map(|row| -> anyhow::Result<(String, u64)> {
            Ok((row.get::<String>(0)?, row.get::<u64>(1)?))
        })
        .collect()
}

impl HistoryStore {
    pub async fn insert_heartbeat_history(
        &self,
        id: &str,
        cycle_timestamp: i64,
        checks_json: &str,
        synthesis_json: Option<&str>,
        actionable: bool,
        digest_text: Option<&str>,
        llm_tokens_used: i64,
        duration_ms: i64,
        status: &str,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO heartbeat_history \
                 (id, cycle_timestamp, checks_json, synthesis_json, actionable, digest_text, llm_tokens_used, duration_ms, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                db::db_params![
                    id, cycle_timestamp, checks_json, synthesis_json,
                    actionable as i32, digest_text, llm_tokens_used, duration_ms, status
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("insert_heartbeat_history: {e}"))?;
        Ok(())
    }

    /// List recent heartbeat history entries. Per D-12.
    pub async fn list_heartbeat_history(&self, limit: usize) -> Result<Vec<HeartbeatHistoryRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, cycle_timestamp, checks_json, synthesis_json, actionable, \
                 digest_text, llm_tokens_used, duration_ms, status \
                 FROM heartbeat_history ORDER BY cycle_timestamp DESC LIMIT ?1",
                db::db_params![limit as i64],
            )
            .await
            .map_err(|e| anyhow::anyhow!("list_heartbeat_history: {e}"))?;
        rows.iter().map(map_heartbeat_history_row).collect()
    }

    /// Insert or replace an action audit entry.
    pub async fn insert_action_audit(&self, entry: &AuditEntryRow) -> Result<()> {
        let entry = entry.clone();
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO action_audit \
                 (id, timestamp, action_type, summary, explanation, confidence, \
                  confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                db::db_params![
                    entry.id,
                    entry.timestamp,
                    entry.action_type,
                    entry.summary,
                    entry.explanation,
                    entry.confidence,
                    entry.confidence_band,
                    entry.causal_trace_id,
                    entry.thread_id,
                    entry.goal_run_id,
                    entry.task_id,
                    entry.raw_data_json,
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("insert_action_audit: {e}"))?;
        Ok(())
    }

    /// List action audit entries with optional filters.
    pub async fn list_action_audit(
        &self,
        action_types: Option<&[String]>,
        since: Option<i64>,
        limit: usize,
    ) -> Result<Vec<AuditEntryRow>> {
        let mut where_parts: Vec<String> = Vec::new();
        let mut values = Vec::<db::Value>::new();

        if let Some(types) = action_types {
            if !types.is_empty() {
                let placeholders: Vec<String> =
                    (0..types.len()).map(|i| format!("?{}", i + 1)).collect();
                where_parts.push(format!("action_type IN ({})", placeholders.join(",")));
                for action_type in types {
                    values.push(db::Value::Text(action_type.clone()));
                }
            }
        }
        let since_idx = values.len() + 1;
        if let Some(ts) = since {
            where_parts.push(format!("timestamp >= ?{since_idx}"));
            values.push(db::Value::Integer(ts));
        }
        let limit_idx = values.len() + 1;
        values.push(db::Value::Integer(limit as i64));

        let mut sql = String::from(
            "SELECT id, timestamp, action_type, summary, explanation, confidence, \
                 confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json \
                 FROM action_audit",
        );
        if !where_parts.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_parts.join(" AND "));
        }
        sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ?{limit_idx}"));

        let rows = self
            .conn_db
            .query(&sql, db::Params::Positional(values))
            .await
            .map_err(|e| anyhow::anyhow!("list_action_audit: {e}"))?;
        rows.iter().map(map_audit_entry_row).collect()
    }

    /// Delete oldest audit entries exceeding retention limits. Returns count deleted.
    pub async fn cleanup_action_audit(
        &self,
        max_entries: usize,
        max_age_days: u32,
    ) -> Result<usize> {
        let mut deleted = 0usize;
        if max_age_days > 0 {
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
                - (max_age_days as i64 * 86_400 * 1000);
            deleted += self
                .conn_db
                .execute(
                    "UPDATE action_audit SET deleted_at = ?2 WHERE timestamp < ?1 AND deleted_at IS NULL",
                    db::db_params![cutoff, now_ts() as i64],
                )
                .await
                .map_err(|e| anyhow::anyhow!("cleanup_action_audit: {e}"))? as usize;
        }
        if max_entries > 0 {
            deleted += self
                .conn_db
                .execute(
                    "UPDATE action_audit SET deleted_at = ?2 WHERE deleted_at IS NULL AND id NOT IN \
                     (SELECT id FROM action_audit WHERE deleted_at IS NULL ORDER BY timestamp DESC LIMIT ?1)",
                    db::db_params![max_entries as i64, now_ts() as i64],
                )
                .await
                .map_err(|e| anyhow::anyhow!("cleanup_action_audit: {e}"))? as usize;
        }
        Ok(deleted)
    }

    pub async fn prune_heartbeat_history_to_latest(&self) -> Result<usize> {
        let mut deleted = 0usize;
        loop {
            let batch = self
                .conn_db
                .execute(
                    "DELETE FROM heartbeat_history WHERE id IN \
                     (SELECT id FROM heartbeat_history \
                      WHERE cycle_timestamp < (SELECT MAX(cycle_timestamp) FROM heartbeat_history) \
                      LIMIT ?1)",
                    db::db_params![RETENTION_DELETE_BATCH],
                )
                .await
                .map_err(|e| anyhow::anyhow!("prune_heartbeat_history_to_latest: {e}"))?
                as usize;
            deleted += batch;
            if batch < RETENTION_DELETE_BATCH as usize {
                return Ok(deleted);
            }
        }
    }

    /// Mark an audit entry as dismissed by the user. Per BEAT-09/D-04.
    pub async fn dismiss_audit_entry(&self, entry_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE action_audit SET user_action = 'dismissed' WHERE id = ?1",
                db::db_params![entry_id],
            )
            .await
            .map_err(|e| anyhow::anyhow!("dismiss_audit_entry: {e}"))?;
        Ok(())
    }

    /// Count dismissed audit entries per action_type since a given timestamp (ms).
    pub async fn count_dismissals_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        count_action_audit_by_type(
            self,
            "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'dismissed' AND timestamp >= ?1 \
                 GROUP BY action_type",
            since_timestamp,
        )
        .await
        .map_err(|e| anyhow::anyhow!("count_dismissals_by_type: {e}"))
    }

    /// Count total audit entries per action_type since a given timestamp (ms).
    pub async fn count_shown_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        count_action_audit_by_type(
            self,
            "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE timestamp >= ?1 \
                 GROUP BY action_type",
            since_timestamp,
        )
        .await
        .map_err(|e| anyhow::anyhow!("count_shown_by_type: {e}"))
    }

    /// Count audit entries where user acted on them, per action_type since a given timestamp (ms).
    pub async fn count_acted_on_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        count_action_audit_by_type(
            self,
            "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'acted_on' AND timestamp >= ?1 \
                 GROUP BY action_type",
            since_timestamp,
        )
        .await
        .map_err(|e| anyhow::anyhow!("count_acted_on_by_type: {e}"))
    }
}

pub(crate) const RETENTION_DELETE_BATCH: i64 = 2_000;

pub(crate) fn retention_cutoff_ms(max_age_days: u32) -> i64 {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    now_ms - i64::from(max_age_days) * 86_400 * 1000
}
