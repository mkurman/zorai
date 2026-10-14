use super::*;

fn map_agent_wakeup_row(row: &db::Row) -> anyhow::Result<AgentWakeupRow> {
    Ok(AgentWakeupRow {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        message: row.get(2)?,
        interval_ms: row.get::<i64>(3)?.max(0) as u64,
        next_fire_at: row.get::<i64>(4)?.max(0) as u64,
        repetitions_remaining: row.get::<Option<i64>>(5)?.map(|value| value.max(0) as u64),
        created_at: row.get::<i64>(6)?.max(0) as u64,
        wakeup_kind: row.get(7)?,
        goal_run_id: row.get(8)?,
        reason: row.get(9)?,
    })
}

impl HistoryStore {
    pub async fn upsert_agent_wakeup(&self, row: &AgentWakeupRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO agent_wakeups (id, thread_id, message, interval_ms, next_fire_at, repetitions_remaining, created_at, wakeup_kind, goal_run_id, reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    row.id,
                    row.thread_id,
                    row.message,
                    row.interval_ms as i64,
                    row.next_fire_at as i64,
                    row.repetitions_remaining.map(|value| value as i64),
                    row.created_at as i64,
                    row.wakeup_kind,
                    row.goal_run_id,
                    row.reason,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_agent_wakeup(&self, id: &str) -> Result<bool> {
        let deleted = self
            .conn_db
            .execute(
                "DELETE FROM agent_wakeups WHERE id = ?1",
                db::db_params![id],
            )
            .await?;
        Ok(deleted > 0)
    }

    pub async fn delete_agent_wakeups_for_goal(&self, goal_run_id: &str) -> Result<u64> {
        self.conn_db
            .execute(
                "DELETE FROM agent_wakeups WHERE goal_run_id = ?1",
                db::db_params![goal_run_id],
            )
            .await
    }

    pub async fn list_agent_wakeups(&self) -> Result<Vec<AgentWakeupRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, thread_id, message, interval_ms, next_fire_at, repetitions_remaining, created_at, wakeup_kind, goal_run_id, reason FROM agent_wakeups ORDER BY next_fire_at ASC, id ASC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_agent_wakeup_row).collect()
    }
}
