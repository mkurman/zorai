use super::*;

fn map_trigger_fire_history_row(row: &db::Row) -> anyhow::Result<TriggerFireHistoryRow> {
    Ok(TriggerFireHistoryRow {
        id: row.get(0)?,
        trigger_id: row.get(1)?,
        event_family: row.get(2)?,
        event_kind: row.get(3)?,
        status: row.get(4)?,
        fired_at_ms: row.get::<i64>(5)?.max(0) as u64,
        completed_at_ms: row.get::<Option<i64>>(6)?.map(|value| value.max(0) as u64),
        retry_count: row.get::<i64>(7)?.max(0) as u64,
        error_message: row.get(8)?,
        created_task_id: row.get(9)?,
        notice_id: row.get(10)?,
        payload_json: row.get(11)?,
    })
}

fn map_event_trigger_row(row: &db::Row) -> anyhow::Result<EventTriggerRow> {
    Ok(EventTriggerRow {
        id: row.get(0)?,
        event_family: row.get(1)?,
        event_kind: row.get(2)?,
        agent_id: row.get(3)?,
        target_state: row.get(4)?,
        thread_id: row.get(5)?,
        enabled: row.get::<i64>(6)? != 0,
        cooldown_secs: row.get::<i64>(7)?.max(0) as u64,
        risk_label: row.get(8)?,
        notification_kind: row.get(9)?,
        prompt_template: row.get(10)?,
        tool_name: row.get(11)?,
        tool_payload_json: row.get(12)?,
        title_template: row.get(13)?,
        body_template: row.get(14)?,
        created_at: row.get::<i64>(15)?.max(0) as u64,
        updated_at: row.get::<i64>(16)?.max(0) as u64,
        last_fired_at: row.get::<Option<i64>>(17)?.map(|value| value.max(0) as u64),
        max_retries: row.get::<i64>(18)?.max(0) as u32,
    })
}

const EVENT_TRIGGER_COLUMNS: &str = "id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries";

const TRIGGER_FIRE_HISTORY_COLUMNS: &str = "id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json";

impl HistoryStore {
    pub async fn upsert_event_trigger(&self, row: &EventTriggerRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO event_triggers (id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                db::db_params![
                    row.id,
                    row.event_family,
                    row.event_kind,
                    row.agent_id,
                    row.target_state,
                    row.thread_id,
                    if row.enabled { 1i64 } else { 0i64 },
                    row.cooldown_secs as i64,
                    row.risk_label,
                    row.notification_kind,
                    row.prompt_template,
                    row.tool_name,
                    row.tool_payload_json,
                    row.title_template,
                    row.body_template,
                    row.created_at as i64,
                    row.updated_at as i64,
                    row.last_fired_at.map(|value| value as i64),
                    row.max_retries as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_event_triggers(
        &self,
        event_family: Option<&str>,
        event_kind: Option<&str>,
    ) -> Result<Vec<EventTriggerRow>> {
        let (sql, params) = match (event_family, event_kind) {
            (Some(event_family), Some(event_kind)) => (
                format!("SELECT {EVENT_TRIGGER_COLUMNS} FROM event_triggers WHERE event_family = ?1 AND event_kind = ?2 ORDER BY updated_at DESC, id ASC"),
                db::db_params![event_family, event_kind],
            ),
            (Some(event_family), None) => (
                format!("SELECT {EVENT_TRIGGER_COLUMNS} FROM event_triggers WHERE event_family = ?1 ORDER BY updated_at DESC, id ASC"),
                db::db_params![event_family],
            ),
            (None, Some(event_kind)) => (
                format!("SELECT {EVENT_TRIGGER_COLUMNS} FROM event_triggers WHERE event_kind = ?1 ORDER BY updated_at DESC, id ASC"),
                db::db_params![event_kind],
            ),
            (None, None) => (
                format!("SELECT {EVENT_TRIGGER_COLUMNS} FROM event_triggers ORDER BY updated_at DESC, id ASC"),
                db::Params::None,
            ),
        };
        let rows = self.read_db.query(&sql, params).await?;
        rows.iter().map(map_event_trigger_row).collect()
    }

    pub async fn list_event_triggers_for_fire(
        &self,
        event_family: &str,
        event_kind: &str,
        state: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<Vec<EventTriggerRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, event_family, event_kind, agent_id, target_state, thread_id, enabled, cooldown_secs, risk_label, notification_kind, prompt_template, tool_name, tool_payload_json, title_template, body_template, created_at, updated_at, last_fired_at, max_retries \
                     FROM event_triggers \
                     WHERE event_family = ?1 \
                       AND event_kind = ?2 \
                       AND enabled = 1 \
                       AND (target_state IS NULL OR target_state = ?3) \
                       AND (thread_id IS NULL OR thread_id = ?4) \
                     ORDER BY updated_at DESC, id ASC",
                db::db_params![event_family, event_kind, state, thread_id],
            )
            .await?;
        rows.iter().map(map_event_trigger_row).collect()
    }

    pub async fn record_event_trigger_fired(&self, id: &str, fired_at: u64) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE event_triggers SET last_fired_at = ?2, updated_at = CASE WHEN updated_at > ?2 THEN updated_at ELSE ?2 END WHERE id = ?1",
                db::db_params![id, fired_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn insert_trigger_fire_history(&self, row: &TriggerFireHistoryRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT INTO trigger_fire_history (id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                db::db_params![
                    row.id,
                    row.trigger_id,
                    row.event_family,
                    row.event_kind,
                    row.status,
                    row.fired_at_ms as i64,
                    row.completed_at_ms.map(|value| value as i64),
                    row.retry_count as i64,
                    row.error_message,
                    row.created_task_id,
                    row.notice_id,
                    row.payload_json,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn update_trigger_fire_status(
        &self,
        fire_id: &str,
        status: &str,
        error_message: Option<&str>,
        created_task_id: Option<&str>,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.conn_db
            .execute(
                "UPDATE trigger_fire_history SET status = ?2, completed_at_ms = ?3, error_message = ?4, created_task_id = COALESCE(?5, created_task_id) WHERE id = ?1",
                db::db_params![fire_id, status, now, error_message, created_task_id],
            )
            .await?;
        Ok(())
    }

    pub async fn list_trigger_fire_history(
        &self,
        trigger_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TriggerFireHistoryRow>> {
        let limit = limit as i64;
        let (sql, params) = match (trigger_id, status) {
            (Some(trigger_id), Some(status)) => (
                format!("SELECT {TRIGGER_FIRE_HISTORY_COLUMNS} FROM trigger_fire_history WHERE trigger_id = ?1 AND status = ?2 ORDER BY fired_at_ms DESC LIMIT ?3"),
                db::db_params![trigger_id, status, limit],
            ),
            (Some(trigger_id), None) => (
                format!("SELECT {TRIGGER_FIRE_HISTORY_COLUMNS} FROM trigger_fire_history WHERE trigger_id = ?1 ORDER BY fired_at_ms DESC LIMIT ?2"),
                db::db_params![trigger_id, limit],
            ),
            (None, Some(status)) => (
                format!("SELECT {TRIGGER_FIRE_HISTORY_COLUMNS} FROM trigger_fire_history WHERE status = ?1 ORDER BY fired_at_ms DESC LIMIT ?2"),
                db::db_params![status, limit],
            ),
            (None, None) => (
                format!("SELECT {TRIGGER_FIRE_HISTORY_COLUMNS} FROM trigger_fire_history ORDER BY fired_at_ms DESC LIMIT ?1"),
                db::db_params![limit],
            ),
        };
        let rows = self.read_db.query(&sql, params).await?;
        rows.iter().map(map_trigger_fire_history_row).collect()
    }

    pub async fn list_trigger_fire_history_since(
        &self,
        since_fired_at_ms: u64,
        limit: usize,
    ) -> Result<Vec<TriggerFireHistoryRow>> {
        let since_fired_at_ms = since_fired_at_ms.min(i64::MAX as u64) as i64;
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, trigger_id, event_family, event_kind, status, fired_at_ms, completed_at_ms, retry_count, error_message, created_task_id, notice_id, payload_json \
                     FROM trigger_fire_history \
                     WHERE fired_at_ms >= ?1 \
                     ORDER BY fired_at_ms DESC \
                     LIMIT ?2",
                db::db_params![since_fired_at_ms, limit],
            )
            .await?;
        rows.iter().map(map_trigger_fire_history_row).collect()
    }

    pub async fn prune_suppressed_trigger_fire_history(&self, max_age_days: u32) -> Result<usize> {
        let cutoff = super::audit::retention_cutoff_ms(max_age_days);
        let mut deleted = 0usize;
        loop {
            let batch = self
                .conn_db
                .execute(
                    "DELETE FROM trigger_fire_history WHERE id IN \
                     (SELECT id FROM trigger_fire_history \
                      WHERE status = 'suppressed' AND fired_at_ms < ?1 LIMIT ?2)",
                    db::db_params![cutoff, super::audit::RETENTION_DELETE_BATCH],
                )
                .await? as usize;
            deleted += batch;
            if batch < super::audit::RETENTION_DELETE_BATCH as usize {
                return Ok(deleted);
            }
        }
    }

    pub async fn count_recent_trigger_fire_history(
        &self,
        trigger_id: &str,
        status: &str,
        limit: usize,
    ) -> Result<u64> {
        let limit = limit.max(1) as i64;
        let row = self
            .read_db
            .query_opt(
                "SELECT COUNT(*) \
                     FROM ( \
                         SELECT 1 \
                         FROM trigger_fire_history \
                         WHERE trigger_id = ?1 AND status = ?2 \
                         ORDER BY fired_at_ms DESC, id DESC \
                         LIMIT ?3 \
                     )",
                db::db_params![trigger_id, status, limit],
            )
            .await?;
        let count: i64 = row.map(|row| row.get::<i64>(0)).transpose()?.unwrap_or(0);
        Ok(count.max(0) as u64)
    }
}
