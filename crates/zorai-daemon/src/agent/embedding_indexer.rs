use super::*;
use std::collections::HashMap;
use zorai_protocol::HistorySearchHit;

const EMBEDDING_IDLE_SLEEP_SECS: u64 = 30;
const EMBEDDING_DISABLED_SLEEP_SECS: u64 = 60;
const EMBEDDING_ACTIVE_SLEEP_MILLIS: u64 = 1_500;
/// Grace period before the embedding indexer's first run after daemon
/// startup. Keeps the writer connection (and the LanceDB store) clear of
/// background work while the concierge welcome and TUI's first-paint queries
/// are in flight — large embedding batches would otherwise queue on the same
/// tokio_rusqlite writer thread or hold the LanceDB index open while welcome
/// is trying to render. The indexer makes up any backlog on subsequent ticks.
const EMBEDDING_STARTUP_GRACE_SECS: u64 = 15;
const SEMANTIC_DOCUMENT_SCAN_SECS: u64 = 60;
const SEMANTIC_DOCUMENT_DAILY_SCAN_SECS: u64 = 86_400;
#[cfg(feature = "lancedb-vector")]
const VECTOR_INDEX_OPTIMIZE_INTERVAL_SECS: u64 = 86_400;
#[cfg(feature = "lancedb-vector")]
const EMBEDDING_REQUEST_TIMEOUT_SECS: u64 = 90;
const MAX_EMBEDDING_BATCH_SIZE: usize = 16;
#[cfg(feature = "lancedb-vector")]
const MAX_EMBEDDING_DELETIONS_PER_TICK: usize = 256;
#[cfg(feature = "lancedb-vector")]
const OPENROUTER_ATTRIBUTION_URL: &str = "https://zorai.app";
#[cfg(feature = "lancedb-vector")]
const OPENROUTER_ATTRIBUTION_TITLE: &str = "Zorai";
#[cfg(feature = "lancedb-vector")]
const OPENROUTER_ATTRIBUTION_CATEGORIES: &str = "cli-agent,personal-agent";

#[derive(Debug, Clone, Default)]
pub(crate) struct SemanticDocumentIndexSyncSummary {
    pub skills: crate::history::SemanticDocumentSyncSummary,
    pub guidelines: crate::history::SemanticDocumentSyncSummary,
}

impl SemanticDocumentIndexSyncSummary {
    fn changed_or_removed(&self) -> usize {
        self.skills.changed
            + self.skills.removed
            + self.guidelines.changed
            + self.guidelines.removed
    }

    fn into_public(
        self,
        embedding_model: String,
        dimensions: u32,
    ) -> zorai_protocol::SemanticDocumentIndexSyncResultPublic {
        zorai_protocol::SemanticDocumentIndexSyncResultPublic {
            embedding_model,
            dimensions,
            skills: semantic_document_summary_public(self.skills),
            guidelines: semantic_document_summary_public(self.guidelines),
        }
    }
}

fn semantic_document_summary_public(
    summary: crate::history::SemanticDocumentSyncSummary,
) -> zorai_protocol::SemanticDocumentSyncSummaryPublic {
    zorai_protocol::SemanticDocumentSyncSummaryPublic {
        discovered: summary.discovered,
        changed: summary.changed,
        queued_embeddings: summary.queued_embeddings,
        removed: summary.removed,
    }
}

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingDatum>,
}

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingErrorEnvelope {
    error: EmbeddingProviderError,
}

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingProviderError {
    message: Option<String>,
    #[serde(default)]
    code: Option<serde_json::Value>,
}

#[cfg(feature = "lancedb-vector")]
#[derive(Debug, serde::Deserialize)]
struct EmbeddingDatum {
    #[serde(default)]
    index: Option<usize>,
    embedding: Vec<f32>,
}

fn openai_like_endpoint(base_url: &str, endpoint: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let endpoint = endpoint.trim_start_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/{endpoint}")
    } else {
        format!("{base}/v1/{endpoint}")
    }
}

fn embedding_batch_size(configured: u32) -> usize {
    (configured as usize).clamp(1, MAX_EMBEDDING_BATCH_SIZE)
}

#[cfg(feature = "lancedb-vector")]
fn parse_embedding_response(response_text: &str) -> Result<EmbeddingResponse> {
    let value = serde_json::from_str::<serde_json::Value>(response_text)
        .context("invalid embedding provider response")?;
    if value.get("error").is_some() {
        let envelope: EmbeddingErrorEnvelope =
            serde_json::from_value(value).context("invalid embedding provider error response")?;
        let mut message = envelope
            .error
            .message
            .unwrap_or_else(|| "unknown provider error".to_string());
        if let Some(code) = envelope.error.code {
            if !code.is_null() && !message.contains(&code.to_string()) {
                message = format!("{message} (code: {code})");
            }
        }
        anyhow::bail!("embedding provider returned error: {message}");
    }
    serde_json::from_value(value).context("invalid embedding provider response")
}

#[cfg(feature = "lancedb-vector")]
fn semantic_score_from_distance(distance: f64) -> f64 {
    if distance.is_finite() && distance >= 0.0 {
        1.0 / (1.0 + distance)
    } else {
        0.0
    }
}

#[cfg(feature = "lancedb-vector")]
fn apply_openrouter_embedding_attribution_headers(
    req: reqwest::RequestBuilder,
    provider_id: &str,
) -> reqwest::RequestBuilder {
    if provider_id != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        return req;
    }

    req.header("HTTP-Referer", OPENROUTER_ATTRIBUTION_URL)
        .header("X-OpenRouter-Title", OPENROUTER_ATTRIBUTION_TITLE)
        .header("X-OpenRouter-Categories", OPENROUTER_ATTRIBUTION_CATEGORIES)
}

#[cfg(feature = "lancedb-vector")]
fn vector_hits_to_history_hits(
    hits: Vec<crate::history::vector_index::VectorSearchHit>,
) -> Vec<HistorySearchHit> {
    hits.into_iter()
        .map(|hit| HistorySearchHit {
            id: hit.source_id,
            kind: hit.source_kind.as_str().to_string(),
            title: hit.title,
            excerpt: hit.snippet.unwrap_or_default(),
            path: None,
            timestamp: hit.timestamp.unwrap_or_default().max(0) as u64,
            score: semantic_score_from_distance(hit.score),
        })
        .collect()
}

#[cfg(feature = "lancedb-vector")]
fn resolved_embedding_model(config: &AgentConfig) -> String {
    config.semantic.embedding.model.trim().to_string()
}

#[cfg(feature = "lancedb-vector")]
fn resolve_embedding_provider(
    config: &AgentConfig,
) -> Result<(String, ProviderConfig, AuthMethod)> {
    let provider_id = config.semantic.embedding.provider.trim();
    if provider_id.is_empty() {
        anyhow::bail!("semantic embedding provider is empty");
    }
    let model = resolved_embedding_model(config);
    if model.is_empty() {
        anyhow::bail!("semantic embedding model is empty");
    }
    let mut provider_config = resolve_provider_config_for(config, provider_id, Some(&model))?;
    provider_config.model = model;
    let auth_method = get_provider_definition(provider_id)
        .map(|definition| definition.auth_method)
        .unwrap_or(AuthMethod::Bearer);
    Ok((provider_id.to_string(), provider_config, auth_method))
}

#[cfg(feature = "lancedb-vector")]
async fn request_embeddings(
    http_client: &reqwest::Client,
    config: &AgentConfig,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let (provider_id, provider_config, auth_method) = resolve_embedding_provider(config)?;
    let api_key = provider_config.api_key.trim();
    if api_key.is_empty() {
        anyhow::bail!(
            "No API key configured for embedding provider '{}'. Log in or configure provider credentials.",
            provider_id
        );
    }
    let endpoint = openai_like_endpoint(&provider_config.base_url, "embeddings");
    let mut body = serde_json::json!({
        "model": provider_config.model,
        "input": inputs,
    });
    if config.semantic.embedding.dimensions > 0 {
        body["dimensions"] = serde_json::json!(config.semantic.embedding.dimensions);
    }

    let request = auth_method.apply(http_client.post(endpoint), api_key);
    let response = apply_openrouter_embedding_attribution_headers(request, &provider_id)
        .timeout(std::time::Duration::from_secs(
            EMBEDDING_REQUEST_TIMEOUT_SECS,
        ))
        .json(&body)
        .send()
        .await
        .context("embedding provider request failed")?;
    let status = response.status();
    let response_text = response
        .text()
        .await
        .context("failed to read embedding provider response")?;
    if !status.is_success() {
        let preview = response_text.chars().take(1000).collect::<String>();
        anyhow::bail!("embedding provider returned HTTP {status}: {preview}");
    }

    let mut parsed = parse_embedding_response(&response_text)?;
    if parsed.data.len() != inputs.len() {
        anyhow::bail!(
            "embedding provider returned {} embeddings for {} inputs",
            parsed.data.len(),
            inputs.len()
        );
    }
    parsed
        .data
        .sort_by_key(|datum| datum.index.unwrap_or(usize::MAX));
    let embeddings = parsed
        .data
        .into_iter()
        .map(|datum| datum.embedding)
        .collect::<Vec<_>>();
    let expected_dimensions = config.semantic.embedding.dimensions as usize;
    if expected_dimensions > 0 {
        for embedding in &embeddings {
            if embedding.len() != expected_dimensions {
                anyhow::bail!(
                    "embedding provider returned {} dimensions, expected {}",
                    embedding.len(),
                    expected_dimensions
                );
            }
        }
    }
    Ok(embeddings)
}

impl AgentEngine {
    pub(crate) async fn run_semantic_document_index_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(EMBEDDING_STARTUP_GRACE_SECS)) => {}
            _ = shutdown.changed() => return,
        }
        let mut last_daily_scan = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(
                SEMANTIC_DOCUMENT_DAILY_SCAN_SECS,
            ))
            .unwrap_or_else(std::time::Instant::now);
        loop {
            let config = self.get_config().await;
            let should_scan = config.semantic.embedding.enabled
                && !config.semantic.embedding.model.trim().is_empty();
            if should_scan {
                let force_daily = last_daily_scan.elapsed()
                    >= std::time::Duration::from_secs(SEMANTIC_DOCUMENT_DAILY_SCAN_SECS);
                match self.sync_semantic_document_indexes_once().await {
                    Ok(summary) => {
                        if force_daily || summary.changed_or_removed() > 0 {
                            last_daily_scan = std::time::Instant::now();
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = %error, "semantic document index sync failed");
                    }
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(SEMANTIC_DOCUMENT_SCAN_SECS)) => {}
                _ = self.config_notify.notified() => {}
                _ = shutdown.changed() => break,
            }
        }
    }

    pub(crate) async fn sync_semantic_document_indexes_once(
        &self,
    ) -> Result<SemanticDocumentIndexSyncSummary> {
        let _guard = self.semantic_document_index_sync_lock.lock().await;
        self.sync_semantic_document_indexes_once_inner().await
    }

    async fn sync_semantic_document_indexes_once_inner(
        &self,
    ) -> Result<SemanticDocumentIndexSyncSummary> {
        let config = self.get_config().await;
        let embedding_model = config.semantic.embedding.model.trim().to_string();
        let dimensions = config.semantic.embedding.dimensions;
        if !config.semantic.embedding.enabled || embedding_model.is_empty() {
            return Ok(SemanticDocumentIndexSyncSummary::default());
        }

        let skills_root = self.history.data_dir().to_path_buf();
        let guidelines_root = super::guidelines_dir(self.history.data_dir());
        super::skill_recommendation::sync_skill_catalog(&self.history, &skills_root).await?;
        let skills = self
            .history
            .sync_semantic_documents_from_dir("skill", &skills_root, &embedding_model, dimensions)
            .await?;
        let guidelines = self
            .history
            .sync_semantic_documents_from_dir(
                "guideline",
                &guidelines_root,
                &embedding_model,
                dimensions,
            )
            .await?;

        Ok(SemanticDocumentIndexSyncSummary { skills, guidelines })
    }

    pub(crate) async fn sync_semantic_document_indexes_public(
        &self,
    ) -> Result<zorai_protocol::SemanticDocumentIndexSyncResultPublic> {
        let config = self.get_config().await;
        let embedding_model = if config.semantic.embedding.enabled {
            config.semantic.embedding.model.trim().to_string()
        } else {
            String::new()
        };
        let dimensions = config.semantic.embedding.dimensions;
        let summary = self.sync_semantic_document_indexes_once().await?;
        Ok(summary.into_public(embedding_model, dimensions))
    }

    pub(crate) async fn repair_semantic_index_public(
        &self,
        confirmed: bool,
    ) -> Result<zorai_protocol::SemanticIndexRepairResultPublic> {
        if !confirmed {
            anyhow::bail!("semantic index repair requires explicit confirmation");
        }
        let _guard = self.semantic_vector_index_lock.lock().await;

        #[cfg(feature = "lancedb-vector")]
        let backup_path = {
            let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
            index.repair_corrupted_index_dir()?
        };
        #[cfg(not(feature = "lancedb-vector"))]
        let backup_path: Option<std::path::PathBuf> = None;

        let reset = self.history.reset_semantic_vector_index_state().await?;
        Ok(zorai_protocol::SemanticIndexRepairResultPublic {
            removed_vector_index: backup_path.is_some(),
            backup_path: backup_path.map(|path| path.display().to_string()),
            cleared_completions: reset.cleared_completions,
            cleared_deletions: reset.cleared_deletions,
            reset_failed_jobs: reset.reset_failed_jobs,
        })
    }

    pub(crate) async fn run_embedding_index_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(EMBEDDING_STARTUP_GRACE_SECS)) => {}
            _ = shutdown.changed() => return,
        }
        #[cfg(feature = "lancedb-vector")]
        let mut last_vector_optimize: Option<std::time::Instant> = None;
        loop {
            let enabled = self.config.read().await.semantic.embedding.enabled;
            let processed = match self.process_embedding_queue_once().await {
                Ok(processed) => processed,
                Err(error) => {
                    tracing::warn!(error = %error, "semantic embedding queue processing failed");
                    0
                }
            };

            #[cfg(feature = "lancedb-vector")]
            if processed == 0
                && last_vector_optimize.is_none_or(|at| {
                    at.elapsed()
                        >= std::time::Duration::from_secs(VECTOR_INDEX_OPTIMIZE_INTERVAL_SECS)
                })
            {
                last_vector_optimize = Some(std::time::Instant::now());
                self.optimize_vector_index().await;
            }

            let sleep_duration = if !enabled {
                std::time::Duration::from_secs(EMBEDDING_DISABLED_SLEEP_SECS)
            } else if processed > 0 {
                std::time::Duration::from_millis(EMBEDDING_ACTIVE_SLEEP_MILLIS)
            } else {
                std::time::Duration::from_secs(EMBEDDING_IDLE_SLEEP_SECS)
            };

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {}
                _ = self.config_notify.notified() => {}
                _ = shutdown.changed() => break,
            }
        }
    }

    #[cfg(feature = "lancedb-vector")]
    async fn optimize_vector_index(&self) {
        let _guard = self.semantic_vector_index_lock.lock().await;
        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        match index.optimize().await {
            Ok(Some(stats)) => tracing::info!(
                fragments_removed = stats.compaction.as_ref().map(|c| c.fragments_removed),
                fragments_added = stats.compaction.as_ref().map(|c| c.fragments_added),
                bytes_removed = stats.prune.as_ref().map(|p| p.bytes_removed),
                old_versions_removed = stats.prune.as_ref().map(|p| p.old_versions),
                "semantic vector index optimized"
            ),
            Ok(None) => {}
            Err(error) => tracing::warn!(error = %error, "semantic vector index optimize failed"),
        }
    }

    pub(crate) async fn process_embedding_queue_once(&self) -> Result<usize> {
        let deleted = self.process_embedding_deletions_once().await?;
        let config = self.get_config().await;
        if !config.semantic.embedding.enabled {
            return Ok(deleted);
        }
        Ok(deleted + self.process_embedding_queue_once_inner(config).await?)
    }

    pub(crate) async fn search_history_semantic_first(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let limit = limit.max(1);
        let config = self.get_config().await;
        if !config.semantic.embedding.enabled {
            return self.history.search(query, limit).await;
        }
        match self
            .semantic_vector_history_search(query, limit, &config)
            .await
        {
            Ok((summary, hits)) if !hits.is_empty() => Ok((summary, hits)),
            Ok(_) => self.history.search(query, limit).await,
            Err(error) => {
                tracing::warn!(error = %error, "semantic history search failed; falling back to SQLite FTS");
                self.history.search(query, limit).await
            }
        }
    }

    #[cfg(not(feature = "lancedb-vector"))]
    pub(crate) async fn semantic_document_scores(
        &self,
        _query: &str,
        _source_kind: &str,
        _limit: usize,
        _config: &AgentConfig,
    ) -> Result<HashMap<String, f64>> {
        Ok(HashMap::new())
    }

    /// Embed arbitrary texts for in-process similarity work (e.g. semantic
    /// handoff routing). Returns `None` when embeddings are disabled, the model
    /// is unconfigured, or the provider request fails, so callers fall back.
    #[cfg(feature = "lancedb-vector")]
    pub(crate) async fn embed_texts(
        &self,
        config: &AgentConfig,
        inputs: &[String],
    ) -> Option<Vec<Vec<f32>>> {
        if !config.semantic.embedding.enabled || inputs.is_empty() {
            return None;
        }
        if resolved_embedding_model(config).is_empty() {
            return None;
        }
        match request_embeddings(&self.http_client, config, inputs).await {
            Ok(vectors) if !vectors.is_empty() => Some(vectors),
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(error = %error, "embed_texts request failed; falling back");
                None
            }
        }
    }

    #[cfg(not(feature = "lancedb-vector"))]
    pub(crate) async fn embed_texts(
        &self,
        _config: &AgentConfig,
        _inputs: &[String],
    ) -> Option<Vec<Vec<f32>>> {
        None
    }

    #[cfg(feature = "lancedb-vector")]
    pub(crate) async fn semantic_document_scores(
        &self,
        query: &str,
        source_kind: &str,
        limit: usize,
        config: &AgentConfig,
    ) -> Result<HashMap<String, f64>> {
        if !config.semantic.embedding.enabled || query.trim().is_empty() || limit == 0 {
            return Ok(HashMap::new());
        }
        let embedding_model = resolved_embedding_model(config);
        if embedding_model.is_empty() {
            return Ok(HashMap::new());
        }
        let vector_source_kind = match source_kind {
            "skill" => crate::history::vector_index::VectorSourceKind::Skill,
            "guideline" => crate::history::vector_index::VectorSourceKind::Guideline,
            other => anyhow::bail!("unsupported semantic document kind '{other}'"),
        };
        let query_embeddings =
            request_embeddings(&self.http_client, config, &[query.trim().to_string()]).await?;
        let Some(query_embedding) = query_embeddings.into_iter().next() else {
            return Ok(HashMap::new());
        };

        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let hits = index
            .search(crate::history::vector_index::VectorSearchRequest {
                embedding: query_embedding,
                embedding_model,
                limit,
                source_kinds: vec![vector_source_kind],
                workspace_id: None,
                thread_id: None,
                agent_id: None,
            })
            .await?;

        Ok(hits
            .into_iter()
            .map(|hit| (hit.source_id, semantic_score_from_distance(hit.score)))
            .collect())
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn semantic_vector_history_search(
        &self,
        _query: &str,
        _limit: usize,
        _config: &AgentConfig,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        Ok((
            "Semantic vector search is not compiled in.".to_string(),
            Vec::new(),
        ))
    }

    #[cfg(feature = "lancedb-vector")]
    async fn semantic_vector_history_search(
        &self,
        query: &str,
        limit: usize,
        config: &AgentConfig,
    ) -> Result<(String, Vec<HistorySearchHit>)> {
        let embedding_model = resolved_embedding_model(config);
        if embedding_model.is_empty() {
            return Ok((
                "No semantic embedding model configured.".to_string(),
                Vec::new(),
            ));
        }
        let query_embeddings =
            request_embeddings(&self.http_client, config, &[query.trim().to_string()]).await?;
        let Some(query_embedding) = query_embeddings.into_iter().next() else {
            return Ok(("No query embedding returned.".to_string(), Vec::new()));
        };
        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let vector_hits = index
            .search(crate::history::vector_index::VectorSearchRequest {
                embedding: query_embedding,
                embedding_model,
                limit,
                source_kinds: vec![
                    crate::history::vector_index::VectorSourceKind::AgentMessage,
                    crate::history::vector_index::VectorSourceKind::AgentTask,
                ],
                workspace_id: None,
                thread_id: None,
                agent_id: None,
            })
            .await?;
        let hits = vector_hits_to_history_hits(vector_hits);
        Ok((
            format!(
                "Found {} semantic history matches for '{}'.",
                hits.len(),
                query
            ),
            hits,
        ))
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn process_embedding_deletions_once(&self) -> Result<usize> {
        Ok(0)
    }

    #[cfg(feature = "lancedb-vector")]
    async fn process_embedding_deletions_once(&self) -> Result<usize> {
        let _guard = self.semantic_vector_index_lock.lock().await;
        let deletions = self
            .history
            .claim_embedding_deletions(MAX_EMBEDDING_DELETIONS_PER_TICK)
            .await?;
        if deletions.is_empty() {
            return Ok(0);
        }
        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let mut supported = Vec::new();
        for deletion in deletions {
            let Some(source_kind) =
                crate::history::vector_index::VectorSourceKind::from_embedding_source_kind(
                    &deletion.source_kind,
                )
            else {
                self.history.complete_embedding_deletion(&deletion).await?;
                continue;
            };
            supported.push((deletion, source_kind));
        }
        if supported.is_empty() {
            return Ok(0);
        }
        let sources = supported
            .iter()
            .map(|(deletion, source_kind)| (*source_kind, deletion.source_id.clone()))
            .collect::<Vec<_>>();
        match index.delete_sources(sources).await {
            Ok(()) => {
                for (deletion, _) in &supported {
                    self.history.complete_embedding_deletion(deletion).await?;
                }
                Ok(supported.len())
            }
            Err(error) => {
                let message = error.to_string();
                for (deletion, _) in &supported {
                    self.history
                        .fail_embedding_deletion(deletion, &message)
                        .await?;
                }
                Err(error)
            }
        }
    }

    #[cfg(not(feature = "lancedb-vector"))]
    async fn process_embedding_queue_once_inner(&self, _config: AgentConfig) -> Result<usize> {
        Ok(0)
    }

    #[cfg(feature = "lancedb-vector")]
    async fn process_embedding_queue_once_inner(&self, config: AgentConfig) -> Result<usize> {
        let _guard = self.semantic_vector_index_lock.lock().await;
        let embedding_model = resolved_embedding_model(&config);
        if embedding_model.is_empty() {
            return Ok(0);
        }
        let dimensions = config.semantic.embedding.dimensions;
        let batch_size = embedding_batch_size(config.semantic.embedding.batch_size);
        let jobs = self
            .history
            .claim_embedding_jobs(&embedding_model, dimensions, batch_size)
            .await?;
        if jobs.is_empty() {
            return Ok(0);
        }

        let inputs = jobs.iter().map(|job| job.body.clone()).collect::<Vec<_>>();
        let embeddings = match request_embeddings(&self.http_client, &config, &inputs).await {
            Ok(embeddings) => embeddings,
            Err(error) => {
                let message = error.to_string();
                let _ = self.history.fail_embedding_jobs(&jobs, &message).await;
                return Err(error);
            }
        };

        let index = crate::history::vector_index::VectorIndex::open(self.history.data_root());
        let mut vector_jobs = Vec::new();
        let mut documents = Vec::new();
        for (job, embedding) in jobs.into_iter().zip(embeddings) {
            let Some(source_kind) =
                crate::history::vector_index::VectorSourceKind::from_embedding_source_kind(
                    &job.source_kind,
                )
            else {
                self.history
                    .fail_embedding_job(&job, "unsupported embedding source kind")
                    .await?;
                continue;
            };
            documents.push(crate::history::vector_index::VectorDocument {
                source_kind,
                source_id: job.source_id.clone(),
                chunk_id: job.chunk_id.clone(),
                title: job.title.clone(),
                body: job.body.clone(),
                workspace_id: job.workspace_id.clone(),
                thread_id: job.thread_id.clone(),
                agent_id: job.agent_id.clone(),
                timestamp: job.source_timestamp,
                embedding_model: embedding_model.clone(),
                embedding,
                metadata_json: Some(
                    serde_json::json!({ "content_hash": job.content_hash }).to_string(),
                ),
            });
            vector_jobs.push(job);
        }
        match index.upsert_many(documents).await {
            Ok(()) => {
                self.history
                    .complete_embedding_jobs(&vector_jobs, &embedding_model, dimensions)
                    .await?;
                Ok(vector_jobs.len())
            }
            Err(error) => {
                let message = error.to_string();
                self.history
                    .fail_embedding_jobs(&vector_jobs, &message)
                    .await?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_like_endpoint_appends_v1_once() {
        assert_eq!(
            openai_like_endpoint("https://api.openai.com/v1", "embeddings"),
            "https://api.openai.com/v1/embeddings"
        );
        assert_eq!(
            openai_like_endpoint("http://localhost:11434", "/embeddings"),
            "http://localhost:11434/v1/embeddings"
        );
    }

    #[test]
    fn embedding_batch_size_is_bounded() {
        assert_eq!(embedding_batch_size(0), 1);
        assert_eq!(embedding_batch_size(8), 8);
        assert_eq!(embedding_batch_size(64), MAX_EMBEDDING_BATCH_SIZE);
        assert_eq!(embedding_batch_size(10_000), MAX_EMBEDDING_BATCH_SIZE);
    }

    #[cfg(feature = "lancedb-vector")]
    #[tokio::test]
    async fn openrouter_embedding_requests_include_app_attribution_headers() {
        let request_texts = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind embedding request listener");
        let addr = listener.local_addr().expect("embedding listener addr");
        let request_texts_for_server = std::sync::Arc::clone(&request_texts);

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept embedding request");
            let mut buf = [0_u8; 4096];
            let size = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
            )
            .await
            .expect("read embedding request timed out")
            .expect("read embedding request");
            let request = String::from_utf8_lossy(&buf[..size]).to_string();
            request_texts_for_server
                .lock()
                .expect("lock embedding request texts")
                .push(request);

            let body = r#"{"data":[{"index":0,"embedding":[0.1,0.2]}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes())
                .await
                .expect("write embedding response");
        });

        let mut config = AgentConfig::default();
        config.provider = zorai_shared::providers::PROVIDER_ID_OPENROUTER.to_string();
        config.base_url = format!("http://{addr}");
        config.api_key = "openrouter-key".to_string();
        config.semantic.embedding.provider =
            zorai_shared::providers::PROVIDER_ID_OPENROUTER.to_string();
        config.semantic.embedding.model = "openai/text-embedding-3-small".to_string();
        config.semantic.embedding.dimensions = 2;

        let embeddings =
            request_embeddings(&reqwest::Client::new(), &config, &["hello".to_string()])
                .await
                .expect("embedding request should succeed");

        server.await.expect("embedding server task");

        let request_text = request_texts
            .lock()
            .expect("lock embedding request texts")
            .first()
            .cloned()
            .expect("request text should be recorded");
        assert!(
            request_text.starts_with("POST /v1/embeddings "),
            "expected OpenRouter embedding request to call embeddings endpoint, got {request_text}"
        );
        assert!(
            request_text.contains("http-referer: https://zorai.app\r\n"),
            "expected OpenRouter embedding request to include attribution referer header, got {request_text}"
        );
        assert!(
            request_text.contains("x-openrouter-title: Zorai\r\n"),
            "expected OpenRouter embedding request to include attribution title header, got {request_text}"
        );
        assert!(
            request_text.contains("x-openrouter-categories: cli-agent,personal-agent\r\n"),
            "expected OpenRouter embedding request to include attribution categories header, got {request_text}"
        );
        assert_eq!(embeddings, vec![vec![0.1, 0.2]]);
    }

    #[cfg(feature = "lancedb-vector")]
    #[test]
    fn embedding_provider_error_envelope_is_reported() {
        let err = parse_embedding_response(
            r#"{"error":{"message":"HTTP 400: input too large","code":400}}"#,
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("embedding provider returned error: HTTP 400: input too large"));
    }

    #[cfg(feature = "lancedb-vector")]
    #[test]
    fn vector_history_hits_are_rendered_as_protocol_hits() {
        let hits =
            vector_hits_to_history_hits(vec![crate::history::vector_index::VectorSearchHit {
                source_kind: crate::history::vector_index::VectorSourceKind::AgentMessage,
                source_id: "msg-1".to_string(),
                chunk_id: "0".to_string(),
                title: "user".to_string(),
                snippet: Some("body".to_string()),
                timestamp: Some(42),
                score: 0.25,
                metadata_json: None,
            }]);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "msg-1");
        assert_eq!(hits[0].kind, "agent_message");
        assert_eq!(hits[0].excerpt, "body");
        assert!((hits[0].score - 0.8).abs() < f64::EPSILON);
    }
}
