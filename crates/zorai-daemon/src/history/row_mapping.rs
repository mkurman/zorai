use super::*;

pub(super) async fn refresh_thread_stats_exec<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
    thread_id: &str,
) -> anyhow::Result<()> {
    let row = exec
        .query_opt(
            "SELECT
                COUNT(*),
                COALESCE(SUM(total_tokens), 0),
                substr(content, 1, 100),
                MAX(created_at)
             FROM agent_messages WHERE thread_id = ?1 AND deleted_at IS NULL",
            super::db::db_params![thread_id],
        )
        .await?;
    let (message_count, total_tokens, last_preview_opt, latest_message_at_opt): (
        i64,
        i64,
        Option<String>,
        Option<i64>,
    ) = match row {
        Some(row) => (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?),
        None => (0, 0, None, None),
    };
    let last_preview = last_preview_opt.unwrap_or_default();
    let latest_message_at = latest_message_at_opt.unwrap_or_else(|| now_ts() as i64);

    exec.execute(
        "UPDATE agent_threads
         SET
            message_count = ?2,
            total_tokens = ?3,
            last_preview = ?4,
            updated_at = MAX(updated_at, ?5)
         WHERE id = ?1",
        super::db::db_params![
            thread_id,
            message_count,
            total_tokens,
            last_preview,
            latest_message_at
        ],
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CausalTraceRecord {
    pub trace_family: String,
    pub selected_json: String,
    pub causal_factors_json: String,
    pub outcome_json: String,
    pub created_at: u64,
}

/// Full causal trace record for explainability queries (EXPL-01, EXPL-02).
#[derive(Debug, Clone)]
pub struct CausalTraceFullRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub decision_type: String,
    pub trace_family: String,
    pub selected_json: String,
    pub rejected_options_json: String,
    pub context_hash: String,
    pub causal_factors_json: String,
    pub outcome_json: String,
    pub model_used: Option<String>,
    pub created_at: u64,
}

pub(super) fn map_command_log_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandLogEntry> {
    Ok(CommandLogEntry {
        id: row.get(0)?,
        command: row.get(1)?,
        timestamp: row.get(2)?,
        path: row.get(3)?,
        cwd: row.get(4)?,
        workspace_id: row.get(5)?,
        surface_id: row.get(6)?,
        pane_id: row.get(7)?,
        exit_code: row.get(8)?,
        duration_ms: row.get(9)?,
    })
}

pub(super) fn map_command_log_entry_row(row: &super::db::Row) -> anyhow::Result<CommandLogEntry> {
    Ok(CommandLogEntry {
        id: row.get(0)?,
        command: row.get(1)?,
        timestamp: row.get(2)?,
        path: row.get(3)?,
        cwd: row.get(4)?,
        workspace_id: row.get(5)?,
        surface_id: row.get(6)?,
        pane_id: row.get(7)?,
        exit_code: row.get(8)?,
        duration_ms: row.get(9)?,
    })
}

pub(super) fn map_event_log_row(row: &super::db::Row) -> anyhow::Result<EventLogRow> {
    Ok(EventLogRow {
        id: row.get(0)?,
        event_family: row.get(1)?,
        event_kind: row.get(2)?,
        state: row.get(3)?,
        thread_id: row.get(4)?,
        payload_json: row.get(5)?,
        risk_label: row.get(6)?,
        handled_at_ms: row.get::<i64>(7)?.max(0) as u64,
    })
}

pub(super) fn map_harness_state_record_row(
    row: &super::db::Row,
) -> anyhow::Result<HarnessStateRecordRow> {
    Ok(HarnessStateRecordRow {
        entry_id: row.get(0)?,
        entity_id: row.get(1)?,
        thread_id: row.get(2)?,
        goal_run_id: row.get(3)?,
        task_id: row.get(4)?,
        record_kind: row.get(5)?,
        status: row.get(6)?,
        summary: row.get(7)?,
        payload_json: row.get(8)?,
        created_at_ms: row.get::<i64>(9)?.max(0) as u64,
    })
}

pub(super) fn map_cognitive_resonance_sample_row(
    row: &super::db::Row,
) -> anyhow::Result<CognitiveResonanceSampleRow> {
    Ok(CognitiveResonanceSampleRow {
        id: Some(row.get(0)?),
        sampled_at_ms: row.get::<i64>(1)?.max(0) as u64,
        revision_velocity_ms: row.get::<Option<i64>>(2)?.map(|value| value.max(0) as u64),
        session_entropy: row.get(3)?,
        approval_latency_ms: row.get::<Option<i64>>(4)?.map(|value| value.max(0) as u64),
        tool_hesitation_count: row.get::<i64>(5)?.max(0) as u64,
        cognitive_state: row.get(6)?,
        state_confidence: row.get(7)?,
        resonance_score: row.get(8)?,
        verbosity_adjustment: row.get(9)?,
        risk_adjustment: row.get(10)?,
        proactiveness_adjustment: row.get(11)?,
        memory_urgency_adjustment: row.get(12)?,
    })
}

pub(super) fn map_behavior_adjustment_log_row(
    row: &super::db::Row,
) -> anyhow::Result<BehaviorAdjustmentLogRow> {
    Ok(BehaviorAdjustmentLogRow {
        id: Some(row.get(0)?),
        adjusted_at_ms: row.get::<i64>(1)?.max(0) as u64,
        parameter: row.get(2)?,
        old_value: row.get(3)?,
        new_value: row.get(4)?,
        trigger_reason: row.get(5)?,
        resonance_score: row.get(6)?,
    })
}

pub(super) fn map_dream_cycle_row(row: &super::db::Row) -> anyhow::Result<DreamCycleRow> {
    Ok(DreamCycleRow {
        id: Some(row.get(0)?),
        started_at_ms: row.get::<i64>(1)?.max(0) as u64,
        completed_at_ms: row.get::<Option<i64>>(2)?.map(|value| value.max(0) as u64),
        idle_duration_ms: row.get::<i64>(3)?.max(0) as u64,
        tasks_analyzed: row.get::<i64>(4)?.max(0) as u64,
        counterfactuals_generated: row.get::<i64>(5)?.max(0) as u64,
        counterfactuals_successful: row.get::<i64>(6)?.max(0) as u64,
        status: row.get(7)?,
    })
}

pub(super) fn map_counterfactual_evaluation_row(
    row: &super::db::Row,
) -> anyhow::Result<CounterfactualEvaluationRow> {
    Ok(CounterfactualEvaluationRow {
        id: Some(row.get(0)?),
        dream_cycle_id: row.get(1)?,
        source_task_id: row.get(2)?,
        variation_type: row.get(3)?,
        counterfactual_description: row.get(4)?,
        estimated_token_saving: row.get(5)?,
        estimated_time_saving_ms: row.get(6)?,
        estimated_revision_reduction: row.get::<Option<i64>>(7)?.map(|value| value.max(0) as u64),
        score: row.get(8)?,
        threshold_met: row.get::<i64>(9)? != 0,
        created_at_ms: row.get::<i64>(10)?.max(0) as u64,
    })
}

pub(super) fn map_agent_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDbMessage> {
    Ok(AgentDbMessage {
        reasoning_tokens: row.get::<_, Option<i64>>(14).ok().flatten(),
        thinking_ms: row.get::<_, Option<i64>>(15).ok().flatten(),
        id: row.get(0)?,
        thread_id: row.get(1)?,
        created_at: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        total_tokens: row.get(9)?,
        cost_usd: row.get(10)?,
        reasoning: row.get(11)?,
        tool_calls_json: row.get(12)?,
        metadata_json: row.get(13)?,
    })
}

pub(super) fn map_agent_thread_db(row: &super::db::Row) -> anyhow::Result<AgentDbThread> {
    Ok(AgentDbThread {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        surface_id: row.get(2)?,
        pane_id: row.get(3)?,
        agent_name: row.get(4)?,
        title: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        message_count: row.get(8)?,
        total_tokens: row.get(9)?,
        last_preview: row.get(10)?,
        metadata_json: row.get(11)?,
    })
}

pub(super) fn map_agent_message_db(row: &super::db::Row) -> anyhow::Result<AgentDbMessage> {
    Ok(AgentDbMessage {
        reasoning_tokens: row.get::<Option<i64>>(14).ok().flatten(),
        thinking_ms: row.get::<Option<i64>>(15).ok().flatten(),
        id: row.get(0)?,
        thread_id: row.get(1)?,
        created_at: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        total_tokens: row.get(9)?,
        cost_usd: row.get(10)?,
        reasoning: row.get(11)?,
        tool_calls_json: row.get(12)?,
        metadata_json: row.get(13)?,
    })
}

pub(super) fn map_memory_distillation_progress_row_db(
    row: &super::db::Row,
) -> anyhow::Result<MemoryDistillationProgressRow> {
    let span_json: Option<String> = row.get(3)?;
    let last_processed_span = span_json
        .as_deref()
        .map(|json| serde_json::from_str::<AgentMessageSpan>(json))
        .transpose()?;

    Ok(MemoryDistillationProgressRow {
        source_thread_id: row.get(0)?,
        last_processed_cursor: AgentMessageCursor {
            created_at: row.get(1)?,
            message_id: row.get(2)?,
        },
        last_processed_span,
        last_run_at_ms: row.get(4)?,
        updated_at_ms: row.get(5)?,
        agent_id: row.get(6)?,
    })
}

pub(super) fn map_memory_distillation_log_row_db(
    row: &super::db::Row,
) -> anyhow::Result<MemoryDistillationLogRow> {
    let span_json: Option<String> = row.get(3)?;
    let source_message_span = span_json
        .as_deref()
        .map(|json| serde_json::from_str::<AgentMessageSpan>(json))
        .transpose()?;

    Ok(MemoryDistillationLogRow {
        id: row.get(0)?,
        source_thread_id: row.get(1)?,
        source_message_range: row.get(2)?,
        source_message_span,
        distilled_fact: row.get(4)?,
        target_file: row.get(5)?,
        category: row.get(6)?,
        confidence: row.get(7)?,
        created_at_ms: row.get(8)?,
        applied_to_memory: row.get::<i64>(9)? != 0,
        agent_id: row.get(10)?,
    })
}

pub(super) fn map_forge_pass_log_row_db(row: &super::db::Row) -> anyhow::Result<ForgePassLogRow> {
    Ok(ForgePassLogRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        period_start_ms: row.get(2)?,
        period_end_ms: row.get(3)?,
        traces_analyzed: row.get(4)?,
        patterns_found: row.get(5)?,
        hints_applied: row.get(6)?,
        hints_logged: row.get(7)?,
        completed_at_ms: row.get(8)?,
    })
}

pub(super) fn map_handoff_routing_row_db(
    row: &super::db::Row,
) -> anyhow::Result<HandoffRoutingRow> {
    Ok(HandoffRoutingRow {
        id: row.get(0)?,
        to_specialist_id: row.get(1)?,
        capability_tags_json: row.get(2)?,
        routing_method: row.get(3)?,
        routing_score: row.get(4)?,
        fallback_used: row.get::<i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}

pub(super) fn map_transcript_index_entry_db(
    row: &super::db::Row,
) -> anyhow::Result<TranscriptIndexEntry> {
    Ok(TranscriptIndexEntry {
        id: row.get(0)?,
        pane_id: row.get(1)?,
        workspace_id: row.get(2)?,
        surface_id: row.get(3)?,
        filename: row.get(4)?,
        reason: row.get(5)?,
        captured_at: row.get(6)?,
        size_bytes: row.get(7)?,
        preview: row.get(8)?,
    })
}

pub(super) fn map_snapshot_index_entry_db(
    row: &super::db::Row,
) -> anyhow::Result<SnapshotIndexEntry> {
    Ok(SnapshotIndexEntry {
        snapshot_id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        kind: row.get(3)?,
        label: row.get(4)?,
        path: row.get(5)?,
        created_at: row.get(6)?,
        details_json: row.get(7)?,
    })
}

pub(super) fn map_agent_event_row_db(row: &super::db::Row) -> anyhow::Result<AgentEventRow> {
    Ok(AgentEventRow {
        id: row.get(0)?,
        category: row.get(1)?,
        kind: row.get(2)?,
        pane_id: row.get(3)?,
        workspace_id: row.get(4)?,
        surface_id: row.get(5)?,
        session_id: row.get(6)?,
        payload_json: row.get(7)?,
        timestamp: row.get(8)?,
    })
}

pub(super) fn flatten_option_str(value: &Option<Option<String>>) -> Option<&str> {
    value.as_ref().and_then(|inner| inner.as_deref())
}

pub(super) fn flatten_option_i64(value: &Option<Option<i64>>) -> Option<i64> {
    value.as_ref().copied().flatten()
}

pub(super) fn flatten_option_f64(value: &Option<Option<f64>>) -> Option<f64> {
    value.as_ref().copied().flatten()
}
