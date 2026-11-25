use super::*;

fn map_collaboration_session_row(row: &db::Row) -> anyhow::Result<CollaborationSessionRow> {
    Ok(CollaborationSessionRow {
        parent_task_id: row.get(0)?,
        session_json: row.get(1)?,
        updated_at: row.get::<i64>(2)? as u64,
    })
}

fn map_operator_profile_field_row(row: &db::Row) -> anyhow::Result<OperatorProfileFieldRow> {
    Ok(OperatorProfileFieldRow {
        field_key: row.get(0)?,
        field_value_json: row.get(1)?,
        confidence: row.get(2)?,
        source: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn map_operator_profile_consent_row(row: &db::Row) -> anyhow::Result<OperatorProfileConsentRow> {
    Ok(OperatorProfileConsentRow {
        consent_key: row.get(0)?,
        granted: row.get::<i64>(1)? != 0,
        updated_at: row.get(2)?,
    })
}

fn map_operator_profile_event_row(row: &db::Row) -> anyhow::Result<OperatorProfileEventRow> {
    Ok(OperatorProfileEventRow {
        id: row.get(0)?,
        event_type: row.get(1)?,
        field_key: row.get(2)?,
        value_json: row.get(3)?,
        source: row.get(4)?,
        metadata_json: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_operator_profile_checkin_row(row: &db::Row) -> anyhow::Result<OperatorProfileCheckinRow> {
    Ok(OperatorProfileCheckinRow {
        id: row.get(0)?,
        kind: row.get(1)?,
        scheduled_at: row.get(2)?,
        shown_at: row.get(3)?,
        status: row.get(4)?,
        response_json: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

impl HistoryStore {
    pub async fn upsert_collaboration_session(
        &self,
        parent_task_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO collaboration_sessions (parent_task_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![parent_task_id, session_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_collaboration_sessions(&self) -> Result<Vec<CollaborationSessionRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT parent_task_id, session_json, updated_at FROM collaboration_sessions ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_collaboration_session_row).collect()
    }

    pub async fn get_collaboration_session(
        &self,
        parent_task_id: &str,
    ) -> Result<Option<CollaborationSessionRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT parent_task_id, session_json, updated_at \
                 FROM collaboration_sessions \
                 WHERE parent_task_id = ?1 \
                 LIMIT 1",
                db::db_params![parent_task_id],
            )
            .await?;
        row.map(|row| map_collaboration_session_row(&row))
            .transpose()
    }

    pub async fn upsert_profile_field(
        &self,
        field_key: &str,
        field_value_json: &str,
        confidence: f64,
        source: &str,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO operator_profile_fields \
                 (field_key, field_value_json, confidence, source, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                db::db_params![
                    field_key,
                    field_value_json,
                    confidence,
                    source,
                    now_ts() as i64
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_profile_field(&self, field_key: &str) -> Result<()> {
        self.conn_db
            .execute(
                "DELETE FROM operator_profile_fields WHERE field_key = ?1",
                db::db_params![field_key],
            )
            .await?;
        Ok(())
    }

    pub async fn get_profile_field(
        &self,
        field_key: &str,
    ) -> Result<Option<OperatorProfileFieldRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT field_key, field_value_json, confidence, source, updated_at \
                 FROM operator_profile_fields WHERE field_key = ?1",
                db::db_params![field_key],
            )
            .await?;
        row.map(|row| map_operator_profile_field_row(&row))
            .transpose()
    }

    pub async fn list_profile_fields(&self) -> Result<Vec<OperatorProfileFieldRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT field_key, field_value_json, confidence, source, updated_at \
                 FROM operator_profile_fields ORDER BY updated_at DESC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_operator_profile_field_row).collect()
    }

    pub async fn list_profile_fields_excluding_ordered_by_key(
        &self,
        excluded_field_key: &str,
    ) -> Result<Vec<OperatorProfileFieldRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT field_key, field_value_json, confidence, source, updated_at \
                 FROM operator_profile_fields \
                 WHERE field_key != ?1 \
                 ORDER BY field_key ASC",
                db::db_params![excluded_field_key],
            )
            .await?;
        rows.iter().map(map_operator_profile_field_row).collect()
    }

    pub async fn upsert_profile_consent(&self, consent_key: &str, granted: bool) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO operator_profile_consents \
                 (consent_key, granted, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![consent_key, granted as i64, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_profile_consent(
        &self,
        consent_key: &str,
    ) -> Result<Option<OperatorProfileConsentRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT consent_key, granted, updated_at \
                 FROM operator_profile_consents WHERE consent_key = ?1",
                db::db_params![consent_key],
            )
            .await?;
        row.map(|row| map_operator_profile_consent_row(&row))
            .transpose()
    }

    pub async fn list_profile_consents(&self) -> Result<Vec<OperatorProfileConsentRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT consent_key, granted, updated_at \
                 FROM operator_profile_consents ORDER BY consent_key ASC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_operator_profile_consent_row).collect()
    }

    pub async fn append_profile_event(
        &self,
        id: &str,
        event_type: &str,
        field_key: Option<&str>,
        value_json: Option<&str>,
        source: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO operator_profile_events \
                 (id, event_type, field_key, value_json, source, metadata_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    id,
                    event_type,
                    field_key.map(str::to_string),
                    value_json.map(str::to_string),
                    source,
                    metadata_json.map(str::to_string),
                    now_ts() as i64
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_profile_events(&self, limit: usize) -> Result<Vec<OperatorProfileEventRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, event_type, field_key, value_json, source, metadata_json, created_at \
                 FROM operator_profile_events ORDER BY created_at DESC LIMIT ?1",
                db::db_params![limit as i64],
            )
            .await?;
        rows.iter().map(map_operator_profile_event_row).collect()
    }

    pub async fn upsert_profile_checkin(&self, row: OperatorProfileCheckinRow) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO operator_profile_checkins \
                 (id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(id) DO UPDATE SET \
                   kind         = excluded.kind, \
                   scheduled_at = excluded.scheduled_at, \
                   shown_at     = excluded.shown_at, \
                   status       = excluded.status, \
                   response_json = excluded.response_json, \
                   updated_at   = excluded.updated_at",
                db::db_params![
                    row.id,
                    row.kind,
                    row.scheduled_at,
                    row.shown_at,
                    row.status,
                    row.response_json,
                    row.created_at,
                    now_ts() as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_profile_checkin(&self, id: &str) -> Result<Option<OperatorProfileCheckinRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at \
                 FROM operator_profile_checkins WHERE id = ?1",
                db::db_params![id],
            )
            .await?;
        row.map(|row| map_operator_profile_checkin_row(&row))
            .transpose()
    }

    pub async fn list_profile_checkins(&self) -> Result<Vec<OperatorProfileCheckinRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, kind, scheduled_at, shown_at, status, response_json, created_at, updated_at \
                 FROM operator_profile_checkins ORDER BY scheduled_at ASC",
                db::Params::None,
            )
            .await?;
        rows.iter().map(map_operator_profile_checkin_row).collect()
    }
}
