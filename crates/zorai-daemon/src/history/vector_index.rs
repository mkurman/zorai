use anyhow::{Context, Result};
use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TABLE_NAME: &str = "history_vectors";
const VECTOR_COL: &str = "embedding";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorSourceKind {
    HistoryEntry,
    AgentMessage,
    AgentTask,
    AgentEvent,
    Skill,
    Guideline,
    ActionAudit,
    CausalTrace,
    Counterfactual,
    MetaCognition,
}

impl VectorSourceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::HistoryEntry => "history_entry",
            Self::AgentMessage => "agent_message",
            Self::AgentTask => "agent_task",
            Self::AgentEvent => "agent_event",
            Self::Skill => "skill",
            Self::Guideline => "guideline",
            Self::ActionAudit => "action_audit",
            Self::CausalTrace => "causal_trace",
            Self::Counterfactual => "counterfactual",
            Self::MetaCognition => "meta_cognition",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "history_entry" => Some(Self::HistoryEntry),
            "agent_message" => Some(Self::AgentMessage),
            "agent_task" => Some(Self::AgentTask),
            "agent_event" => Some(Self::AgentEvent),
            "skill" => Some(Self::Skill),
            "guideline" => Some(Self::Guideline),
            "action_audit" => Some(Self::ActionAudit),
            "causal_trace" => Some(Self::CausalTrace),
            "counterfactual" => Some(Self::Counterfactual),
            "meta_cognition" => Some(Self::MetaCognition),
            _ => None,
        }
    }

    pub(crate) fn from_embedding_source_kind(value: &str) -> Option<Self> {
        Self::from_str(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VectorDocument {
    pub source_kind: VectorSourceKind,
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub body: String,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub timestamp: i64,
    pub embedding_model: String,
    pub embedding: Vec<f32>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct VectorSearchRequest {
    pub embedding: Vec<f32>,
    pub embedding_model: String,
    pub limit: usize,
    pub source_kinds: Vec<VectorSourceKind>,
    pub workspace_id: Option<String>,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VectorSearchHit {
    pub source_kind: VectorSourceKind,
    pub source_id: String,
    pub chunk_id: String,
    pub title: String,
    pub snippet: Option<String>,
    pub timestamp: Option<i64>,
    pub score: f64,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct VectorIndex {
    dir: PathBuf,
}

impl VectorIndex {
    pub(crate) fn open(root: &Path) -> Self {
        Self {
            dir: root.join("vector-index").join("lancedb"),
        }
    }

    pub(crate) async fn upsert(&self, document: VectorDocument) -> Result<()> {
        self.upsert_many(vec![document]).await
    }

    pub(crate) async fn upsert_many(&self, documents: Vec<VectorDocument>) -> Result<()> {
        if documents.is_empty() {
            return Ok(());
        }
        let dim = documents[0].embedding.len();
        for document in &documents {
            anyhow::ensure!(
                document.embedding.len() == dim,
                "vector documents in a batch must share the same dimension"
            );
            anyhow::ensure!(dim > 0, "vector document embedding cannot be empty");
            anyhow::ensure!(
                document.embedding.iter().all(|value| value.is_finite()),
                "vector document embedding contains non-finite values"
            );
        }

        let table = self.open_or_create_table(dim).await?;
        ensure_table_dimension(&table, dim).await?;
        let source_keys = documents
            .iter()
            .map(|document| {
                source_key(
                    document.source_kind,
                    &document.source_id,
                    &document.chunk_id,
                )
            })
            .collect::<Vec<_>>();
        table
            .delete(&format!("source_key IN ({})", sql_list(&source_keys)))
            .await
            .context("failed to delete previous LanceDB vector chunk")?;
        table
            .add(record_batch_for_documents(documents, source_keys)?)
            .execute()
            .await
            .context("failed to add LanceDB vector document")?;
        Ok(())
    }

    pub(crate) async fn delete_source(
        &self,
        source_kind: VectorSourceKind,
        source_id: &str,
    ) -> Result<()> {
        self.delete_sources(vec![(source_kind, source_id.to_string())])
            .await
    }

    pub(crate) async fn delete_sources(
        &self,
        sources: Vec<(VectorSourceKind, String)>,
    ) -> Result<()> {
        if sources.is_empty() {
            return Ok(());
        }
        let Some(table) = self.open_existing_table().await? else {
            return Ok(());
        };
        let mut by_kind: std::collections::BTreeMap<&'static str, Vec<String>> =
            std::collections::BTreeMap::new();
        for (source_kind, source_id) in sources {
            by_kind
                .entry(source_kind.as_str())
                .or_default()
                .push(source_id);
        }
        let predicate = by_kind
            .into_iter()
            .map(|(source_kind_str, source_ids)| {
                format!(
                    "(source_kind = {} AND source_id IN ({}))",
                    sql_quote(source_kind_str),
                    sql_list(&source_ids)
                )
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        table
            .delete(&predicate)
            .await
            .context("failed to delete LanceDB vector source")?;
        Ok(())
    }

    pub(crate) fn repair_corrupted_index_dir(&self) -> Result<Option<PathBuf>> {
        if !self.dir.exists() {
            return Ok(None);
        }
        let parent = self
            .dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("LanceDB vector index path has no parent"))?;
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to ensure LanceDB vector index parent {}",
                parent.display()
            )
        })?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut backup = parent.join(format!("lancedb.repair-backup-{now_ms}"));
        let mut suffix = 1usize;
        while backup.exists() {
            backup = parent.join(format!("lancedb.repair-backup-{now_ms}-{suffix}"));
            suffix += 1;
        }
        std::fs::rename(&self.dir, &backup).with_context(|| {
            format!(
                "failed to move corrupted LanceDB vector index {} to {}",
                self.dir.display(),
                backup.display()
            )
        })?;
        Ok(Some(backup))
    }

    pub(crate) async fn search(
        &self,
        request: VectorSearchRequest,
    ) -> Result<Vec<VectorSearchHit>> {
        if request.limit == 0 || request.embedding.is_empty() {
            return Ok(Vec::new());
        }
        let Some(table) = self.open_existing_table().await? else {
            return Ok(Vec::new());
        };
        ensure_table_dimension(&table, request.embedding.len()).await?;

        let mut query = table
            .query()
            .nearest_to(request.embedding.clone())?
            .column(VECTOR_COL)
            .limit(request.limit);
        let filter = search_filter(&request);
        if !filter.is_empty() {
            query = query.only_if(filter.join(" AND "));
        }
        let batches = query
            .execute()
            .await
            .context("failed to execute LanceDB vector search")?
            .try_collect::<Vec<_>>()
            .await
            .context("failed to collect LanceDB vector search results")?;

        let mut hits = Vec::new();
        for batch in batches {
            for row in 0..batch.num_rows() {
                if let Some(hit) = hit_from_batch(&batch, row) {
                    hits.push(hit);
                }
            }
        }
        Ok(hits)
    }

    pub(crate) async fn optimize(&self) -> Result<Option<lancedb::table::OptimizeStats>> {
        let Some(table) = self.open_existing_table().await? else {
            return Ok(None);
        };
        let stats = table
            .optimize(lancedb::table::OptimizeAction::All)
            .await
            .context("failed to optimize LanceDB vector index")?;
        Ok(Some(stats))
    }

    async fn connection(&self) -> Result<lancedb::Connection> {
        let uri = self
            .dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("LanceDB vector index path is not valid UTF-8"))?;
        lancedb::connect(uri)
            .execute()
            .await
            .context("failed to connect to LanceDB vector index")
    }

    async fn open_existing_table(&self) -> Result<Option<lancedb::Table>> {
        let db = self.connection().await?;
        if !db
            .table_names()
            .execute()
            .await?
            .contains(&TABLE_NAME.to_string())
        {
            return Ok(None);
        }
        Ok(Some(db.open_table(TABLE_NAME).execute().await?))
    }

    async fn open_or_create_table(&self, dim: usize) -> Result<lancedb::Table> {
        let db = self.connection().await?;
        if db
            .table_names()
            .execute()
            .await?
            .contains(&TABLE_NAME.to_string())
        {
            return Ok(db.open_table(TABLE_NAME).execute().await?);
        }
        db.create_empty_table(TABLE_NAME, Arc::new(vector_schema(dim)))
            .execute()
            .await
            .context("failed to create LanceDB vector table")
    }
}

fn vector_schema(dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("source_key", DataType::Utf8, false),
        Field::new("source_kind", DataType::Utf8, false),
        Field::new("source_id", DataType::Utf8, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("body", DataType::Utf8, false),
        Field::new("workspace_id", DataType::Utf8, true),
        Field::new("thread_id", DataType::Utf8, true),
        Field::new("agent_id", DataType::Utf8, true),
        Field::new("timestamp", DataType::Int64, true),
        Field::new("embedding_model", DataType::Utf8, false),
        Field::new("metadata_json", DataType::Utf8, true),
        Field::new(
            VECTOR_COL,
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
    ])
}

fn record_batch_for_documents(
    documents: Vec<VectorDocument>,
    source_keys: Vec<String>,
) -> Result<RecordBatch> {
    let Some(dim) = documents.first().map(|document| document.embedding.len()) else {
        anyhow::bail!("vector document batch cannot be empty");
    };
    let row_count = documents.len();
    let mut source_kinds = Vec::with_capacity(row_count);
    let mut source_ids = Vec::with_capacity(row_count);
    let mut chunk_ids = Vec::with_capacity(row_count);
    let mut titles = Vec::with_capacity(row_count);
    let mut bodies = Vec::with_capacity(row_count);
    let mut workspace_ids = Vec::with_capacity(row_count);
    let mut thread_ids = Vec::with_capacity(row_count);
    let mut agent_ids = Vec::with_capacity(row_count);
    let mut timestamps = Vec::with_capacity(row_count);
    let mut embedding_models = Vec::with_capacity(row_count);
    let mut metadata_json = Vec::with_capacity(row_count);
    let mut vector_rows = Vec::with_capacity(row_count);

    for document in documents {
        source_kinds.push(document.source_kind.as_str());
        source_ids.push(document.source_id);
        chunk_ids.push(document.chunk_id);
        titles.push(document.title);
        bodies.push(document.body);
        workspace_ids.push(document.workspace_id);
        thread_ids.push(document.thread_id);
        agent_ids.push(document.agent_id);
        timestamps.push(Some(document.timestamp));
        embedding_models.push(document.embedding_model);
        metadata_json.push(document.metadata_json);
        vector_rows.push(Some(
            document
                .embedding
                .into_iter()
                .map(Some)
                .collect::<Vec<Option<f32>>>(),
        ));
    }

    RecordBatch::try_new(
        Arc::new(vector_schema(dim)),
        vec![
            Arc::new(StringArray::from(source_keys)),
            Arc::new(StringArray::from(source_kinds)),
            Arc::new(StringArray::from(source_ids)),
            Arc::new(StringArray::from(chunk_ids)),
            Arc::new(StringArray::from(titles)),
            Arc::new(StringArray::from(bodies)),
            Arc::new(StringArray::from(workspace_ids)),
            Arc::new(StringArray::from(thread_ids)),
            Arc::new(StringArray::from(agent_ids)),
            Arc::new(Int64Array::from(timestamps)),
            Arc::new(StringArray::from(embedding_models)),
            Arc::new(StringArray::from(metadata_json)),
            Arc::new(
                FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    vector_rows,
                    dim as i32,
                ),
            ),
        ],
    )
    .context("failed to build LanceDB vector record batch")
}

async fn ensure_table_dimension(table: &lancedb::Table, dim: usize) -> Result<()> {
    let schema = table.schema().await?;
    let field = schema.field_with_name(VECTOR_COL)?;
    let DataType::FixedSizeList(_, actual_dim) = field.data_type() else {
        anyhow::bail!("LanceDB vector table has non-vector embedding column");
    };
    anyhow::ensure!(
        *actual_dim as usize == dim,
        "LanceDB vector dimension mismatch: table has {actual_dim}, request has {dim}"
    );
    Ok(())
}

fn search_filter(request: &VectorSearchRequest) -> Vec<String> {
    let mut filters = Vec::new();
    if !request.source_kinds.is_empty() {
        let kinds = request
            .source_kinds
            .iter()
            .map(|kind| sql_quote(kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        filters.push(format!("source_kind IN ({kinds})"));
    }
    if let Some(workspace_id) = &request.workspace_id {
        filters.push(format!("workspace_id = {}", sql_quote(workspace_id)));
    }
    if let Some(thread_id) = &request.thread_id {
        filters.push(format!("thread_id = {}", sql_quote(thread_id)));
    }
    if let Some(agent_id) = &request.agent_id {
        filters.push(format!("agent_id = {}", sql_quote(agent_id)));
    }
    filters.push(format!(
        "embedding_model = {}",
        sql_quote(&request.embedding_model)
    ));
    filters
}

fn hit_from_batch(batch: &RecordBatch, row: usize) -> Option<VectorSearchHit> {
    let source_kind = VectorSourceKind::from_str(&string_value(batch, "source_kind", row)?)?;
    Some(VectorSearchHit {
        source_kind,
        source_id: string_value(batch, "source_id", row)?,
        chunk_id: string_value(batch, "chunk_id", row)?,
        title: string_value(batch, "title", row)?,
        snippet: string_value(batch, "body", row),
        timestamp: int64_value(batch, "timestamp", row),
        score: distance_value(batch, row),
        metadata_json: string_value(batch, "metadata_json", row),
    })
}

fn string_value(batch: &RecordBatch, name: &str, row: usize) -> Option<String> {
    let idx = batch.schema().index_of(name).ok()?;
    let array = batch.column(idx).as_any().downcast_ref::<StringArray>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row).to_string())
    }
}

fn int64_value(batch: &RecordBatch, name: &str, row: usize) -> Option<i64> {
    let idx = batch.schema().index_of(name).ok()?;
    let array = batch.column(idx).as_any().downcast_ref::<Int64Array>()?;
    if array.is_null(row) {
        None
    } else {
        Some(array.value(row))
    }
}

fn distance_value(batch: &RecordBatch, row: usize) -> f64 {
    let Ok(idx) = batch.schema().index_of("_distance") else {
        return 0.0;
    };
    if let Some(array) = batch.column(idx).as_any().downcast_ref::<Float32Array>() {
        return if array.is_null(row) {
            0.0
        } else {
            f64::from(array.value(row))
        };
    }
    if let Some(array) = batch.column(idx).as_any().downcast_ref::<Float64Array>() {
        return if array.is_null(row) {
            0.0
        } else {
            array.value(row)
        };
    }
    0.0
}

fn source_key(source_kind: VectorSourceKind, source_id: &str, chunk_id: &str) -> String {
    format!("{}:{source_id}:{chunk_id}", source_kind.as_str())
}

fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| sql_quote(value))
        .collect::<Vec<_>>()
        .join(", ")
}
