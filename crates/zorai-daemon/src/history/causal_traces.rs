use super::*;

fn settlement_factor_for_outcome(
    outcome_json: &str,
) -> Option<crate::agent::learning::traces::CausalFactor> {
    let outcome =
        serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(outcome_json)
            .ok()?;
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Success => {
            Some(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastSuccess,
                description: "selected plan completed successfully".to_string(),
                weight: 0.55,
            })
        }
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            Some(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastFailure,
                description: reason,
                weight: 0.85,
            })
        }
        _ => None,
    }
}

fn merged_causal_factors_json(
    current_json: &str,
    settlement_factor: Option<&crate::agent::learning::traces::CausalFactor>,
) -> String {
    let mut factors =
        serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(current_json)
            .unwrap_or_default();
    if let Some(factor) = settlement_factor {
        let exists = factors.iter().any(|current| {
            current.factor_type == factor.factor_type && current.description == factor.description
        });
        if !exists {
            factors.push(factor.clone());
        }
    }
    serde_json::to_string(&factors).unwrap_or_else(|_| current_json.to_string())
}

fn map_causal_trace_record(row: &db::Row) -> anyhow::Result<CausalTraceRecord> {
    Ok(CausalTraceRecord {
        trace_family: row.get(0)?,
        selected_json: row.get(1)?,
        causal_factors_json: row.get(2)?,
        outcome_json: row.get(3)?,
        created_at: row.get::<i64>(4)? as u64,
    })
}

fn map_causal_trace_full_record(row: &db::Row) -> anyhow::Result<CausalTraceFullRecord> {
    Ok(CausalTraceFullRecord {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        goal_run_id: row.get(2)?,
        task_id: row.get(3)?,
        decision_type: row.get(4)?,
        trace_family: row.get(5)?,
        selected_json: row.get(6)?,
        rejected_options_json: row.get(7)?,
        context_hash: row.get(8)?,
        causal_factors_json: row.get(9)?,
        outcome_json: row.get(10)?,
        model_used: row.get(11)?,
        created_at: row.get::<i64>(12)? as u64,
    })
}

impl HistoryStore {
    pub async fn insert_execution_trace(
        &self,
        id: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        task_type: &str,
        outcome: &str,
        quality_score: Option<f64>,
        tool_sequence_json: &str,
        metrics_json: &str,
        duration_ms: u64,
        tokens_used: u32,
        agent_id: &str,
        started_at_ms: u64,
        completed_at_ms: u64,
        created_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO execution_traces (id, thread_id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at, agent_id, started_at_ms, completed_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                db::db_params![id, thread_id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms as i64, tokens_used as i64, created_at as i64, agent_id, started_at_ms as i64, completed_at_ms as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_execution_traces(
        &self,
        task_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<String>> {
        let (sql, params) = match task_type {
            Some(task_type) => (
                "SELECT metrics_json FROM execution_traces WHERE task_type = ?1 ORDER BY created_at DESC LIMIT ?2",
                db::db_params![task_type, limit as i64],
            ),
            None => (
                "SELECT metrics_json FROM execution_traces ORDER BY created_at DESC LIMIT ?1",
                db::db_params![limit as i64],
            ),
        };
        let rows = self.read_db.query(sql, params).await?;
        rows.iter().map(|row| row.get::<String>(0)).collect()
    }

    pub async fn insert_causal_trace(
        &self,
        id: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        decision_type: &str,
        trace_family: &str,
        selected_json: &str,
        rejected_options_json: &str,
        context_hash: &str,
        causal_factors_json: &str,
        outcome_json: &str,
        model_used: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO causal_traces (id, thread_id, goal_run_id, task_id, decision_type, trace_family, selected_json, rejected_options_json, context_hash, causal_factors_json, outcome_json, model_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                db::db_params![
                    id,
                    thread_id,
                    goal_run_id,
                    task_id,
                    decision_type,
                    trace_family,
                    selected_json,
                    rejected_options_json,
                    context_hash,
                    causal_factors_json,
                    outcome_json,
                    model_used,
                    created_at as i64
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_causal_traces_for_option(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let rows = self
            .read_db
            .query(
                "SELECT outcome_json
             FROM causal_traces
             WHERE json_valid(selected_json)
               AND json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
                db::db_params![option_type, limit as i64],
            )
            .await?;
        rows.iter().map(|row| row.get::<String>(0)).collect()
    }

    pub async fn list_recent_causal_trace_records(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<CausalTraceRecord>> {
        let rows = self
            .read_db
            .query(
                "SELECT trace_family, selected_json, causal_factors_json, outcome_json, created_at
             FROM causal_traces
             WHERE json_valid(selected_json)
               AND json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
                db::db_params![option_type, limit as i64],
            )
            .await?;
        rows.iter().map(map_causal_trace_record).collect()
    }

    /// Query causal traces for a given goal_run_id, ordered by creation time descending.
    /// Used by the explainability handler (EXPL-01, EXPL-02).
    pub async fn list_causal_traces_for_goal_run(
        &self,
        goal_run_id: &str,
        limit: u32,
    ) -> Result<Vec<CausalTraceFullRecord>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, thread_id, goal_run_id, task_id, decision_type, trace_family,
                            selected_json, rejected_options_json, context_hash,
                            causal_factors_json, outcome_json, model_used, created_at
                     FROM causal_traces
                     WHERE goal_run_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                db::db_params![goal_run_id, limit as i64],
            )
            .await?;
        rows.iter().map(map_causal_trace_full_record).collect()
    }

    pub async fn settle_skill_selection_causal_traces(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome_json: &str,
    ) -> Result<usize> {
        let updated = self
            .conn_db
            .execute(
                "UPDATE causal_traces
             SET outcome_json = ?4
             WHERE decision_type = 'skill_selection'
               AND json_extract(outcome_json, '$.type') = 'unresolved'
               AND (
                    (?1 IS NOT NULL AND task_id = ?1) OR
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3)
               )",
                db::db_params![task_id, goal_run_id, thread_id, outcome_json],
            )
            .await?;
        Ok(updated as usize)
    }

    pub async fn settle_goal_plan_causal_traces(
        &self,
        goal_run_id: &str,
        outcome_json: &str,
    ) -> Result<usize> {
        let settlement_factor = settlement_factor_for_outcome(outcome_json);
        let rows = self
            .conn_db
            .query(
                "SELECT id, causal_factors_json
                     FROM causal_traces
                     WHERE goal_run_id = ?1
                       AND json_extract(outcome_json, '$.type') = 'unresolved'
                       AND json_extract(selected_json, '$.option_type') IN ('goal_plan', 'goal_replan')",
                db::db_params![goal_run_id],
            )
            .await?;
        let pairs: Vec<(String, String)> = rows
            .iter()
            .map(|row| -> anyhow::Result<(String, String)> {
                Ok((row.get::<String>(0)?, row.get::<String>(1)?))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut updated = 0usize;
        for (trace_id, causal_factors_json) in pairs {
            let merged =
                merged_causal_factors_json(&causal_factors_json, settlement_factor.as_ref());
            updated += self
                .conn_db
                .execute(
                    "UPDATE causal_traces SET outcome_json = ?2, causal_factors_json = ?3 WHERE id = ?1",
                    db::db_params![trace_id, outcome_json, merged],
                )
                .await? as usize;
        }
        Ok(updated)
    }
}
