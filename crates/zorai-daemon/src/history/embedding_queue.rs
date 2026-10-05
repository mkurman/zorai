use super::*;
use std::future::Future;

const CLAIM_STALE_AFTER_SECS: i64 = 300;
const EMBEDDING_JOB_CHUNK_MAX_CHARS: usize = 6_000;
const EMBEDDING_WRITER_LOCK_RETRY_BASE_DELAY_MS: u64 = 250;
const EMBEDDING_WRITER_LOCK_RETRY_MAX_DELAY_MS: u64 = 2_000;
const EMBEDDING_WRITER_LOCK_RETRY_WINDOW_SECS: u64 = 15;

#[derive(Debug, Clone)]
pub(crate) struct EmbeddingJobInput {
    pub source_kind: String,
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub source_timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingJob {
    pub source_kind: String,
    pub source_id: String,
    pub chunk_id: String,
    pub content_hash: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub source_timestamp: i64,
    pub attempts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbeddingDeletion {
    pub source_kind: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticBackfillResult {
    pub messages_queued: u64,
    pub tasks_queued: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndexStatus {
    pub queued_jobs: u64,
    pub pending_for_model: u64,
    pub completed_for_model: u64,
    pub queued_deletions: u64,
    pub failed_jobs: u64,
    pub failed_deletions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndexRepairStateReset {
    pub cleared_completions: u64,
    pub cleared_deletions: u64,
    pub reset_failed_jobs: u64,
}

fn content_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn chunk_embedding_body(body: &str) -> Vec<String> {
    let mut remaining = body.trim();
    let mut chunks = Vec::new();
    while !remaining.is_empty() {
        let mut limit = remaining.len();
        let mut chars = 0usize;
        for (idx, _) in remaining.char_indices() {
            if chars == EMBEDDING_JOB_CHUNK_MAX_CHARS {
                limit = idx;
                break;
            }
            chars += 1;
        }
        if limit == remaining.len() {
            chunks.push(remaining.to_string());
            break;
        }

        let prefix = &remaining[..limit];
        let split_at = prefix
            .char_indices()
            .rev()
            .find(|(idx, ch)| *idx > 0 && ch.is_whitespace())
            .map(|(idx, _)| idx)
            .unwrap_or(limit);
        let chunk = remaining[..split_at].trim_end();
        if !chunk.is_empty() {
            chunks.push(chunk.to_string());
        }
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

fn chunk_id_for(base_chunk_id: &str, index: usize) -> String {
    if base_chunk_id == "0" {
        index.to_string()
    } else {
        format!("{base_chunk_id}:{index}")
    }
}

fn is_retryable_embedding_writer_lock(error: &anyhow::Error) -> bool {
    // Best-effort: if the typed cause survived in the error chain, match the
    // SQLite primary code directly (5 = BUSY, 6 = LOCKED).
    for cause in error.chain() {
        if let Some(rusqlite::Error::SqliteFailure(code, _)) =
            cause.downcast_ref::<rusqlite::Error>()
        {
            let primary_code = code.extended_code & 0xff;
            if primary_code == 5 || primary_code == 6 {
                return true;
            }
        }
        if let Some(tokio_rusqlite::Error::Rusqlite(rusqlite::Error::SqliteFailure(code, _))) =
            cause.downcast_ref::<tokio_rusqlite::Error>()
        {
            let primary_code = code.extended_code & 0xff;
            if primary_code == 5 || primary_code == 6 {
                return true;
            }
        }
    }
    // The facade collapses errors to their `Display` string, which still
    // carries SQLite's lock messages, so fall back to a text match.
    let text = format!("{error:?}").to_ascii_lowercase();
    text.contains("database is locked")
        || text.contains("database table is locked")
        || text.contains("database schema is locked")
        || text.contains("database busy")
}

fn delete_stale_embedding_chunks(
    connection: &Connection,
    source_kind: &str,
    source_id: &str,
    active_chunk_ids: &[String],
) -> rusqlite::Result<()> {
    if active_chunk_ids.is_empty() {
        connection.execute(
            "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2",
            params![source_kind, source_id],
        )?;
        connection.execute(
            "DELETE FROM embedding_job_completions
             WHERE source_kind = ?1 AND source_id = ?2",
            params![source_kind, source_id],
        )?;
        return Ok(());
    }

    let placeholders = std::iter::repeat("?")
        .take(active_chunk_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql_params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + active_chunk_ids.len());
    sql_params.push(&source_kind);
    sql_params.push(&source_id);
    for id in active_chunk_ids {
        sql_params.push(id);
    }

    connection.execute(
        &format!(
            "DELETE FROM embedding_jobs
             WHERE source_kind = ?1 AND source_id = ?2
               AND chunk_id NOT IN ({placeholders})"
        ),
        rusqlite::params_from_iter(sql_params.iter()),
    )?;
    connection.execute(
        &format!(
            "DELETE FROM embedding_job_completions
             WHERE source_kind = ?1 AND source_id = ?2
               AND chunk_id NOT IN ({placeholders})"
        ),
        rusqlite::params_from_iter(sql_params.iter()),
    )?;
    Ok(())
}

fn select_claimable_embedding_jobs(
    connection: &Connection,
    embedding_model: &str,
    dimensions: i64,
    stale_before: i64,
    limit: i64,
) -> rusqlite::Result<Vec<EmbeddingJob>> {
    let mut stmt = connection.prepare(
        "SELECT
            source_kind,
            source_id,
            chunk_id,
            content_hash,
            title,
            body,
            workspace_id,
            thread_id,
            agent_id,
            source_timestamp,
            attempts
        FROM embedding_jobs job
        WHERE (job.claimed_at IS NULL OR job.claimed_at <= ?3)
          AND NOT EXISTS (
            SELECT 1
            FROM embedding_job_completions completion
            WHERE completion.source_kind = job.source_kind
              AND completion.source_id = job.source_id
              AND completion.chunk_id = job.chunk_id
              AND completion.content_hash = job.content_hash
              AND completion.embedding_model = ?1
              AND completion.dimensions = ?2
          )
        ORDER BY job.updated_at ASC, job.chunk_id ASC
        LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![embedding_model, dimensions, stale_before, limit],
        |row| {
            Ok(EmbeddingJob {
                source_kind: row.get(0)?,
                source_id: row.get(1)?,
                chunk_id: row.get(2)?,
                content_hash: row.get(3)?,
                title: row.get(4)?,
                body: row.get(5)?,
                workspace_id: row.get(6)?,
                thread_id: row.get(7)?,
                agent_id: row.get(8)?,
                source_timestamp: row.get(9)?,
                attempts: row.get(10)?,
            })
        },
    )?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
}

async fn select_claimable_embedding_jobs_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    embedding_model: &str,
    dimensions: i64,
    stale_before: i64,
    limit: i64,
) -> Result<Vec<EmbeddingJob>> {
    let rows = exec
        .query(
            "SELECT
                source_kind,
                source_id,
                chunk_id,
                content_hash,
                title,
                body,
                workspace_id,
                thread_id,
                agent_id,
                source_timestamp,
                attempts
            FROM embedding_jobs job
            WHERE (job.claimed_at IS NULL OR job.claimed_at <= ?3)
              AND NOT EXISTS (
                SELECT 1
                FROM embedding_job_completions completion
                WHERE completion.source_kind = job.source_kind
                  AND completion.source_id = job.source_id
                  AND completion.chunk_id = job.chunk_id
                  AND completion.content_hash = job.content_hash
                  AND completion.embedding_model = ?1
                  AND completion.dimensions = ?2
              )
            ORDER BY job.updated_at ASC, job.chunk_id ASC
            LIMIT ?4",
            db::db_params![embedding_model, dimensions, stale_before, limit],
        )
        .await?;
    rows.iter()
        .map(|row| {
            Ok(EmbeddingJob {
                source_kind: row.get(0)?,
                source_id: row.get(1)?,
                chunk_id: row.get(2)?,
                content_hash: row.get(3)?,
                title: row.get(4)?,
                body: row.get(5)?,
                workspace_id: row.get(6)?,
                thread_id: row.get(7)?,
                agent_id: row.get(8)?,
                source_timestamp: row.get(9)?,
                attempts: row.get(10)?,
            })
        })
        .collect()
}

pub(super) fn enqueue_embedding_job_on_connection(
    connection: &Connection,
    job: &EmbeddingJobInput,
    now: i64,
) -> rusqlite::Result<()> {
    let chunks = chunk_embedding_body(&job.body);
    if chunks.is_empty() {
        return Ok(());
    }
    let title = job.title.trim();
    let active_chunk_ids = chunks
        .iter()
        .enumerate()
        .map(|(index, _)| chunk_id_for(&job.chunk_id, index))
        .collect::<Vec<_>>();
    delete_stale_embedding_chunks(
        connection,
        &job.source_kind,
        &job.source_id,
        &active_chunk_ids,
    )?;
    for (body, chunk_id) in chunks.iter().zip(active_chunk_ids) {
        let content_hash = content_hash(body);
        connection.execute(
            "INSERT INTO embedding_jobs (
                source_kind,
                source_id,
                chunk_id,
                content_hash,
                title,
                body,
                workspace_id,
                thread_id,
                agent_id,
                source_timestamp,
                queued_at,
                updated_at,
                claimed_at,
                attempts,
                last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL, 0, NULL)
            ON CONFLICT(source_kind, source_id, chunk_id) DO UPDATE SET
                content_hash = excluded.content_hash,
                title = excluded.title,
                body = excluded.body,
                workspace_id = excluded.workspace_id,
                thread_id = excluded.thread_id,
                agent_id = excluded.agent_id,
                source_timestamp = excluded.source_timestamp,
                updated_at = excluded.updated_at,
                claimed_at = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.claimed_at
                    ELSE NULL
                END,
                attempts = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.attempts
                    ELSE 0
                END,
                last_error = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.last_error
                    ELSE NULL
                END",
            params![
                job.source_kind,
                job.source_id,
                chunk_id,
                content_hash,
                title,
                body,
                job.workspace_id,
                job.thread_id,
                job.agent_id,
                job.source_timestamp,
                now,
            ],
        )?;
    }
    Ok(())
}

pub(super) fn enqueue_message_embedding_job(
    connection: &Connection,
    message: &AgentDbMessage,
    workspace_id: Option<&str>,
    now: i64,
) -> rusqlite::Result<()> {
    if message.content.trim().is_empty() {
        return queue_embedding_deletion_on_connection(
            connection,
            "agent_message",
            &message.id,
            now,
        );
    }
    enqueue_embedding_job_on_connection(
        connection,
        &EmbeddingJobInput {
            source_kind: "agent_message".to_string(),
            source_id: message.id.clone(),
            chunk_id: "0".to_string(),
            title: message.role.clone(),
            body: message.content.clone(),
            workspace_id: workspace_id.map(str::to_string),
            thread_id: Some(message.thread_id.clone()),
            agent_id: None,
            source_timestamp: message.created_at,
        },
        now,
    )
}

pub(super) fn enqueue_task_embedding_job(
    connection: &Connection,
    task: &AgentTask,
    now: i64,
) -> rusqlite::Result<()> {
    let body = match (task.title.trim(), task.description.trim()) {
        ("", "") => String::new(),
        (title, "") => title.to_string(),
        ("", description) => description.to_string(),
        (title, description) => format!("{title}\n\n{description}"),
    };
    if body.trim().is_empty() {
        return queue_embedding_deletion_on_connection(connection, "agent_task", &task.id, now);
    }
    enqueue_embedding_job_on_connection(
        connection,
        &EmbeddingJobInput {
            source_kind: "agent_task".to_string(),
            source_id: task.id.clone(),
            chunk_id: "0".to_string(),
            title: task.title.clone(),
            body,
            workspace_id: None,
            thread_id: task.thread_id.clone(),
            agent_id: None,
            source_timestamp: task.created_at as i64,
        },
        now,
    )
}

pub(super) fn queue_embedding_deletion_on_connection(
    connection: &Connection,
    source_kind: &str,
    source_id: &str,
    now: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2",
        params![source_kind, source_id],
    )?;
    let completed_rows = connection.execute(
        "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id = ?2",
        params![source_kind, source_id],
    )?;
    if completed_rows == 0 {
        return Ok(());
    }
    connection.execute(
        "INSERT INTO embedding_deletions (
            source_kind,
            source_id,
            queued_at,
            claimed_at,
            attempts,
            last_error
        ) VALUES (?1, ?2, ?3, NULL, 0, NULL)
        ON CONFLICT(source_kind, source_id) DO UPDATE SET
            queued_at = excluded.queued_at,
            claimed_at = NULL,
            last_error = NULL",
        params![source_kind, source_id, now],
    )?;
    Ok(())
}

/// Unlike the per-id variant, this unconditionally inserts a deletion row
/// for every source_id rather than skipping ids that had no completions —
/// the downstream pipeline tolerates extra rows.
pub(super) fn queue_embedding_deletions_on_connection(
    connection: &Connection,
    source_kind: &str,
    source_ids: &[String],
    now: i64,
) -> rusqlite::Result<()> {
    if source_ids.is_empty() {
        return Ok(());
    }
    let placeholders = (0..source_ids.len())
        .map(|i| format!("?{}", 2 + i))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql_params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + source_ids.len());
    sql_params.push(&source_kind);
    for id in source_ids {
        sql_params.push(id);
    }

    connection.execute(
        &format!(
            "DELETE FROM embedding_jobs
             WHERE source_kind = ?1 AND source_id IN ({placeholders})"
        ),
        rusqlite::params_from_iter(sql_params.iter()),
    )?;
    connection.execute(
        &format!(
            "DELETE FROM embedding_job_completions
             WHERE source_kind = ?1 AND source_id IN ({placeholders})"
        ),
        rusqlite::params_from_iter(sql_params.iter()),
    )?;

    // Bulk INSERT with multi-row VALUES to make this a single statement.
    let value_rows = (0..source_ids.len())
        .map(|i| format!("(?1, ?{}, ?{}, NULL, 0, NULL)", 2 + i, 2 + source_ids.len()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut insert_params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + source_ids.len());
    insert_params.push(&source_kind);
    for id in source_ids {
        insert_params.push(id);
    }
    insert_params.push(&now);

    connection.execute(
        &format!(
            "INSERT INTO embedding_deletions (
                source_kind,
                source_id,
                queued_at,
                claimed_at,
                attempts,
                last_error
            ) VALUES {value_rows}
            ON CONFLICT(source_kind, source_id) DO UPDATE SET
                queued_at = excluded.queued_at,
                claimed_at = NULL,
                last_error = NULL"
        ),
        rusqlite::params_from_iter(insert_params.iter()),
    )?;
    Ok(())
}

// ---- Facade (db::DbExecutor) variants of the shared embedding helpers. Used by
// call sites already migrated onto the async facade; the `*_on_connection`
// rusqlite versions above remain until every caller has moved over. ----

async fn delete_stale_embedding_chunks_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    source_kind: &str,
    source_id: &str,
    active_chunk_ids: &[String],
) -> Result<()> {
    if active_chunk_ids.is_empty() {
        exec.execute(
            "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2",
            db::db_params![source_kind, source_id],
        )
        .await?;
        exec.execute(
            "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id = ?2",
            db::db_params![source_kind, source_id],
        )
        .await?;
        return Ok(());
    }

    let placeholders = std::iter::repeat("?")
        .take(active_chunk_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let mut values = vec![
        db::Value::Text(source_kind.to_string()),
        db::Value::Text(source_id.to_string()),
    ];
    values.extend(
        active_chunk_ids
            .iter()
            .map(|id| db::Value::Text(id.clone())),
    );
    exec.execute(
        &format!(
            "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id NOT IN ({placeholders})"
        ),
        db::Params::Positional(values.clone()),
    )
    .await?;
    exec.execute(
        &format!(
            "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id NOT IN ({placeholders})"
        ),
        db::Params::Positional(values),
    )
    .await?;
    Ok(())
}

pub(super) async fn enqueue_embedding_job_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    job: &EmbeddingJobInput,
    now: i64,
) -> Result<()> {
    let chunks = chunk_embedding_body(&job.body);
    if chunks.is_empty() {
        return Ok(());
    }
    let title = job.title.trim();
    let active_chunk_ids = chunks
        .iter()
        .enumerate()
        .map(|(index, _)| chunk_id_for(&job.chunk_id, index))
        .collect::<Vec<_>>();
    delete_stale_embedding_chunks_exec(exec, &job.source_kind, &job.source_id, &active_chunk_ids)
        .await?;
    for (body, chunk_id) in chunks.iter().zip(active_chunk_ids) {
        let content_hash = content_hash(body);
        exec.execute(
            "INSERT INTO embedding_jobs (
                source_kind,
                source_id,
                chunk_id,
                content_hash,
                title,
                body,
                workspace_id,
                thread_id,
                agent_id,
                source_timestamp,
                queued_at,
                updated_at,
                claimed_at,
                attempts,
                last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL, 0, NULL)
            ON CONFLICT(source_kind, source_id, chunk_id) DO UPDATE SET
                content_hash = excluded.content_hash,
                title = excluded.title,
                body = excluded.body,
                workspace_id = excluded.workspace_id,
                thread_id = excluded.thread_id,
                agent_id = excluded.agent_id,
                source_timestamp = excluded.source_timestamp,
                updated_at = excluded.updated_at,
                claimed_at = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.claimed_at
                    ELSE NULL
                END,
                attempts = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.attempts
                    ELSE 0
                END,
                last_error = CASE
                    WHEN embedding_jobs.content_hash = excluded.content_hash THEN embedding_jobs.last_error
                    ELSE NULL
                END",
            db::db_params![
                job.source_kind.clone(),
                job.source_id.clone(),
                chunk_id,
                content_hash,
                title,
                body,
                job.workspace_id.clone(),
                job.thread_id.clone(),
                job.agent_id.clone(),
                job.source_timestamp,
                now,
            ],
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn queue_embedding_deletion_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    source_kind: &str,
    source_id: &str,
    now: i64,
) -> Result<()> {
    exec.execute(
        "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id = ?2",
        db::db_params![source_kind, source_id],
    )
    .await?;
    let completed_rows = exec
        .execute(
            "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id = ?2",
            db::db_params![source_kind, source_id],
        )
        .await?;
    if completed_rows == 0 {
        return Ok(());
    }
    exec.execute(
        "INSERT INTO embedding_deletions (
            source_kind,
            source_id,
            queued_at,
            claimed_at,
            attempts,
            last_error
        ) VALUES (?1, ?2, ?3, NULL, 0, NULL)
        ON CONFLICT(source_kind, source_id) DO UPDATE SET
            queued_at = excluded.queued_at,
            claimed_at = NULL,
            last_error = NULL",
        db::db_params![source_kind, source_id, now],
    )
    .await?;
    Ok(())
}

pub(super) async fn queue_embedding_deletions_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    source_kind: &str,
    source_ids: &[String],
    now: i64,
) -> Result<()> {
    if source_ids.is_empty() {
        return Ok(());
    }
    let placeholders = (0..source_ids.len())
        .map(|i| format!("?{}", 2 + i))
        .collect::<Vec<_>>()
        .join(", ");
    let mut delete_values = vec![db::Value::Text(source_kind.to_string())];
    delete_values.extend(source_ids.iter().map(|id| db::Value::Text(id.clone())));
    exec.execute(
        &format!(
            "DELETE FROM embedding_jobs WHERE source_kind = ?1 AND source_id IN ({placeholders})"
        ),
        db::Params::Positional(delete_values.clone()),
    )
    .await?;
    exec.execute(
        &format!(
            "DELETE FROM embedding_job_completions WHERE source_kind = ?1 AND source_id IN ({placeholders})"
        ),
        db::Params::Positional(delete_values),
    )
    .await?;

    let value_rows = (0..source_ids.len())
        .map(|i| format!("(?1, ?{}, ?{}, NULL, 0, NULL)", 2 + i, 2 + source_ids.len()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut insert_values = vec![db::Value::Text(source_kind.to_string())];
    insert_values.extend(source_ids.iter().map(|id| db::Value::Text(id.clone())));
    insert_values.push(db::Value::Integer(now));
    exec.execute(
        &format!(
            "INSERT INTO embedding_deletions (
                source_kind,
                source_id,
                queued_at,
                claimed_at,
                attempts,
                last_error
            ) VALUES {value_rows}
            ON CONFLICT(source_kind, source_id) DO UPDATE SET
                queued_at = excluded.queued_at,
                claimed_at = NULL,
                last_error = NULL"
        ),
        db::Params::Positional(insert_values),
    )
    .await?;
    Ok(())
}

pub(super) async fn enqueue_message_embedding_job_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    message: &AgentDbMessage,
    workspace_id: Option<&str>,
    now: i64,
) -> Result<()> {
    if message.content.trim().is_empty() {
        return queue_embedding_deletion_exec(exec, "agent_message", &message.id, now).await;
    }
    enqueue_embedding_job_exec(
        exec,
        &EmbeddingJobInput {
            source_kind: "agent_message".to_string(),
            source_id: message.id.clone(),
            chunk_id: "0".to_string(),
            title: message.role.clone(),
            body: message.content.clone(),
            workspace_id: workspace_id.map(str::to_string),
            thread_id: Some(message.thread_id.clone()),
            agent_id: None,
            source_timestamp: message.created_at,
        },
        now,
    )
    .await
}

pub(super) async fn enqueue_task_embedding_job_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    task: &AgentTask,
    now: i64,
) -> Result<()> {
    let body = match (task.title.trim(), task.description.trim()) {
        ("", "") => String::new(),
        (title, "") => title.to_string(),
        ("", description) => description.to_string(),
        (title, description) => format!("{title}\n\n{description}"),
    };
    if body.trim().is_empty() {
        return queue_embedding_deletion_exec(exec, "agent_task", &task.id, now).await;
    }
    enqueue_embedding_job_exec(
        exec,
        &EmbeddingJobInput {
            source_kind: "agent_task".to_string(),
            source_id: task.id.clone(),
            chunk_id: "0".to_string(),
            title: task.title.clone(),
            body,
            workspace_id: None,
            thread_id: task.thread_id.clone(),
            agent_id: None,
            source_timestamp: task.created_at as i64,
        },
        now,
    )
    .await
}

impl HistoryStore {
    async fn call_embedding_writer_with_retry<R, F, Fut>(
        &self,
        operation: &'static str,
        mut make_call: F,
    ) -> Result<R>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<R>>,
    {
        let started = std::time::Instant::now();
        let mut attempt = 0usize;
        loop {
            match make_call().await {
                Ok(value) => return Ok(value),
                Err(error) if is_retryable_embedding_writer_lock(&error) => {
                    let elapsed = started.elapsed();
                    if elapsed
                        >= std::time::Duration::from_secs(EMBEDDING_WRITER_LOCK_RETRY_WINDOW_SECS)
                    {
                        return Err(error);
                    }
                    attempt += 1;
                    let delay_ms = EMBEDDING_WRITER_LOCK_RETRY_BASE_DELAY_MS
                        .saturating_mul(1_u64 << (attempt - 1))
                        .min(EMBEDDING_WRITER_LOCK_RETRY_MAX_DELAY_MS);
                    tracing::warn!(
                        operation,
                        attempt,
                        delay_ms,
                        elapsed_ms = elapsed.as_millis() as u64,
                        error = %error,
                        "embedding writer hit SQLite lock; retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub(crate) async fn enqueue_embedding_job(&self, job: EmbeddingJobInput) -> Result<()> {
        self.call_embedding_writer_with_retry("enqueue_embedding_job", || async {
            enqueue_embedding_job_exec(
                &mut db::ConnExecutor(&*self.embedding_writer_db),
                &job,
                now_ts() as i64,
            )
            .await
        })
        .await
    }

    pub(crate) async fn claim_embedding_jobs(
        &self,
        embedding_model: &str,
        dimensions: u32,
        limit: usize,
    ) -> Result<Vec<EmbeddingJob>> {
        let embedding_model = embedding_model.trim().to_string();
        if embedding_model.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(512) as i64;
        let dimensions = dimensions as i64;
        self.call_embedding_writer_with_retry("claim_embedding_jobs", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let now = now_ts() as i64;
            let stale_before = now.saturating_sub(CLAIM_STALE_AFTER_SECS);
            let mut jobs = select_claimable_embedding_jobs_exec(
                &mut *txn,
                &embedding_model,
                dimensions,
                stale_before,
                limit,
            )
            .await?;
            let oversized_jobs = jobs
                .iter()
                .filter(|job| chunk_embedding_body(&job.body).len() > 1)
                .cloned()
                .collect::<Vec<_>>();
            if !oversized_jobs.is_empty() {
                for job in oversized_jobs {
                    enqueue_embedding_job_exec(
                        &mut *txn,
                        &EmbeddingJobInput {
                            source_kind: job.source_kind,
                            source_id: job.source_id,
                            chunk_id: job.chunk_id,
                            title: job.title,
                            body: job.body,
                            workspace_id: job.workspace_id,
                            thread_id: job.thread_id,
                            agent_id: job.agent_id,
                            source_timestamp: job.source_timestamp,
                        },
                        now,
                    )
                    .await?;
                }
                jobs = select_claimable_embedding_jobs_exec(
                    &mut *txn,
                    &embedding_model,
                    dimensions,
                    stale_before,
                    limit,
                )
                .await?;
            }

            for job in &jobs {
                txn.execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = ?4, attempts = attempts + 1
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    db::db_params![
                        job.source_kind.clone(),
                        job.source_id.clone(),
                        job.chunk_id.clone(),
                        now
                    ],
                )
                .await?;
            }
            txn.commit().await?;
            Ok(jobs)
        })
        .await
    }

    pub(crate) async fn complete_embedding_job(
        &self,
        job: &EmbeddingJob,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<()> {
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;
        self.call_embedding_writer_with_retry("complete_embedding_job", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            txn.execute(
                "INSERT OR REPLACE INTO embedding_job_completions (
                    source_kind,
                    source_id,
                    chunk_id,
                    content_hash,
                    embedding_model,
                    dimensions,
                    completed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    job.source_kind.clone(),
                    job.source_id.clone(),
                    job.chunk_id.clone(),
                    job.content_hash.clone(),
                    embedding_model.clone(),
                    dimensions,
                    now_ts() as i64,
                ],
            )
            .await?;
            txn.execute(
                "UPDATE embedding_jobs
                 SET claimed_at = NULL, last_error = NULL
                 WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                db::db_params![
                    job.source_kind.clone(),
                    job.source_id.clone(),
                    job.chunk_id.clone()
                ],
            )
            .await?;
            txn.commit().await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn fail_embedding_job(&self, job: &EmbeddingJob, error: &str) -> Result<()> {
        let error = error.chars().take(2000).collect::<String>();
        self.call_embedding_writer_with_retry("fail_embedding_job", || async {
            self.embedding_writer_db
                .execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = ?4, last_error = ?5
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    db::db_params![
                        job.source_kind.clone(),
                        job.source_id.clone(),
                        job.chunk_id.clone(),
                        now_ts() as i64,
                        error.clone()
                    ],
                )
                .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn complete_embedding_jobs(
        &self,
        jobs: &[EmbeddingJob],
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<()> {
        if jobs.is_empty() {
            return Ok(());
        }
        let jobs = jobs.to_vec();
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;
        self.call_embedding_writer_with_retry("complete_embedding_jobs", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let now = now_ts() as i64;
            for job in &jobs {
                txn.execute(
                    "INSERT OR REPLACE INTO embedding_job_completions (
                        source_kind,
                        source_id,
                        chunk_id,
                        content_hash,
                        embedding_model,
                        dimensions,
                        completed_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    db::db_params![
                        job.source_kind.clone(),
                        job.source_id.clone(),
                        job.chunk_id.clone(),
                        job.content_hash.clone(),
                        embedding_model.clone(),
                        dimensions,
                        now,
                    ],
                )
                .await?;
                txn.execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = NULL, last_error = NULL
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    db::db_params![
                        job.source_kind.clone(),
                        job.source_id.clone(),
                        job.chunk_id.clone()
                    ],
                )
                .await?;
            }
            txn.commit().await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn fail_embedding_jobs(
        &self,
        jobs: &[EmbeddingJob],
        error: &str,
    ) -> Result<()> {
        if jobs.is_empty() {
            return Ok(());
        }
        let jobs = jobs.to_vec();
        let error = error.chars().take(2000).collect::<String>();
        self.call_embedding_writer_with_retry("fail_embedding_jobs", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let now = now_ts() as i64;
            for job in &jobs {
                txn.execute(
                    "UPDATE embedding_jobs
                     SET claimed_at = ?4, last_error = ?5
                     WHERE source_kind = ?1 AND source_id = ?2 AND chunk_id = ?3",
                    db::db_params![
                        job.source_kind.clone(),
                        job.source_id.clone(),
                        job.chunk_id.clone(),
                        now,
                        error.clone()
                    ],
                )
                .await?;
            }
            txn.commit().await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn claim_embedding_deletions(
        &self,
        limit: usize,
    ) -> Result<Vec<EmbeddingDeletion>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.min(512) as i64;
        self.call_embedding_writer_with_retry("claim_embedding_deletions", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let now = now_ts() as i64;
            let stale_before = now.saturating_sub(CLAIM_STALE_AFTER_SECS);
            let deletions = txn
                .query(
                    "SELECT source_kind, source_id
                     FROM embedding_deletions
                     WHERE claimed_at IS NULL OR claimed_at <= ?1
                     ORDER BY queued_at ASC
                     LIMIT ?2",
                    db::db_params![stale_before, limit],
                )
                .await?
                .iter()
                .map(|row| {
                    Ok(EmbeddingDeletion {
                        source_kind: row.get(0)?,
                        source_id: row.get(1)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            for deletion in &deletions {
                txn.execute(
                    "UPDATE embedding_deletions
                     SET claimed_at = ?3, attempts = attempts + 1
                     WHERE source_kind = ?1 AND source_id = ?2",
                    db::db_params![
                        deletion.source_kind.clone(),
                        deletion.source_id.clone(),
                        now
                    ],
                )
                .await?;
            }
            txn.commit().await?;
            Ok(deletions)
        })
        .await
    }

    pub(crate) async fn complete_embedding_deletion(
        &self,
        deletion: &EmbeddingDeletion,
    ) -> Result<()> {
        self.call_embedding_writer_with_retry("complete_embedding_deletion", || async {
            self.embedding_writer_db
                .execute(
                    "DELETE FROM embedding_deletions WHERE source_kind = ?1 AND source_id = ?2",
                    db::db_params![deletion.source_kind.clone(), deletion.source_id.clone()],
                )
                .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn fail_embedding_deletion(
        &self,
        deletion: &EmbeddingDeletion,
        error: &str,
    ) -> Result<()> {
        let error = error.chars().take(2000).collect::<String>();
        self.call_embedding_writer_with_retry("fail_embedding_deletion", || async {
            self.embedding_writer_db
                .execute(
                    "UPDATE embedding_deletions
                     SET claimed_at = ?3, last_error = ?4
                     WHERE source_kind = ?1 AND source_id = ?2",
                    db::db_params![
                        deletion.source_kind.clone(),
                        deletion.source_id.clone(),
                        now_ts() as i64,
                        error.clone()
                    ],
                )
                .await?;
            Ok(())
        })
        .await
    }

    pub async fn queue_semantic_backfill(
        &self,
        limit: Option<usize>,
    ) -> Result<SemanticBackfillResult> {
        let limit = limit.unwrap_or(usize::MAX).max(1);
        self.call_embedding_writer_with_retry("queue_semantic_backfill", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let now = now_ts() as i64;
            let mut remaining = limit;
            let message_sql = if remaining == usize::MAX {
                "SELECT message.id, message.thread_id, message.created_at, message.role, message.content, message.provider, message.model, message.input_tokens, message.output_tokens, message.total_tokens, message.cost_usd, message.reasoning, NULL, NULL, thread.workspace_id
                 FROM agent_messages message
                 LEFT JOIN agent_threads thread ON thread.id = message.thread_id
                 WHERE message.deleted_at IS NULL AND trim(message.content) <> ''
                 ORDER BY message.created_at ASC"
            } else {
                "SELECT message.id, message.thread_id, message.created_at, message.role, message.content, message.provider, message.model, message.input_tokens, message.output_tokens, message.total_tokens, message.cost_usd, message.reasoning, NULL, NULL, thread.workspace_id
                 FROM agent_messages message
                 LEFT JOIN agent_threads thread ON thread.id = message.thread_id
                 WHERE message.deleted_at IS NULL AND trim(message.content) <> ''
                 ORDER BY message.created_at ASC
                 LIMIT ?1"
            };
            let message_params = if remaining == usize::MAX {
                db::Params::None
            } else {
                db::db_params![remaining as i64]
            };
            let messages = txn
                .query(message_sql, message_params)
                .await?
                .iter()
                .map(|row| {
                    Ok((map_agent_message_db(row)?, row.get::<Option<String>>(14)?))
                })
                .collect::<Result<Vec<(AgentDbMessage, Option<String>)>>>()?;
            let messages_queued = messages.len() as u64;
            remaining = remaining.saturating_sub(messages.len());
            for (message, workspace_id) in &messages {
                enqueue_message_embedding_job_exec(
                    &mut *txn,
                    message,
                    workspace_id.as_deref(),
                    now,
                )
                .await?;
            }

            let tasks = if remaining == 0 {
                Vec::new()
            } else {
                let task_sql = if remaining == usize::MAX {
                    "SELECT id, title, description, created_at, thread_id
                     FROM agent_tasks
                     WHERE deleted_at IS NULL AND trim(title || char(10) || description) <> ''
                     ORDER BY created_at ASC"
                } else {
                    "SELECT id, title, description, created_at, thread_id
                     FROM agent_tasks
                     WHERE deleted_at IS NULL AND trim(title || char(10) || description) <> ''
                     ORDER BY created_at ASC
                     LIMIT ?1"
                };
                let task_params = if remaining == usize::MAX {
                    db::Params::None
                } else {
                    db::db_params![remaining as i64]
                };
                txn.query(task_sql, task_params)
                    .await?
                    .iter()
                    .map(|row| {
                        let title = row.get::<String>(1)?;
                        let description = row.get::<String>(2)?;
                        Ok(EmbeddingJobInput {
                            source_kind: "agent_task".to_string(),
                            source_id: row.get(0)?,
                            chunk_id: "0".to_string(),
                            title: row.get(1)?,
                            body: format!("{title}\n\n{description}"),
                            workspace_id: None,
                            thread_id: row.get(4)?,
                            agent_id: None,
                            source_timestamp: row.get(3)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let tasks_queued = tasks.len() as u64;
            for task in &tasks {
                enqueue_embedding_job_exec(&mut *txn, task, now).await?;
            }

            txn.commit().await?;
            Ok(SemanticBackfillResult {
                messages_queued,
                tasks_queued,
            })
        })
        .await
    }

    pub(crate) async fn reset_semantic_vector_index_state(
        &self,
    ) -> Result<SemanticIndexRepairStateReset> {
        self.call_embedding_writer_with_retry("reset_semantic_vector_index_state", || async {
            let mut txn = self.embedding_writer_db.transaction().await?;
            let cleared_completions = count_i64_exec(
                &mut *txn,
                "SELECT COUNT(*) FROM embedding_job_completions",
                db::Params::None,
            )
            .await?;
            let cleared_deletions = count_i64_exec(
                &mut *txn,
                "SELECT COUNT(*) FROM embedding_deletions",
                db::Params::None,
            )
            .await?;
            let reset_failed_jobs = count_i64_exec(
                &mut *txn,
                "SELECT COUNT(*) FROM embedding_jobs WHERE last_error IS NOT NULL",
                db::Params::None,
            )
            .await?;

            txn.execute("DELETE FROM embedding_job_completions", db::Params::None)
                .await?;
            txn.execute("DELETE FROM embedding_deletions", db::Params::None)
                .await?;
            txn.execute(
                "UPDATE embedding_jobs
                 SET claimed_at = NULL, last_error = NULL
                 WHERE claimed_at IS NOT NULL OR last_error IS NOT NULL",
                db::Params::None,
            )
            .await?;
            txn.commit().await?;

            Ok(SemanticIndexRepairStateReset {
                cleared_completions,
                cleared_deletions,
                reset_failed_jobs,
            })
        })
        .await
    }

    pub async fn semantic_index_status(
        &self,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<SemanticIndexStatus> {
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;
        let mut exec = db::ConnExecutor(&*self.read_db);
        let queued_jobs = count_i64_exec(
            &mut exec,
            "SELECT COUNT(*) FROM embedding_jobs",
            db::Params::None,
        )
        .await?;
        let queued_deletions = count_i64_exec(
            &mut exec,
            "SELECT COUNT(*) FROM embedding_deletions",
            db::Params::None,
        )
        .await?;
        let failed_jobs = count_i64_exec(
            &mut exec,
            "SELECT COUNT(*) FROM embedding_jobs WHERE last_error IS NOT NULL",
            db::Params::None,
        )
        .await?;
        let failed_deletions = count_i64_exec(
            &mut exec,
            "SELECT COUNT(*) FROM embedding_deletions WHERE last_error IS NOT NULL",
            db::Params::None,
        )
        .await?;
        let completed_for_model = if embedding_model.is_empty() {
            0
        } else {
            count_i64_exec(
                &mut exec,
                "SELECT COUNT(*) FROM embedding_job_completions
                 WHERE embedding_model = ?1 AND dimensions = ?2",
                db::db_params![embedding_model.clone(), dimensions],
            )
            .await?
        };
        let pending_for_model = if embedding_model.is_empty() {
            0
        } else {
            count_i64_exec(
                &mut exec,
                "SELECT COUNT(*)
                 FROM embedding_jobs job
                 WHERE NOT EXISTS (
                    SELECT 1
                    FROM embedding_job_completions completion
                    WHERE completion.source_kind = job.source_kind
                      AND completion.source_id = job.source_id
                      AND completion.chunk_id = job.chunk_id
                      AND completion.content_hash = job.content_hash
                      AND completion.embedding_model = ?1
                      AND completion.dimensions = ?2
                 )",
                db::db_params![embedding_model.clone(), dimensions],
            )
            .await?
        };
        Ok(SemanticIndexStatus {
            queued_jobs,
            pending_for_model,
            completed_for_model,
            queued_deletions,
            failed_jobs,
            failed_deletions,
        })
    }
}

fn count_i64<P>(conn: &Connection, sql: &str, params: P) -> rusqlite::Result<u64>
where
    P: rusqlite::Params,
{
    let count = conn.query_row(sql, params, |row| row.get::<_, i64>(0))?;
    Ok(count.max(0) as u64)
}

async fn count_i64_exec<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    sql: &str,
    params: db::Params,
) -> Result<u64> {
    let count = exec
        .query_opt(sql, params)
        .await?
        .map(|row| row.get::<i64>(0))
        .transpose()?
        .unwrap_or(0);
    Ok(count.max(0) as u64)
}
