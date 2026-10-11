use super::*;

fn map_whatsapp_provider_state_row(row: &db::Row) -> anyhow::Result<WhatsAppProviderStateRow> {
    Ok(WhatsAppProviderStateRow {
        provider_id: row.get(0)?,
        linked_phone: row.get(1)?,
        auth_json: row.get(2)?,
        metadata_json: row.get(3)?,
        last_reset_at: row.get::<Option<i64>>(4)?.map(|value| value as u64),
        last_linked_at: row.get::<Option<i64>>(5)?.map(|value| value as u64),
        updated_at: row.get::<i64>(6)? as u64,
    })
}

fn map_gateway_replay_cursor_row(row: &db::Row) -> anyhow::Result<GatewayReplayCursorRow> {
    Ok(GatewayReplayCursorRow {
        platform: row.get(0)?,
        channel_id: row.get(1)?,
        cursor_value: row.get(2)?,
        cursor_type: row.get(3)?,
        updated_at: row.get::<i64>(4)? as u64,
    })
}

fn map_gateway_health_snapshot_row(row: &db::Row) -> anyhow::Result<GatewayHealthSnapshotRow> {
    Ok(GatewayHealthSnapshotRow {
        platform: row.get(0)?,
        state_json: row.get(1)?,
        updated_at: row.get::<i64>(2)? as u64,
    })
}

fn map_operator_profile_session_row(row: &db::Row) -> anyhow::Result<OperatorProfileSessionRow> {
    Ok(OperatorProfileSessionRow {
        session_id: row.get(0)?,
        kind: row.get(1)?,
        session_json: row.get(2)?,
        updated_at: row.get::<i64>(3)? as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_gateway_thread_binding(
        &self,
        channel_key: &str,
        thread_id: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO gateway_threads (channel_key, thread_id, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                db::db_params![channel_key, thread_id, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_gateway_thread_binding(&self, channel_key: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE gateway_threads SET deleted_at = ?2 WHERE channel_key = ?1 AND deleted_at IS NULL",
                db::db_params![channel_key, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_gateway_thread_bindings(&self) -> Result<Vec<(String, String)>> {
        let rows = self
            .read_db
            .query(
                "SELECT channel_key, thread_id FROM gateway_threads WHERE deleted_at IS NULL ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .filter_map(|row| match (row.get::<String>(0), row.get::<String>(1)) {
                (Ok(a), Ok(b)) => Some((a, b)),
                _ => None,
            })
            .map(Ok)
            .collect()
    }

    pub async fn list_gateway_chat_history(&self) -> Result<Vec<(String, String, u64, bool)>> {
        let rows = self
            .read_db
            .query(
                "SELECT channel_key, thread_id, updated_at, deleted_at IS NULL FROM gateway_threads ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .filter_map(|row| {
                match (
                    row.get::<String>(0),
                    row.get::<String>(1),
                    row.get::<i64>(2),
                    row.get::<i64>(3),
                ) {
                    (Ok(key), Ok(thread_id), Ok(updated_at), Ok(active)) => {
                        Some((key, thread_id, updated_at.max(0) as u64, active != 0))
                    }
                    _ => None,
                }
            })
            .map(Ok)
            .collect()
    }

    pub async fn upsert_gateway_route_mode(
        &self,
        channel_key: &str,
        route_mode: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO gateway_channel_modes (channel_key, route_mode, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                db::db_params![channel_key, route_mode, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_gateway_route_mode(&self, channel_key: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE gateway_channel_modes SET deleted_at = ?2 WHERE channel_key = ?1 AND deleted_at IS NULL",
                db::db_params![channel_key, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_gateway_route_modes(&self) -> Result<Vec<(String, String)>> {
        let rows = self
            .read_db
            .query(
                "SELECT channel_key, route_mode FROM gateway_channel_modes WHERE deleted_at IS NULL ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .filter_map(|row| match (row.get::<String>(0), row.get::<String>(1)) {
                (Ok(a), Ok(b)) => Some((a, b)),
                _ => None,
            })
            .map(Ok)
            .collect()
    }

    pub async fn upsert_whatsapp_provider_state(
        &self,
        state: WhatsAppProviderStateRow,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO whatsapp_provider_state \
                 (provider_id, linked_phone, auth_json, metadata_json, last_reset_at, last_linked_at, updated_at, deleted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                db::db_params![
                    state.provider_id,
                    state.linked_phone,
                    state.auth_json,
                    state.metadata_json,
                    state.last_reset_at.map(|value| value as i64),
                    state.last_linked_at.map(|value| value as i64),
                    state.updated_at as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_whatsapp_provider_state(
        &self,
        provider_id: &str,
    ) -> Result<Option<WhatsAppProviderStateRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT provider_id, linked_phone, auth_json, metadata_json, last_reset_at, last_linked_at, updated_at \
                 FROM whatsapp_provider_state WHERE provider_id = ?1 AND deleted_at IS NULL",
                db::db_params![provider_id],
            )
            .await?;
        row.map(|row| map_whatsapp_provider_state_row(&row))
            .transpose()
    }

    pub async fn delete_whatsapp_provider_state(&self, provider_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE whatsapp_provider_state SET deleted_at = ?2 WHERE provider_id = ?1 AND deleted_at IS NULL",
                db::db_params![provider_id, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn save_gateway_replay_cursor(
        &self,
        platform: &str,
        channel_id: &str,
        cursor_value: &str,
        cursor_type: &str,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO gateway_replay_cursors (platform, channel_id, cursor_value, cursor_type, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                db::db_params![platform, channel_id, cursor_value, cursor_type, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn load_gateway_replay_cursor(
        &self,
        platform: &str,
        channel_id: &str,
    ) -> Result<Option<GatewayReplayCursorRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT platform, channel_id, cursor_value, cursor_type, updated_at FROM gateway_replay_cursors WHERE platform = ?1 AND channel_id = ?2",
                db::db_params![platform, channel_id],
            )
            .await?;
        row.map(|row| map_gateway_replay_cursor_row(&row))
            .transpose()
    }

    pub async fn load_gateway_replay_cursors(
        &self,
        platform: &str,
    ) -> Result<Vec<GatewayReplayCursorRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT platform, channel_id, cursor_value, cursor_type, updated_at FROM gateway_replay_cursors WHERE platform = ?1 ORDER BY updated_at DESC",
                db::db_params![platform],
            )
            .await?;
        rows.iter().map(map_gateway_replay_cursor_row).collect()
    }

    pub async fn upsert_gateway_health_snapshot(
        &self,
        state: &GatewayHealthState,
        updated_at: u64,
    ) -> Result<()> {
        let platform = state.platform.to_string();
        let state_json =
            serde_json::to_string(state).context("failed to serialize health state")?;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO gateway_health_snapshots (platform, state_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![platform, state_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_gateway_health_snapshots(&self) -> Result<Vec<GatewayHealthSnapshotRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT platform, state_json, updated_at FROM gateway_health_snapshots ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_gateway_health_snapshot_row).collect()
    }

    pub async fn upsert_operator_profile_session(
        &self,
        session_id: &str,
        kind: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO operator_profile_sessions (session_id, kind, session_json, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, NULL)",
                db::db_params![session_id, kind, session_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_operator_profile_session(&self, session_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE operator_profile_sessions SET deleted_at = ?2 WHERE session_id = ?1 AND deleted_at IS NULL",
                db::db_params![session_id, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_all_operator_profile_sessions(&self) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE operator_profile_sessions SET deleted_at = ?1 WHERE deleted_at IS NULL",
                db::db_params![now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_operator_profile_sessions(&self) -> Result<Vec<OperatorProfileSessionRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT session_id, kind, session_json, updated_at FROM operator_profile_sessions WHERE deleted_at IS NULL ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_operator_profile_session_row).collect()
    }
}
