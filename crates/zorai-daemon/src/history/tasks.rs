use super::*;
use crate::agent::types::ApiTransport;

fn parse_string_vec_json(value: Option<String>) -> Option<Vec<String>> {
    value.and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
}

fn serialize_string_vec_json(
    value: &Option<Vec<String>>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .call_err()
}

fn serialize_supervisor_config_json(
    value: &Option<crate::agent::types::SupervisorConfig>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .call_err()
}

fn parse_supervisor_config_json(
    value: Option<String>,
) -> Option<crate::agent::types::SupervisorConfig> {
    value.and_then(|json| serde_json::from_str::<crate::agent::types::SupervisorConfig>(&json).ok())
}

fn serialize_completion_contract_json(
    value: &Option<crate::agent::types::TaskCompletionContract>,
) -> std::result::Result<Option<String>, tokio_rusqlite::Error> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .call_err()
}

fn parse_completion_contract_json(
    value: Option<String>,
) -> Option<crate::agent::types::TaskCompletionContract> {
    value.and_then(|json| {
        serde_json::from_str::<crate::agent::types::TaskCompletionContract>(&json).ok()
    })
}

fn preserve_notification_lifecycle_state(
    notification: &mut zorai_protocol::InboxNotification,
    existing: zorai_protocol::InboxNotification,
) {
    notification.read_at = notification.read_at.or(existing.read_at);
    notification.archived_at = notification.archived_at.or(existing.archived_at);
    notification.deleted_at = notification.deleted_at.or(existing.deleted_at);
}

/// Synchronous task-upsert body that runs inside an existing transaction.
/// Extracted so that single-task `upsert_agent_task` and bulk
/// `upsert_agent_tasks_batch` share one implementation — keep the column
/// list and side-effect order in lockstep here, never duplicate it.
async fn upsert_agent_task_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    task: &AgentTask,
) -> anyhow::Result<()> {
    let mut guarded_task;
    let task = if task.status == TaskStatus::Completed {
        let blockers = task.completion_blockers();
        if blockers.is_empty() {
            task
        } else {
            guarded_task = task.clone();
            guarded_task.status = TaskStatus::Blocked;
            guarded_task.progress = guarded_task.progress.min(99);
            guarded_task.completed_at = None;
            guarded_task.blocked_reason = Some(format!(
                "completion contract remains open: {}",
                blockers.join("; ")
            ));
            if let Some(contract) = guarded_task.completion_contract.as_mut() {
                contract.terminal_status = None;
            }
            &guarded_task
        }
    } else {
        task
    };
    let notify_channels_json = serde_json::to_string(&task.notify_channels)?;
    let tool_whitelist_json = serialize_string_vec_json(&task.tool_whitelist)?;
    let tool_blacklist_json = serialize_string_vec_json(&task.tool_blacklist)?;
    let supervisor_config_json = serialize_supervisor_config_json(&task.supervisor_config)?;
    let completion_contract_json = serialize_completion_contract_json(&task.completion_contract)?;

    exec.execute(
        "INSERT OR REPLACE INTO agent_tasks \
         (id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id, tool_whitelist_json, tool_blacklist_json, context_budget_tokens, context_overflow_action, termination_conditions, success_criteria, max_duration_secs, supervisor_config_json, override_api_transport, completion_contract_json, deleted_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, NULL)",
        db::db_params![
            task.id.clone(),
            task.title.clone(),
            task.description.clone(),
            task_status_to_str(task.status),
            task_priority_to_str(task.priority),
            task.progress as i64,
            task.created_at as i64,
            task.started_at.map(|value| value as i64),
            task.completed_at.map(|value| value as i64),
            task.error.clone(),
            task.result.clone(),
            task.thread_id.clone(),
            task.source.clone(),
            if task.notify_on_complete { 1i64 } else { 0i64 },
            notify_channels_json,
            task.command.clone(),
            task.session_id.clone(),
            task.goal_run_id.clone(),
            task.goal_run_title.clone(),
            task.goal_step_id.clone(),
            task.goal_step_title.clone(),
            task.parent_task_id.clone(),
            task.parent_thread_id.clone(),
            task.runtime.clone(),
            task.retry_count as i64,
            task.max_retries as i64,
            task.next_retry_at.map(|value| value as i64),
            task.scheduled_at.map(|value| value as i64),
            task.blocked_reason.clone(),
            task.awaiting_approval_id.clone(),
            task.policy_fingerprint.clone(),
            task.approval_expires_at.map(|value| value as i64),
            task.containment_scope.clone(),
            task.compensation_status.clone(),
            task.compensation_summary.clone(),
            task.lane_id.clone(),
            task.last_error.clone(),
            task.override_provider.clone(),
            task.override_model.clone(),
            task.override_system_prompt.clone(),
            task.sub_agent_def_id.clone(),
            tool_whitelist_json,
            tool_blacklist_json,
            task.context_budget_tokens.map(|value| value as i64),
            task.context_overflow_action.map(context_overflow_action_to_str),
            task.termination_conditions.clone(),
            task.success_criteria.clone(),
            task.max_duration_secs.map(|value| value as i64),
            supervisor_config_json,
            task.override_api_transport.map(|value| value.as_snake_str()),
            completion_contract_json,
        ],
    )
    .await?;

    exec.execute(
        "UPDATE agent_task_dependencies SET deleted_at = ?2 WHERE task_id = ?1 AND deleted_at IS NULL",
        db::db_params![task.id.clone(), now_ts() as i64],
    )
    .await?;
    for (ordinal, dependency) in task.dependencies.iter().enumerate() {
        exec.execute(
            "INSERT OR REPLACE INTO agent_task_dependencies (task_id, depends_on_task_id, ordinal, deleted_at) VALUES (?1, ?2, ?3, NULL)",
            db::db_params![task.id.clone(), dependency.clone(), ordinal as i64],
        )
        .await?;
    }

    exec.execute(
        "UPDATE agent_task_logs SET deleted_at = ?2 WHERE task_id = ?1 AND deleted_at IS NULL",
        db::db_params![task.id.clone(), now_ts() as i64],
    )
    .await?;
    for log in &task.logs {
        exec.execute(
            "INSERT OR REPLACE INTO agent_task_logs (id, task_id, timestamp, level, phase, message, details, attempt, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
            db::db_params![
                log.id.clone(),
                task.id.clone(),
                log.timestamp as i64,
                task_log_level_to_str(log.level),
                log.phase.clone(),
                log.message.clone(),
                log.details.clone(),
                log.attempt as i64,
            ],
        )
        .await?;
    }

    embedding_queue::enqueue_task_embedding_job_exec(exec, task, now_ts() as i64).await?;
    Ok(())
}

fn append_agent_task_query_filters(
    task_sql: &mut String,
    task_values: &mut Vec<db::Value>,
    query: &AgentTaskListQuery,
) {
    if let Some(id) = query.id.as_deref().filter(|value| !value.is_empty()) {
        task_sql.push_str(" AND id = ?");
        task_values.push(db::Value::Text(id.to_string()));
    }
    let ids = query
        .ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !ids.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        task_sql.push_str(&format!(" AND id IN ({placeholders})"));
        for id in ids {
            task_values.push(db::Value::Text(id.to_string()));
        }
    }
    if let Some(status) = query.status.as_deref().filter(|value| !value.is_empty()) {
        task_sql.push_str(" AND status = ?");
        task_values.push(db::Value::Text(status.to_string()));
    }
    let statuses = query
        .statuses
        .iter()
        .map(|status| status.trim())
        .filter(|status| !status.is_empty())
        .collect::<Vec<_>>();
    if !statuses.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(statuses.len())
            .collect::<Vec<_>>()
            .join(", ");
        task_sql.push_str(&format!(" AND status IN ({placeholders})"));
        for status in statuses {
            task_values.push(db::Value::Text(status.to_string()));
        }
    }
    if let Some(source) = query.source.as_deref().filter(|value| !value.is_empty()) {
        task_sql.push_str(" AND source = ?");
        task_values.push(db::Value::Text(source.to_string()));
    }
    if let Some(thread_id) = query.thread_id.as_deref().filter(|value| !value.is_empty()) {
        task_sql.push_str(" AND thread_id = ?");
        task_values.push(db::Value::Text(thread_id.to_string()));
    }
    let thread_ids = query
        .thread_ids
        .iter()
        .map(|thread_id| thread_id.trim())
        .filter(|thread_id| !thread_id.is_empty())
        .collect::<Vec<_>>();
    if !thread_ids.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(thread_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        task_sql.push_str(&format!(" AND thread_id IN ({placeholders})"));
        for thread_id in thread_ids {
            task_values.push(db::Value::Text(thread_id.to_string()));
        }
    }
    if let Some(goal_run_id) = query
        .goal_run_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        task_sql.push_str(" AND goal_run_id = ?");
        task_values.push(db::Value::Text(goal_run_id.to_string()));
    }
    if let Some(parent_task_id) = query
        .parent_task_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        task_sql.push_str(" AND parent_task_id = ?");
        task_values.push(db::Value::Text(parent_task_id.to_string()));
    }
    let parent_task_ids = query
        .parent_task_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !parent_task_ids.is_empty() {
        let placeholders = std::iter::repeat("?")
            .take(parent_task_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        task_sql.push_str(&format!(" AND parent_task_id IN ({placeholders})"));
        for parent_task_id in parent_task_ids {
            task_values.push(db::Value::Text(parent_task_id.to_string()));
        }
    }
    if let Some(approval_id) = query
        .awaiting_approval_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        task_sql.push_str(" AND awaiting_approval_id = ?");
        task_values.push(db::Value::Text(approval_id.to_string()));
    }
    if query.supervisor_config_present {
        task_sql.push_str(
            " AND supervisor_config_json IS NOT NULL AND TRIM(supervisor_config_json) <> ''",
        );
    }
    if query.exclude_terminal_statuses {
        task_sql
            .push_str(" AND status NOT IN ('completed', 'budget_exceeded', 'failed', 'cancelled')");
    }
}

const AGENT_TASK_SELECT_SQL: &str = "SELECT id, title, description, status, priority, progress, created_at, started_at, completed_at, error, result, thread_id, source, notify_on_complete, notify_channels_json, command, session_id, goal_run_id, goal_run_title, goal_step_id, goal_step_title, parent_task_id, parent_thread_id, runtime, retry_count, max_retries, next_retry_at, scheduled_at, blocked_reason, awaiting_approval_id, policy_fingerprint, approval_expires_at, containment_scope, compensation_status, compensation_summary, lane_id, last_error, override_provider, override_model, override_system_prompt, sub_agent_def_id, tool_whitelist_json, tool_blacklist_json, context_budget_tokens, context_overflow_action, termination_conditions, success_criteria, max_duration_secs, supervisor_config_json, override_api_transport, completion_contract_json FROM agent_tasks WHERE deleted_at IS NULL";

fn append_agent_task_order_and_limit(
    task_sql: &mut String,
    task_values: &mut Vec<db::Value>,
    query: &AgentTaskListQuery,
) {
    if query.order_by_recent_activity_desc {
        task_sql.push_str(
            " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
        );
    } else {
        task_sql.push_str(
            " ORDER BY CASE status \
             WHEN 'in_progress' THEN 0 \
             WHEN 'awaiting_approval' THEN 1 \
             WHEN 'failed_analyzing' THEN 2 \
             WHEN 'blocked' THEN 3 \
             WHEN 'queued' THEN 4 \
             WHEN 'failed' THEN 5 \
             WHEN 'completed' THEN 6 \
             ELSE 7 END, \
             CASE priority \
             WHEN 'urgent' THEN 0 \
             WHEN 'high' THEN 1 \
             WHEN 'normal' THEN 2 \
             ELSE 3 END, \
             created_at DESC",
        );
    }
    if let Some(limit) = query.limit {
        task_sql.push_str(" LIMIT ?");
        task_values.push(db::Value::Integer(limit.max(1) as i64));
    }
}

fn map_agent_task_row(row: &db::Row) -> anyhow::Result<AgentTask> {
    let task_id: String = row.get(0)?;
    let notify_channels_json: String = row.get(14)?;
    Ok(AgentTask {
        id: task_id,
        title: row.get(1)?,
        description: row.get(2)?,
        status: parse_task_status(&row.get::<String>(3)?),
        priority: parse_task_priority(&row.get::<String>(4)?),
        progress: row.get::<i64>(5)? as u8,
        created_at: row.get::<i64>(6)? as u64,
        started_at: row.get::<Option<i64>>(7)?.map(|value| value as u64),
        completed_at: row.get::<Option<i64>>(8)?.map(|value| value as u64),
        error: row.get(9)?,
        result: row.get(10)?,
        thread_id: row.get(11)?,
        source: row.get(12)?,
        notify_on_complete: row.get::<i64>(13)? != 0,
        notify_channels: serde_json::from_str(&notify_channels_json).unwrap_or_default(),
        dependencies: Vec::new(),
        command: row.get(15)?,
        session_id: row.get(16)?,
        goal_run_id: row.get(17)?,
        goal_run_title: row.get(18)?,
        goal_step_id: row.get(19)?,
        goal_step_title: row.get(20)?,
        parent_task_id: row.get(21)?,
        parent_thread_id: row.get(22)?,
        runtime: row
            .get::<Option<String>>(23)?
            .unwrap_or_else(|| "daemon".to_string()),
        retry_count: row.get::<i64>(24)? as u32,
        max_retries: row.get::<i64>(25)? as u32,
        next_retry_at: row.get::<Option<i64>>(26)?.map(|value| value as u64),
        scheduled_at: row.get::<Option<i64>>(27)?.map(|value| value as u64),
        blocked_reason: row.get(28)?,
        awaiting_approval_id: row.get(29)?,
        policy_fingerprint: row.get(30)?,
        approval_expires_at: row.get::<Option<i64>>(31)?.map(|value| value as u64),
        containment_scope: row.get(32)?,
        compensation_status: row.get(33)?,
        compensation_summary: row.get(34)?,
        lane_id: row.get(35)?,
        last_error: row.get(36)?,
        logs: Vec::new(),
        override_provider: row.get(37)?,
        override_model: row.get(38)?,
        override_system_prompt: row.get(39)?,
        sub_agent_def_id: row.get(40)?,
        completion_contract: parse_completion_contract_json(row.get(50)?),
        tool_whitelist: parse_string_vec_json(row.get(41)?),
        tool_blacklist: parse_string_vec_json(row.get(42)?),
        context_budget_tokens: row.get::<Option<i64>>(43)?.map(|value| value as u32),
        context_overflow_action: row
            .get::<Option<String>>(44)?
            .map(|value| parse_context_overflow_action(&value)),
        termination_conditions: row.get(45)?,
        success_criteria: row.get(46)?,
        max_duration_secs: row.get::<Option<i64>>(47)?.map(|value| value as u64),
        supervisor_config: parse_supervisor_config_json(row.get(48)?),
        override_api_transport: row
            .get::<Option<String>>(49)?
            .and_then(|value| ApiTransport::from_snake_str(&value)),
    })
}

fn map_agent_task_quiet_recovery_ref(row: &db::Row) -> anyhow::Result<AgentTaskQuietRecoveryRef> {
    Ok(AgentTaskQuietRecoveryRef {
        id: row.get(0)?,
        source: row.get(1)?,
        status: parse_task_status(&row.get::<String>(2)?),
        progress: row.get::<i64>(3)? as u8,
        created_at: row.get::<i64>(4)? as u64,
        started_at: row.get::<Option<i64>>(5)?.map(|value| value as u64),
        goal_run_id: row.get(6)?,
        parent_task_id: row.get(7)?,
        thread_id: row.get(8)?,
    })
}

async fn hydrate_agent_task_related_rows(
    conn: &dyn db::DbConn,
    tasks: &mut [AgentTask],
    include_logs: bool,
) -> anyhow::Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }

    let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    let id_placeholders = std::iter::repeat("?")
        .take(task_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let id_values = || {
        task_ids
            .iter()
            .cloned()
            .map(db::Value::Text)
            .collect::<Vec<_>>()
    };

    let dependency_rows = conn
        .query(
            &format!(
                "SELECT task_id, depends_on_task_id \
             FROM agent_task_dependencies \
             WHERE deleted_at IS NULL AND task_id IN ({id_placeholders}) \
             ORDER BY task_id ASC, ordinal ASC"
            ),
            db::Params::Positional(id_values()),
        )
        .await?;
    let mut dependency_map = std::collections::HashMap::<String, Vec<String>>::new();
    for row in dependency_rows.iter() {
        let task_id = row.get::<String>(0)?;
        let dependency = row.get::<String>(1)?;
        dependency_map.entry(task_id).or_default().push(dependency);
    }

    // `agent_task_logs` can hold hundreds of MB of historical tool output
    // (a single runaway task accumulated 200MB on 2026-08-30). List endpoints
    // that only need task metadata must not read, serialize, and ship it —
    // that blocked the SQLite read pool for 15-30s and stalled every other
    // IPC query (thread list, status, prompt queue) behind it.
    let mut log_map = std::collections::HashMap::<String, Vec<AgentTaskLogEntry>>::new();
    if include_logs {
        let log_rows = conn
            .query(
                &format!(
                    "SELECT id, task_id, timestamp, level, phase, message, details, attempt \
             FROM agent_task_logs \
             WHERE deleted_at IS NULL AND task_id IN ({id_placeholders}) \
             ORDER BY task_id ASC, timestamp ASC"
                ),
                db::Params::Positional(id_values()),
            )
            .await?;
        for row in log_rows.iter() {
            let task_id = row.get::<String>(1)?;
            let log = AgentTaskLogEntry {
                id: row.get(0)?,
                timestamp: row.get::<i64>(2)? as u64,
                level: parse_task_log_level(&row.get::<String>(3)?),
                phase: row.get(4)?,
                message: row.get(5)?,
                details: row.get(6)?,
                attempt: row.get::<i64>(7)? as u32,
            };
            log_map.entry(task_id).or_default().push(log);
        }
    }

    for task in tasks {
        task.dependencies = dependency_map.remove(&task.id).unwrap_or_default();
        task.logs = log_map.remove(&task.id).unwrap_or_default();
    }
    Ok(())
}

async fn load_agent_tasks_from_query(
    conn: &dyn db::DbConn,
    query: &AgentTaskListQuery,
    task_sql: String,
    task_values: Vec<db::Value>,
) -> anyhow::Result<Vec<AgentTask>> {
    load_agent_tasks_from_query_with_logs(conn, query, task_sql, task_values, true).await
}

async fn load_agent_tasks_from_query_with_logs(
    conn: &dyn db::DbConn,
    query: &AgentTaskListQuery,
    mut task_sql: String,
    mut task_values: Vec<db::Value>,
    include_logs: bool,
) -> anyhow::Result<Vec<AgentTask>> {
    append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
    append_agent_task_order_and_limit(&mut task_sql, &mut task_values, query);

    let rows = conn
        .query(&task_sql, db::Params::Positional(task_values))
        .await?;
    let mut tasks = rows
        .iter()
        .map(map_agent_task_row)
        .collect::<anyhow::Result<Vec<_>>>()?;

    hydrate_agent_task_related_rows(conn, &mut tasks, include_logs).await?;
    Ok(tasks)
}

impl HistoryStore {
    pub async fn upsert_notification(
        &self,
        notification: &zorai_protocol::InboxNotification,
    ) -> Result<()> {
        let row = crate::notifications::notification_event_row(notification)?;
        self.upsert_agent_event(&row).await
    }

    pub(crate) async fn get_notification_by_id(
        &self,
        notification_id: &str,
    ) -> Result<Option<zorai_protocol::InboxNotification>> {
        let payload_json: Option<String> = self
            .conn_db
            .query_opt(
                "SELECT payload_json FROM agent_events WHERE id = ?1 AND category = ?2",
                db::db_params![notification_id, crate::notifications::NOTIFICATION_CATEGORY],
            )
            .await?
            .map(|row| row.get::<Option<String>>(0))
            .transpose()?
            .flatten();
        Ok(payload_json
            .and_then(|json| serde_json::from_str::<zorai_protocol::InboxNotification>(&json).ok()))
    }

    pub async fn list_notifications(
        &self,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<zorai_protocol::InboxNotification>> {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let inactive_filter = if include_inactive {
            ""
        } else {
            " AND json_extract(payload_json, '$.archived_at') IS NULL
              AND json_extract(payload_json, '$.deleted_at') IS NULL"
        };
        let sql = format!(
            "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp
             FROM agent_events
             WHERE category = ?1
               AND json_valid(payload_json)
               {inactive_filter}
             ORDER BY
               COALESCE(json_extract(payload_json, '$.updated_at'), timestamp) DESC,
               COALESCE(json_extract(payload_json, '$.created_at'), timestamp) DESC
             LIMIT ?2"
        );
        let rows = self
            .read_db
            .query(
                &sql,
                db::db_params![crate::notifications::NOTIFICATION_CATEGORY, limit],
            )
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_agent_event_row_db(row).ok())
            .filter_map(|row| crate::notifications::parse_notification_row(&row))
            .collect())
    }

    pub async fn list_notifications_by_source(
        &self,
        source: &str,
        include_inactive: bool,
        limit: Option<usize>,
    ) -> Result<Vec<zorai_protocol::InboxNotification>> {
        let limit = limit.unwrap_or(500).max(1) as i64;
        let inactive_filter = if include_inactive {
            ""
        } else {
            " AND json_extract(payload_json, '$.archived_at') IS NULL
              AND json_extract(payload_json, '$.deleted_at') IS NULL"
        };
        let sql = format!(
            "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp
             FROM agent_events
             WHERE category = ?1
               AND json_valid(payload_json)
               AND json_extract(payload_json, '$.source') = ?2
               {inactive_filter}
             ORDER BY
               COALESCE(json_extract(payload_json, '$.updated_at'), timestamp) DESC,
               COALESCE(json_extract(payload_json, '$.created_at'), timestamp) DESC
             LIMIT ?3"
        );
        let rows = self
            .read_db
            .query(
                &sql,
                db::db_params![crate::notifications::NOTIFICATION_CATEGORY, source, limit],
            )
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_agent_event_row_db(row).ok())
            .filter_map(|row| crate::notifications::parse_notification_row(&row))
            .collect())
    }

    pub async fn archive_notifications_by_source_except_ids(
        &self,
        source: &str,
        active_ids: &[String],
        archived_at: i64,
    ) -> Result<usize> {
        let source = source.to_string();
        let mut active_ids = active_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        active_ids.sort();
        active_ids.dedup();

        let mut sql = "UPDATE agent_events
             SET payload_json = json_set(payload_json, '$.archived_at', ?, '$.updated_at', ?),
                 timestamp = ?
             WHERE category = ?
               AND json_valid(payload_json)
               AND json_extract(payload_json, '$.source') = ?
               AND json_extract(payload_json, '$.archived_at') IS NULL
               AND json_extract(payload_json, '$.deleted_at') IS NULL"
            .to_string();
        let mut values = vec![
            db::Value::Integer(archived_at),
            db::Value::Integer(archived_at),
            db::Value::Integer(archived_at),
            db::Value::Text(crate::notifications::NOTIFICATION_CATEGORY.to_string()),
            db::Value::Text(source),
        ];
        if !active_ids.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(active_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" AND id NOT IN ({placeholders})"));
            values.extend(active_ids.into_iter().map(db::Value::Text));
        }
        let updated = self
            .conn_db
            .execute(&sql, db::Params::Positional(values))
            .await?;
        Ok(updated as usize)
    }

    pub async fn mark_all_notifications_read(&self, read_at: i64) -> Result<usize> {
        let updated = self
            .conn_db
            .execute(
                "UPDATE agent_events
                 SET payload_json = json_set(payload_json, '$.read_at', ?1, '$.updated_at', ?1),
                     timestamp = ?1
                 WHERE category = ?2
                   AND json_valid(payload_json)
                   AND json_extract(payload_json, '$.read_at') IS NULL
                   AND json_extract(payload_json, '$.archived_at') IS NULL
                   AND json_extract(payload_json, '$.deleted_at') IS NULL",
                db::db_params![read_at, crate::notifications::NOTIFICATION_CATEGORY],
            )
            .await?;
        Ok(updated as usize)
    }

    pub async fn archive_read_notifications(&self, archived_at: i64) -> Result<usize> {
        let updated = self
            .conn_db
            .execute(
                "UPDATE agent_events
                 SET payload_json = json_set(payload_json, '$.archived_at', ?1, '$.updated_at', ?1),
                     timestamp = ?1
                 WHERE category = ?2
                   AND json_valid(payload_json)
                   AND json_extract(payload_json, '$.read_at') IS NOT NULL
                   AND json_extract(payload_json, '$.archived_at') IS NULL
                   AND json_extract(payload_json, '$.deleted_at') IS NULL",
                db::db_params![archived_at, crate::notifications::NOTIFICATION_CATEGORY],
            )
            .await?;
        Ok(updated as usize)
    }

    pub async fn upsert_agent_event(&self, entry: &AgentEventRow) -> Result<()> {
        let mut entry = entry.clone();
        if let Some(mut notification) = crate::notifications::parse_notification_row(&entry) {
            if let Some(existing) = self.get_notification_by_id(&notification.id).await? {
                preserve_notification_lifecycle_state(&mut notification, existing);
                entry = crate::notifications::notification_event_row(&notification)?;
            }
        }
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO agent_events \
                 (id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                db::db_params![
                    entry.id,
                    entry.category,
                    entry.kind,
                    entry.pane_id,
                    entry.workspace_id,
                    entry.surface_id,
                    entry.session_id,
                    entry.payload_json,
                    entry.timestamp,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_agent_events(
        &self,
        category: Option<&str>,
        pane_id: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<AgentEventRow>> {
        let category = category.map(str::to_string);
        let pane_id = pane_id.map(str::to_string);
        let limit = limit.unwrap_or(500).max(1) as i64;
        let sql = match (category.is_some(), pane_id.is_some()) {
            (true, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 AND pane_id = ?2 ORDER BY timestamp DESC LIMIT ?3"
            }
            (true, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE category = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, true) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events WHERE pane_id = ?1 ORDER BY timestamp DESC LIMIT ?2"
            }
            (false, false) => {
                "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
                 FROM agent_events ORDER BY timestamp DESC LIMIT ?1"
            }
        };
        let params = match (category, pane_id) {
            (Some(category), Some(pane_id)) => db::db_params![category, pane_id, limit],
            (Some(category), None) => db::db_params![category, limit],
            (None, Some(pane_id)) => db::db_params![pane_id, limit],
            (None, None) => db::db_params![limit],
        };
        let rows = self.read_db.query(sql, params).await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_agent_event_row_db(row).ok())
            .collect())
    }

    pub(crate) async fn agent_event_recall_matches(
        &self,
        category: &str,
        tokens: &[String],
        limit: usize,
    ) -> Result<Vec<AgentEventRow>> {
        let category = category.to_string();
        let tokens = tokens
            .iter()
            .map(|token| token.trim().to_ascii_lowercase())
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.max(1) as i64;
        let clauses = std::iter::repeat_n(
            "(lower(kind) LIKE ? OR lower(payload_json) LIKE ?)",
            tokens.len(),
        )
        .collect::<Vec<_>>()
        .join(" OR ");
        let sql = format!(
            "SELECT id, category, kind, pane_id, workspace_id, surface_id, session_id, payload_json, timestamp \
             FROM agent_events \
             WHERE category = ? \
               AND ({clauses}) \
             ORDER BY timestamp DESC, id DESC \
             LIMIT ?"
        );
        let mut values = Vec::<db::Value>::new();
        values.push(db::Value::Text(category));
        for token in tokens {
            let pattern = format!("%{token}%");
            values.push(db::Value::Text(pattern.clone()));
            values.push(db::Value::Text(pattern));
        }
        values.push(db::Value::Integer(limit));

        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_agent_event_row_db).collect()
    }

    pub async fn upsert_agent_task(&self, task: &AgentTask) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        upsert_agent_task_exec(&mut *txn, task).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Batched upsert. Wraps N tasks in a single transaction + a single
    /// background-thread roundtrip — the previous per-task `upsert_agent_task`
    /// loop in `persist_tasks` paid the BEGIN/COMMIT and tokio_rusqlite
    /// dispatch cost N times. With N=50–100 tasks this turns into orders of
    /// magnitude fewer fsyncs.
    pub async fn upsert_agent_tasks_batch(&self, tasks: &[AgentTask]) -> Result<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let mut txn = self.conn_db.transaction().await?;
        for task in tasks {
            upsert_agent_task_exec(&mut *txn, task).await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn delete_agent_task(&self, task_id: &str) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        txn.execute(
            "UPDATE agent_tasks SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            db::db_params![task_id, now_ts() as i64],
        )
        .await?;
        embedding_queue::queue_embedding_deletion_exec(
            &mut *txn,
            "agent_task",
            task_id,
            now_ts() as i64,
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_agent_tasks(&self) -> Result<Vec<AgentTask>> {
        self.list_agent_tasks_filtered(&AgentTaskListQuery::default())
            .await
    }

    pub(crate) async fn count_agent_tasks_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<usize> {
        let mut task_sql = "SELECT 1 FROM agent_tasks WHERE deleted_at IS NULL".to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let count_sql = format!("SELECT COUNT(1) FROM ({task_sql})");
        let count = self
            .read_db
            .query_opt(&count_sql, db::Params::Positional(task_values))
            .await?
            .map(|row| row.get::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        Ok(count.max(0) as usize)
    }

    pub(crate) async fn list_agent_tasks_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTask>> {
        load_agent_tasks_from_query(
            &*self.interactive_read_db,
            query,
            AGENT_TASK_SELECT_SQL.to_string(),
            Vec::<db::Value>::new(),
        )
        .await
    }

    /// Task list without `logs` hydration. Used by the Electron `list-tasks`
    /// IPC path: the log table can hold hundreds of MB, and shipping it to
    /// the renderer every 4-5s stalled the SQLite read pool and every other
    /// IPC query.
    pub(crate) async fn list_agent_tasks_filtered_without_logs(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTask>> {
        load_agent_tasks_from_query_with_logs(
            &*self.interactive_read_db,
            query,
            AGENT_TASK_SELECT_SQL.to_string(),
            Vec::<db::Value>::new(),
            false,
        )
        .await
    }

    /// Batched fetch of all tasks belonging to a set of goal_run_ids in a
    /// single SQL round-trip. Replaces the per-goal-run N+1 that
    /// `project_goal_run` was doing in `list_goal_runs_paginated_capped_for_ipc`
    /// (50 goal runs = 50 separate `list_agent_tasks_filtered` calls,
    /// each through the reader pool's tokio thread).
    pub(crate) async fn list_agent_tasks_by_goal_run_ids(
        &self,
        goal_run_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<AgentTask>>> {
        if goal_run_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(goal_run_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "{AGENT_TASK_SELECT_SQL} AND goal_run_id IN ({placeholders}) \
             ORDER BY goal_run_id ASC, created_at DESC"
        );
        let values = goal_run_ids
            .iter()
            .cloned()
            .map(db::Value::Text)
            .collect::<Vec<_>>();
        let rows = self
            .interactive_read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        let mut map: std::collections::HashMap<String, Vec<AgentTask>> =
            std::collections::HashMap::with_capacity(goal_run_ids.len());
        for row in rows.iter() {
            let task = map_agent_task_row(row)?;
            if let Some(id) = task.goal_run_id.clone() {
                map.entry(id).or_default().push(task);
            }
        }
        Ok(map)
    }

    pub(crate) async fn list_agent_tasks_for_parent_thread_subagents(
        &self,
        parent_thread_id: &str,
        status: Option<&str>,
        limit: Option<usize>,
        include_logs: bool,
    ) -> Result<Vec<AgentTask>> {
        let parent_thread_id = parent_thread_id.to_string();
        let status = status
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let query = AgentTaskListQuery {
            status,
            limit,
            ..Default::default()
        };
        load_agent_tasks_from_query_with_logs(
            &*self.read_db,
            &query,
            format!("{AGENT_TASK_SELECT_SQL} AND parent_thread_id = ? AND source = 'subagent'"),
            vec![db::Value::Text(parent_thread_id)],
            include_logs,
        )
        .await
    }

    pub(crate) async fn list_agent_task_refs_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<(String, Option<String>, Option<String>)>> {
        let mut task_sql =
            "SELECT id, thread_id, parent_thread_id FROM agent_tasks WHERE deleted_at IS NULL"
                .to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.get::<String>(0)?,
                    row.get::<Option<String>>(1)?,
                    row.get::<Option<String>>(2)?,
                ))
            })
            .collect()
    }

    pub(crate) async fn list_agent_task_ids_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<String>> {
        let mut task_sql = "SELECT id FROM agent_tasks WHERE deleted_at IS NULL".to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter().map(|row| row.get::<String>(0)).collect()
    }

    pub(crate) async fn list_agent_task_titles_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<(String, String)>> {
        let mut task_sql = "SELECT id, title FROM agent_tasks WHERE deleted_at IS NULL".to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter()
            .map(|row| Ok((row.get::<String>(0)?, row.get::<String>(1)?)))
            .collect()
    }

    pub(crate) async fn list_agent_task_operational_refs_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTaskOperationalRef>> {
        let mut task_sql =
            "SELECT id, title, status, progress, awaiting_approval_id FROM agent_tasks WHERE deleted_at IS NULL"
                .to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter()
            .map(|row| {
                Ok(AgentTaskOperationalRef {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: parse_task_status(&row.get::<String>(2)?),
                    progress: row.get::<i64>(3)? as u8,
                    awaiting_approval_id: row.get(4)?,
                })
            })
            .collect()
    }

    pub(crate) async fn list_agent_task_summary_refs_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTaskSummaryRef>> {
        let mut task_sql =
            "SELECT id, title, status, priority FROM agent_tasks WHERE deleted_at IS NULL"
                .to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter()
            .map(|row| {
                Ok(AgentTaskSummaryRef {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: parse_task_status(&row.get::<String>(2)?),
                    priority: parse_task_priority(&row.get::<String>(3)?),
                })
            })
            .collect()
    }

    pub(crate) async fn list_agent_task_quiet_recovery_refs_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTaskQuietRecoveryRef>> {
        let mut task_sql = "SELECT id, source, status, progress, created_at, started_at, goal_run_id, parent_task_id, thread_id FROM agent_tasks WHERE deleted_at IS NULL"
            .to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter().map(map_agent_task_quiet_recovery_ref).collect()
    }

    pub(crate) async fn list_agent_task_quiet_recovery_refs_for_goal_runs_statuses(
        &self,
        goal_run_ids: &[String],
        statuses: &[String],
    ) -> Result<Vec<AgentTaskQuietRecoveryRef>> {
        let goal_run_ids = goal_run_ids
            .iter()
            .map(|goal_run_id| goal_run_id.trim())
            .filter(|goal_run_id| !goal_run_id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let statuses = statuses
            .iter()
            .map(|status| status.trim())
            .filter(|status| !status.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if goal_run_ids.is_empty() || statuses.is_empty() {
            return Ok(Vec::new());
        }

        let goal_placeholders = std::iter::repeat("?")
            .take(goal_run_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let status_placeholders = std::iter::repeat("?")
            .take(statuses.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT id, source, status, progress, created_at, started_at, goal_run_id, parent_task_id, thread_id \
             FROM agent_tasks \
             WHERE deleted_at IS NULL \
               AND goal_run_id IN ({goal_placeholders}) \
               AND status IN ({status_placeholders}) \
             ORDER BY id ASC"
        );
        let mut values = goal_run_ids
            .into_iter()
            .map(db::Value::Text)
            .collect::<Vec<_>>();
        values.extend(statuses.into_iter().map(db::Value::Text));

        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_agent_task_quiet_recovery_ref).collect()
    }

    pub(crate) async fn list_agent_task_subagent_hierarchy_refs_filtered(
        &self,
        query: &AgentTaskListQuery,
    ) -> Result<Vec<AgentTaskSubagentHierarchyRef>> {
        let mut task_sql =
            "SELECT id, parent_task_id, containment_scope FROM agent_tasks WHERE deleted_at IS NULL"
                .to_string();
        let mut task_values = Vec::<db::Value>::new();
        append_agent_task_query_filters(&mut task_sql, &mut task_values, query);
        if query.order_by_recent_activity_desc {
            task_sql.push_str(
                " ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC",
            );
        } else {
            task_sql.push_str(" ORDER BY id ASC");
        }
        if let Some(limit) = query.limit {
            task_sql.push_str(" LIMIT ?");
            task_values.push(db::Value::Integer(limit.max(1) as i64));
        }

        let rows = self
            .read_db
            .query(&task_sql, db::Params::Positional(task_values))
            .await?;
        rows.iter()
            .map(|row| {
                Ok(AgentTaskSubagentHierarchyRef {
                    id: row.get(0)?,
                    parent_task_id: row.get(1)?,
                    containment_scope: row.get(2)?,
                })
            })
            .collect()
    }

    pub(crate) async fn agent_task_goal_context(
        &self,
        task_id: &str,
    ) -> Result<Option<AgentTaskGoalContext>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT goal_run_id, goal_step_id, session_id, source, parent_task_id
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND id = ?1
                 LIMIT 1",
                db::db_params![task_id],
            )
            .await?;
        row.map(|row| -> anyhow::Result<AgentTaskGoalContext> {
            Ok(AgentTaskGoalContext {
                goal_run_id: row.get(0)?,
                goal_step_id: row.get(1)?,
                session_id: row.get(2)?,
                source: row.get(3)?,
                parent_task_id: row.get(4)?,
            })
        })
        .transpose()
    }

    pub(crate) async fn agent_task_provider_override(
        &self,
        task_id: &str,
    ) -> Result<Option<(String, Option<String>)>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT override_provider, override_model
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND id = ?1
                 LIMIT 1",
                db::db_params![task_id],
            )
            .await?
            .map(|row| -> anyhow::Result<(Option<String>, Option<String>)> {
                Ok((row.get::<Option<String>>(0)?, row.get::<Option<String>>(1)?))
            })
            .transpose()?;
        Ok(row.and_then(|(provider, model)| provider.map(|provider| (provider, model))))
    }

    pub(crate) async fn agent_task_override_system_prompt(
        &self,
        task_id: &str,
    ) -> Result<Option<Option<String>>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT override_system_prompt
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND id = ?1
                 LIMIT 1",
                db::db_params![task_id],
            )
            .await?;
        row.map(|row| row.get::<Option<String>>(0)).transpose()
    }

    pub(crate) async fn latest_agent_task_session_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT session_id
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND thread_id = ?1
                   AND session_id IS NOT NULL
                   AND TRIM(session_id) <> ''
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    pub(crate) async fn has_agent_task_for_thread(&self, thread_id: &str) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT EXISTS(
                    SELECT 1
                    FROM agent_tasks
                    WHERE deleted_at IS NULL
                      AND thread_id = ?1
                 )",
                db::db_params![thread_id],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub(crate) async fn has_agent_task_id(&self, task_id: &str) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT EXISTS(
                    SELECT 1
                    FROM agent_tasks
                    WHERE deleted_at IS NULL
                      AND id = ?1
                 )",
                db::db_params![task_id],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub(crate) async fn latest_agent_task_approval_id_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT awaiting_approval_id
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND thread_id = ?1
                   AND awaiting_approval_id IS NOT NULL
                   AND TRIM(awaiting_approval_id) <> ''
                 ORDER BY COALESCE(completed_at, started_at, created_at) DESC, created_at DESC
                 LIMIT 1",
                db::db_params![thread_id],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    pub(crate) async fn has_agent_task_pending_approval(&self) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT COUNT(1)
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND awaiting_approval_id IS NOT NULL
                   AND TRIM(awaiting_approval_id) <> ''",
                db::Params::None,
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? > 0,
            None => false,
        })
    }

    /// Returns the `(task_id, thread_id, approval_id, approval_expires_at)` of
    /// every task whose operator-approval deadline has passed. Used by the
    /// heartbeat L2-timeout watcher to dispatch external (L3) escalation
    /// notifications without loading the full row set.
    ///
    /// We intentionally return only the minimum fields the dispatcher needs
    /// rather than the full `AgentTask` — the heartbeat runs every tick, and
    /// hydrating the full row would be wasteful for what is usually an empty
    /// or small result.
    pub(crate) async fn list_tasks_past_approval_deadline(
        &self,
        now_ms: u64,
    ) -> Result<Vec<(String, Option<String>, String, u64)>> {
        let now = now_ms as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, thread_id, awaiting_approval_id, approval_expires_at
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND status = 'awaiting_approval'
                   AND awaiting_approval_id IS NOT NULL
                   AND TRIM(awaiting_approval_id) <> ''
                   AND approval_expires_at IS NOT NULL
                   AND approval_expires_at < ?1
                 ORDER BY approval_expires_at ASC",
                db::db_params![now],
            )
            .await?;
        rows.iter()
            .map(|row| {
                let id: String = row.get(0)?;
                let thread_id: Option<String> = row.get(1)?;
                let approval_id: String = row.get(2)?;
                let expires_at: i64 = row.get(3)?;
                Ok((id, thread_id, approval_id, expires_at.max(0) as u64))
            })
            .collect()
    }

    pub(crate) async fn has_awaiting_approval_task_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT COUNT(1)
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND status = 'awaiting_approval'
                   AND (thread_id = ?1 OR parent_thread_id = ?1)",
                db::db_params![thread_id],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? > 0,
            None => false,
        })
    }

    pub(crate) async fn pending_agent_task_approval_command(
        &self,
        approval_id: &str,
    ) -> Result<Option<String>> {
        const APPROVAL_REASON_PREFIX: &str = "waiting for operator approval: ";

        let row = self
            .read_db
            .query_opt(
                "SELECT command, blocked_reason
                 FROM agent_tasks
                 WHERE deleted_at IS NULL
                   AND awaiting_approval_id = ?1
                 ORDER BY created_at DESC
                 LIMIT 1",
                db::db_params![approval_id],
            )
            .await?
            .map(|row| -> anyhow::Result<(Option<String>, Option<String>)> {
                Ok((row.get::<Option<String>>(0)?, row.get::<Option<String>>(1)?))
            })
            .transpose()?;
        let Some((command, blocked_reason)) = row else {
            return Ok(None);
        };
        if let Some(command) = command.map(|value| value.trim().to_string()) {
            if !command.is_empty() {
                return Ok(Some(command));
            }
        }
        Ok(blocked_reason
            .as_deref()
            .and_then(|reason| reason.strip_prefix(APPROVAL_REASON_PREFIX))
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(ToOwned::to_owned))
    }
}
