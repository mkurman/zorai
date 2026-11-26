use super::embedding_queue::{
    enqueue_embedding_job_exec, queue_embedding_deletion_exec, EmbeddingJobInput,
};
use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SemanticDocumentSyncSummary {
    pub discovered: usize,
    pub changed: usize,
    pub queued_embeddings: usize,
    pub removed: usize,
}

#[derive(Debug, Clone)]
struct SemanticDocumentInput {
    source_kind: String,
    root_path: String,
    relative_path: String,
    source_id: String,
    title: String,
    content_hash: String,
    body: String,
    source_timestamp: i64,
}

impl HistoryStore {
    pub(crate) async fn sync_semantic_documents_from_dir(
        &self,
        source_kind: &str,
        root: &Path,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<SemanticDocumentSyncSummary> {
        validate_semantic_document_kind(source_kind)?;
        let root_path = normalized_root_path(root);
        let documents = collect_semantic_documents(source_kind, root, &root_path)?;
        self.sync_semantic_document_inputs(
            source_kind,
            root_path,
            documents,
            embedding_model,
            dimensions,
        )
        .await
    }

    pub(crate) async fn sync_code_chunks(
        &self,
        root: &Path,
        chunks: Vec<(String, String, String)>,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<SemanticDocumentSyncSummary> {
        let root_path = normalized_root_path(root);
        let now = now_ts() as i64;
        let documents = chunks
            .into_iter()
            .map(|(key, title, body)| SemanticDocumentInput {
                source_kind: "code".to_string(),
                root_path: root_path.clone(),
                source_id: format!("{root_path}::{key}"),
                relative_path: key,
                title,
                content_hash: semantic_document_hash(&body),
                body,
                source_timestamp: now,
            })
            .collect();
        self.sync_semantic_document_inputs(
            "code",
            root_path,
            documents,
            embedding_model,
            dimensions,
        )
        .await
    }

    pub(crate) async fn code_chunks_by_source_id(
        &self,
        source_ids: &[String],
    ) -> Result<Vec<(String, String, String, String)>> {
        let mut found = Vec::new();
        for source_id in source_ids {
            let rows = self
                .read_db
                .query(
                    "SELECT source_id, relative_path, title, body FROM semantic_documents
                     WHERE source_kind = 'code' AND source_id = ?1 AND deleted_at IS NULL",
                    db::db_params![source_id.clone()],
                )
                .await?;
            for row in rows {
                found.push((
                    row.get::<String>(0)?,
                    row.get::<String>(1)?,
                    row.get::<String>(2)?,
                    row.get::<String>(3)?,
                ));
            }
        }
        Ok(found)
    }

    async fn sync_semantic_document_inputs(
        &self,
        source_kind: &str,
        root_path: String,
        documents: Vec<SemanticDocumentInput>,
        embedding_model: &str,
        dimensions: u32,
    ) -> Result<SemanticDocumentSyncSummary> {
        let source_kind = source_kind.to_string();
        let embedding_model = embedding_model.trim().to_string();
        let dimensions = dimensions as i64;

        let now = now_ts() as i64;
        let mut summary = SemanticDocumentSyncSummary {
            discovered: documents.len(),
            ..SemanticDocumentSyncSummary::default()
        };
        let mut seen = BTreeSet::new();

        let mut txn = self.conn_db.transaction().await?;
        for document in &documents {
            seen.insert(document.relative_path.clone());
            let existing = load_semantic_document_state(
                &mut *txn,
                &document.source_kind,
                &document.root_path,
                &document.relative_path,
            )
            .await?;
            let changed = existing
                .as_ref()
                .map(|state| {
                    state.content_hash != document.content_hash || state.deleted_at.is_some()
                })
                .unwrap_or(true);
            if changed {
                summary.changed += 1;
            }

            txn.execute(
                "INSERT INTO semantic_documents (
                            source_kind, root_path, relative_path, source_id, title,
                            content_hash, body, discovered_at, updated_at, last_seen_at, deleted_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, NULL)
                         ON CONFLICT(source_kind, root_path, relative_path) DO UPDATE SET
                            source_id = excluded.source_id,
                            title = excluded.title,
                            content_hash = excluded.content_hash,
                            body = excluded.body,
                            updated_at = CASE
                                WHEN semantic_documents.content_hash != excluded.content_hash
                                  OR semantic_documents.deleted_at IS NOT NULL
                                THEN excluded.updated_at
                                ELSE semantic_documents.updated_at
                            END,
                            last_seen_at = excluded.last_seen_at,
                            deleted_at = NULL",
                db::db_params![
                    document.source_kind.clone(),
                    document.root_path.clone(),
                    document.relative_path.clone(),
                    document.source_id.clone(),
                    document.title.clone(),
                    document.content_hash.clone(),
                    document.body.clone(),
                    now
                ],
            )
            .await?;

            let needs_embedding = !embedding_model.is_empty()
                && !semantic_document_embedding_complete(
                    &mut *txn,
                    document,
                    &embedding_model,
                    dimensions,
                )
                .await?;
            if needs_embedding {
                enqueue_embedding_job_exec(
                    &mut *txn,
                    &EmbeddingJobInput {
                        source_kind: document.source_kind.clone(),
                        source_id: document.source_id.clone(),
                        chunk_id: "0".to_string(),
                        title: document.title.clone(),
                        body: document.body.clone(),
                        workspace_id: None,
                        thread_id: None,
                        agent_id: None,
                        source_timestamp: document.source_timestamp,
                    },
                    now,
                )
                .await?;
                summary.queued_embeddings += 1;
            }
        }

        let removed = mark_missing_semantic_documents_removed(
            &mut *txn,
            &source_kind,
            &root_path,
            &seen,
            now,
        )
        .await?;
        summary.removed = removed;
        txn.commit().await?;
        Ok(summary)
    }
}

#[derive(Debug)]
struct SemanticDocumentState {
    content_hash: String,
    deleted_at: Option<i64>,
}

fn validate_semantic_document_kind(source_kind: &str) -> Result<()> {
    match source_kind {
        "skill" | "guideline" | "code" => Ok(()),
        other => anyhow::bail!("unsupported semantic document kind '{other}'"),
    }
}

fn normalized_root_path(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn collect_semantic_documents(
    source_kind: &str,
    root: &Path,
    root_path: &str,
) -> Result<Vec<SemanticDocumentInput>> {
    let mut files = Vec::new();
    collect_semantic_document_paths(source_kind, root, root, &mut files)?;
    files.sort();
    files
        .into_iter()
        .map(|path| semantic_document_input(source_kind, root, root_path, &path))
        .collect()
}

fn collect_semantic_document_paths(
    source_kind: &str,
    root: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_semantic_document_paths(source_kind, root, &path, out)?;
        } else if should_index_semantic_document(source_kind, root, &path) {
            out.push(path);
        }
    }
    Ok(())
}

fn should_index_semantic_document(source_kind: &str, root: &Path, path: &Path) -> bool {
    match source_kind {
        "skill" => {
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            file_name.eq_ignore_ascii_case("skill.md")
                || (is_markdown(path)
                    && path.strip_prefix(root).ok().is_some_and(|relative| {
                        relative.components().any(|c| c.as_os_str() == "generated")
                    }))
        }
        "guideline" => is_markdown(path),
        _ => false,
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
}

fn semantic_document_input(
    source_kind: &str,
    root: &Path,
    root_path: &str,
    path: &Path,
) -> Result<SemanticDocumentInput> {
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("read semantic document {}", path.display()))?;
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let metadata = derive_skill_metadata(&relative_path, &body);
    let title = if metadata.skill_name.is_empty() {
        fallback_semantic_document_title(path)
    } else {
        metadata.skill_name
    };
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_else(|| now_ts() as i64);

    Ok(SemanticDocumentInput {
        source_kind: source_kind.to_string(),
        root_path: root_path.to_string(),
        source_id: relative_path.clone(),
        relative_path,
        title,
        content_hash: semantic_document_hash(&body),
        body,
        source_timestamp: modified,
    })
}

fn fallback_semantic_document_title(path: &Path) -> String {
    if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("skill.md"))
    {
        return path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .unwrap_or("skill")
            .to_string();
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .to_string()
}

fn semantic_document_hash(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn load_semantic_document_state<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    source_kind: &str,
    root_path: &str,
    relative_path: &str,
) -> Result<Option<SemanticDocumentState>> {
    let row = exec
        .query_opt(
            "SELECT content_hash, deleted_at FROM semantic_documents
         WHERE source_kind = ?1 AND root_path = ?2 AND relative_path = ?3",
            db::db_params![source_kind, root_path, relative_path],
        )
        .await?;
    row.map(|row| -> anyhow::Result<SemanticDocumentState> {
        Ok(SemanticDocumentState {
            content_hash: row.get(0)?,
            deleted_at: row.get(1)?,
        })
    })
    .transpose()
}

async fn semantic_document_embedding_complete<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    document: &SemanticDocumentInput,
    embedding_model: &str,
    dimensions: i64,
) -> Result<bool> {
    let row = exec
        .query_opt(
            "SELECT 1 FROM embedding_job_completions
         WHERE source_kind = ?1
           AND source_id = ?2
           AND chunk_id = '0'
           AND content_hash = ?3
           AND embedding_model = ?4
           AND dimensions = ?5
         LIMIT 1",
            db::db_params![
                document.source_kind.clone(),
                document.source_id.clone(),
                document.content_hash.clone(),
                embedding_model,
                dimensions
            ],
        )
        .await?;
    Ok(row.is_some())
}

async fn mark_missing_semantic_documents_removed<E: db::DbExecutor + ?Sized>(
    exec: &mut E,
    source_kind: &str,
    root_path: &str,
    seen: &BTreeSet<String>,
    now: i64,
) -> Result<usize> {
    let rows = exec
        .query(
            "SELECT relative_path, source_id FROM semantic_documents
         WHERE source_kind = ?1 AND root_path = ?2 AND deleted_at IS NULL",
            db::db_params![source_kind, root_path],
        )
        .await?;
    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|row| -> anyhow::Result<(String, String)> {
            Ok((row.get::<String>(0)?, row.get::<String>(1)?))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut removed = 0usize;
    for (relative_path, source_id) in pairs {
        if seen.contains(&relative_path) {
            continue;
        }
        exec.execute(
            "UPDATE semantic_documents
             SET deleted_at = ?4, updated_at = ?4
             WHERE source_kind = ?1 AND root_path = ?2 AND relative_path = ?3",
            db::db_params![source_kind, root_path, relative_path, now],
        )
        .await?;
        queue_embedding_deletion_exec(exec, source_kind, &source_id, now).await?;
        removed += 1;
    }
    Ok(removed)
}
