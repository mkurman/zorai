#![allow(dead_code)]

use super::*;
use crate::agent::types::SkillRecommendationConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zorai_protocol::SessionId;

const MAX_SKILL_PREFLIGHT_MATCHES: usize = 3;
const SKILL_DISCOVERY_NORMALIZER_MARKER: &str = "[[skill_discovery_query_normalizer]]";
const SKILL_DISCOVERY_SEMANTIC_SHORTLIST_MARKER: &str = "[[skill_discovery_semantic_shortlist]]";
const MAX_NORMALIZED_SKILL_QUERY_CHARS: usize = 160;
const LOCAL_SKILL_DISCOVERY_NORMALIZER_ID: &str = "local-skill-discovery-heuristic";
pub(crate) const SKILL_DISCOVERY_WORKER_ARG: &str = "__zorai-skill-discovery-worker";
const ZORAI_DAEMON_BIN_ENV: &str = "ZORAI_DAEMON_BIN";

pub(crate) struct SkillPreflightContext {
    pub prompt_context: String,
    pub state: LatestSkillDiscoveryState,
    pub workflow_message: String,
    pub workflow_details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AsyncSkillDiscoveryRequest {
    pub thread_id: String,
    pub query: String,
    pub context_tags: Vec<String>,
    pub limit: usize,
    pub history_root: PathBuf,
    pub cfg: SkillRecommendationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AsyncSkillDiscoveryCompletion {
    pub thread_id: String,
    pub query: String,
    pub state: LatestSkillDiscoveryState,
    pub context_tags: Vec<String>,
    pub cfg: SkillRecommendationConfig,
    pub result: super::skill_recommendation::SkillDiscoveryResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct SemanticSkillCatalogEntry {
    record: crate::history::SkillVariantRecord,
    metadata: crate::agent::skill_recommendation::SkillDocumentMetadata,
    excerpt: String,
}

#[derive(Debug, Clone)]
struct SemanticResearchFallback {
    result: super::skill_recommendation::SkillDiscoveryResult,
}

#[cfg(test)]
pub(crate) trait SkillDiscoveryTestRunner: Send + Sync {
    fn spawn(
        &self,
        request: AsyncSkillDiscoveryRequest,
        result_tx: tokio::sync::mpsc::UnboundedSender<AsyncSkillDiscoveryCompletion>,
    );
}

#[cfg(test)]
#[derive(Clone)]
struct DelayedTestSkillDiscoveryRunner {
    started: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
    completion: AsyncSkillDiscoveryCompletion,
}

#[cfg(test)]
impl SkillDiscoveryTestRunner for DelayedTestSkillDiscoveryRunner {
    fn spawn(
        &self,
        request: AsyncSkillDiscoveryRequest,
        result_tx: tokio::sync::mpsc::UnboundedSender<AsyncSkillDiscoveryCompletion>,
    ) {
        self.started.store(true, Ordering::SeqCst);
        let release = self.release.clone();
        let mut completion = self.completion.clone();
        completion.thread_id = request.thread_id;
        completion.query = request.query;
        tokio::spawn(async move {
            release.notified().await;
            let _ = result_tx.send(completion);
        });
    }
}

#[cfg(test)]
pub(crate) fn make_delayed_test_skill_discovery_runner(
    started: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
    completion: AsyncSkillDiscoveryCompletion,
) -> Arc<dyn SkillDiscoveryTestRunner> {
    Arc::new(DelayedTestSkillDiscoveryRunner {
        started,
        release,
        completion,
    })
}

#[cfg(test)]
pub(crate) fn sample_test_skill_discovery_completion(
    query: &str,
    skill_name: &str,
) -> AsyncSkillDiscoveryCompletion {
    AsyncSkillDiscoveryCompletion {
        thread_id: String::new(),
        query: query.to_string(),
        state: LatestSkillDiscoveryState {
            query: query.to_string(),
            confidence_tier: "strong".to_string(),
            recommended_skill: Some(skill_name.to_string()),
            recommended_action: format!("read_skill {skill_name}"),
            mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
            mesh_requires_approval: false,
            mesh_approval_id: None,
            read_skill_identifier: Some(skill_name.to_string()),
            skip_rationale: None,
            discovery_pending: false,
            skill_read_completed: false,
            compliant: false,
            updated_at: now_millis(),
        },
        context_tags: Vec::new(),
        cfg: SkillRecommendationConfig::default(),
        result: super::skill_recommendation::SkillDiscoveryResult::default(),
        error: None,
    }
}

pub(super) fn preserve_noncompliant_mesh_state(
    previous_state: &LatestSkillDiscoveryState,
    next_state: &mut LatestSkillDiscoveryState,
) -> bool {
    if previous_state.compliant {
        return false;
    }

    let previous_mesh_next_step = previous_state
        .mesh_next_step
        .unwrap_or_else(|| previous_state.effective_mesh_next_step());
    let has_preservable_mesh_guidance = previous_state.mesh_requires_approval
        || (matches!(
            previous_mesh_next_step,
            crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill
                | crate::agent::skill_mesh::types::SkillMeshNextStep::ChooseOrBypass
        ) && (previous_state.recommended_skill.is_some()
            || previous_state.read_skill_identifier.is_some()));
    if !has_preservable_mesh_guidance {
        return false;
    }

    let preserved_skill = next_state
        .recommended_skill
        .clone()
        .or_else(|| previous_state.recommended_skill.clone());
    next_state.recommended_skill = preserved_skill.clone();
    next_state.mesh_requires_approval = previous_state.mesh_requires_approval;
    next_state.mesh_approval_id = if previous_state.mesh_requires_approval {
        previous_state.mesh_approval_id.clone()
    } else {
        None
    };
    next_state.mesh_next_step = previous_state
        .mesh_next_step
        .or(Some(previous_mesh_next_step));
    next_state.read_skill_identifier = next_state
        .read_skill_identifier
        .clone()
        .or_else(|| preserved_skill.clone())
        .or_else(|| previous_state.read_skill_identifier.clone())
        .or_else(|| previous_state.recommended_skill.clone());
    next_state.skill_read_completed = previous_state.skill_read_completed;
    if next_state.is_discovery_pending() {
        next_state.confidence_tier = previous_state.confidence_tier.clone();
    }
    next_state.compliant = false;
    next_state.recommended_action = if next_state.mesh_requires_approval {
        preserved_skill
            .as_deref()
            .map(|skill| format!("request_approval {skill}"))
            .unwrap_or_else(|| previous_state.recommended_action.clone())
    } else {
        match next_state
            .mesh_next_step
            .unwrap_or(crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip)
        {
            crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill
            | crate::agent::skill_mesh::types::SkillMeshNextStep::ChooseOrBypass => preserved_skill
                .as_deref()
                .map(|skill| format!("read_skill {skill}"))
                .unwrap_or_else(|| previous_state.recommended_action.clone()),
            crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip => {
                zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP.to_string()
            }
        }
    };
    true
}

pub(super) fn spawn_skill_discovery_result_applier(
    engine: Arc<AgentEngine>,
    mut result_rx: tokio::sync::mpsc::UnboundedReceiver<AsyncSkillDiscoveryCompletion>,
) {
    tokio::spawn(async move {
        while let Some(completion) = result_rx.recv().await {
            apply_async_skill_discovery_completion(engine.as_ref(), completion).await;
        }
    });
}

impl AgentEngine {
    pub(crate) async fn discover_skill_recommendations_public(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<zorai_protocol::SkillDiscoveryResultPublic> {
        let context = self
            .run_public_skill_discovery(query, session_id, 512)
            .await?;

        super::skill_recommendation::page_public_discovery_result(
            query,
            &context.normalized_intent,
            &context.context_tags,
            &context.result,
            &context.cfg,
            cursor,
            limit,
        )
    }

    pub(crate) async fn discover_guideline_recommendations_public(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<zorai_protocol::SkillDiscoveryResultPublic> {
        let guidelines_root = super::guidelines_dir(self.history.data_dir());
        let context_tags = resolve_skill_discovery_context_tags(
            self.workspace_root.as_ref(),
            &self.session_manager,
            session_id,
            self.plugin_manager.get().map(|manager| manager.as_ref()),
        )
        .await;
        let agent_config = self.config.read().await.clone();
        let cfg = agent_config.skill_recommendation.clone();
        let result = if agent_config.semantic.embedding.enabled {
            let _ = self.sync_semantic_document_indexes_once().await;
            let semantic_scores = self
                .semantic_document_scores(query, "guideline", 512, &agent_config)
                .await
                .unwrap_or_default();
            if semantic_scores.is_empty() {
                super::skill_recommendation::discover_local_guidelines(
                    &self.history,
                    &guidelines_root,
                    query,
                    &context_tags,
                    512,
                    &cfg,
                )
                .await?
            } else {
                super::skill_recommendation::discover_local_guidelines_with_semantic_scores(
                    &self.history,
                    &guidelines_root,
                    query,
                    &context_tags,
                    512,
                    &cfg,
                    &semantic_scores,
                )
                .await?
            }
        } else {
            super::skill_recommendation::discover_local_guidelines(
                &self.history,
                &guidelines_root,
                query,
                &context_tags,
                512,
                &cfg,
            )
            .await?
        };

        super::skill_recommendation::page_public_discovery_result_with_action(
            query,
            query.trim(),
            &context_tags,
            &result,
            &cfg,
            cursor,
            limit,
            zorai_protocol::tool_names::READ_GUIDELINE,
            Some("guideline"),
        )
    }

    pub(super) async fn build_skill_preflight_context(
        &self,
        thread_id: &str,
        content: &str,
        session_id: Option<SessionId>,
    ) -> Result<Option<SkillPreflightContext>> {
        if !should_run_skill_preflight(content) {
            return Ok(None);
        }

        let skill_query = compact_skill_discovery_query_for_persistence(content);
        let previous_state = self.get_thread_skill_discovery_state(thread_id).await;
        if let Some(state) = previous_state.as_ref() {
            if state.is_discovery_pending() && state.query == skill_query {
                return Ok(Some(build_skill_preflight_context_from_state(
                    state.clone(),
                )));
            }
        }

        let cfg = self.config.read().await.skill_recommendation.clone();

        #[cfg(test)]
        if self.skill_discovery_test_runner.get().is_none() {
            let context_tags = resolve_skill_discovery_context_tags(
                self.workspace_root.as_ref(),
                &self.session_manager,
                session_id,
                self.plugin_manager.get().map(|manager| manager.as_ref()),
            )
            .await;
            let mut state = build_latest_skill_discovery_state(
                &skill_query,
                &execute_skill_discovery_backend(
                    self.force_mesh_discovery_degraded_for_tests
                        .load(std::sync::atomic::Ordering::SeqCst),
                    &self.history,
                    &self.history.data_dir().to_path_buf(),
                    &skill_query,
                    &context_tags,
                    MAX_SKILL_PREFLIGHT_MATCHES,
                    &cfg,
                )
                .await?
                .0,
            );
            if let Some(previous_state) = previous_state.as_ref() {
                preserve_noncompliant_mesh_state(previous_state, &mut state);
            }
            self.set_thread_skill_discovery_state(thread_id, state.clone())
                .await;
            return Ok(Some(build_skill_preflight_context_from_state(state)));
        }

        let mut pending_state = build_pending_skill_discovery_state(&skill_query);
        if let Some(previous_state) = previous_state.as_ref() {
            preserve_noncompliant_mesh_state(previous_state, &mut pending_state);
        }

        self.set_thread_skill_discovery_state(thread_id, pending_state.clone())
            .await;
        self.spawn_background_skill_discovery(
            AsyncSkillDiscoveryRequest {
                thread_id: thread_id.to_string(),
                query: skill_query,
                context_tags: Vec::new(),
                limit: MAX_SKILL_PREFLIGHT_MATCHES,
                history_root: self.history.data_root().to_path_buf(),
                cfg,
            },
            session_id,
        );

        Ok(Some(build_skill_preflight_context_from_state(
            pending_state,
        )))
    }

    async fn run_public_skill_discovery(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<SkillDiscoveryComputation> {
        self.run_skill_discovery(query, session_id, limit).await
    }

    fn spawn_background_skill_discovery(
        &self,
        mut request: AsyncSkillDiscoveryRequest,
        session_id: Option<SessionId>,
    ) {
        #[cfg(test)]
        if let Some(runner) = self.skill_discovery_test_runner.get() {
            runner.spawn(request, self.skill_discovery_result_tx.clone());
            return;
        }

        let result_tx = self.skill_discovery_result_tx.clone();
        let workspace_root = self.workspace_root.clone();
        let session_manager = self.session_manager.clone();
        let plugin_manager = self.plugin_manager.get().cloned();
        tokio::spawn(async move {
            request.context_tags = resolve_skill_discovery_context_tags(
                workspace_root.as_ref(),
                &session_manager,
                session_id,
                plugin_manager.as_deref(),
            )
            .await;
            let completion = compute_async_skill_discovery_completion(request).await;
            let _ = result_tx.send(completion);
        });
    }

    pub(crate) async fn refresh_thread_skill_discovery_state(
        &self,
        thread_id: &str,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<LatestSkillDiscoveryState> {
        let context = self.run_skill_discovery(query, session_id, limit).await?;
        let state = build_latest_skill_discovery_state(query, &context.result);
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Ok(state)
    }

    pub(crate) async fn record_thread_skill_skip_rationale(
        &self,
        thread_id: &str,
        rationale: &str,
    ) -> Result<LatestSkillDiscoveryState> {
        let trimmed = rationale.trim();
        if trimmed.is_empty() {
            anyhow::bail!("skip rationale must not be empty");
        }

        let mut state = self
            .get_thread_skill_discovery_state(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no active skill discovery state for this thread"))?;
        state.skip_rationale = Some(trimmed.to_string());
        state.compliant =
            !state.requires_skill_read_before_progress() && !state.mesh_requires_approval;
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Ok(state)
    }

    pub(crate) async fn record_thread_skill_approval_resolution(
        &self,
        thread_id: &str,
        approval_id: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        let mut state = self.get_thread_skill_discovery_state(thread_id).await?;
        if state.mesh_approval_id.as_deref() != Some(approval_id) {
            return Some(state);
        }
        state.mesh_requires_approval = false;
        state.mesh_approval_id = None;
        state.compliant =
            state.skill_read_completed || !state.requires_skill_read_before_progress();
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Some(state)
    }

    pub(crate) async fn record_thread_skill_approval_denial(
        &self,
        thread_id: &str,
        approval_id: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        let mut state = self.get_thread_skill_discovery_state(thread_id).await?;
        if state.mesh_approval_id.as_deref() != Some(approval_id) {
            return Some(state);
        }
        state.mesh_requires_approval = false;
        state.mesh_approval_id = None;
        state.mesh_next_step =
            Some(crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip);
        state.recommended_action = zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP.to_string();
        state.compliant = false;
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Some(state)
    }

    pub(crate) async fn record_thread_skill_read_compliance(
        &self,
        thread_id: &str,
        skill_identifier: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        let mut state = self.get_thread_skill_discovery_state(thread_id).await?;
        let expected = state
            .read_skill_identifier
            .as_deref()
            .or(state.recommended_skill.as_deref())?;
        if !skill_identifier_matches(expected, skill_identifier) {
            if state
                .recommended_skill
                .as_deref()
                .is_some_and(|expected_skill| {
                    skill_identifier_matches(expected_skill, skill_identifier)
                })
            {
                state.skill_read_completed = true;
                state.compliant = !state.mesh_requires_approval;
                state.updated_at = now_millis();
                self.set_thread_skill_discovery_state(thread_id, state.clone())
                    .await;
            }
            return Some(state);
        }

        state.skill_read_completed = true;
        state.compliant = !state.mesh_requires_approval;
        state.updated_at = now_millis();
        self.set_thread_skill_discovery_state(thread_id, state.clone())
            .await;
        Some(state)
    }

    async fn run_skill_discovery(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        limit: usize,
    ) -> Result<SkillDiscoveryComputation> {
        let skills_root = self.history.data_dir().to_path_buf();
        let context_tags = resolve_skill_discovery_context_tags(
            self.workspace_root.as_ref(),
            &self.session_manager,
            session_id,
            self.plugin_manager.get().map(|manager| manager.as_ref()),
        )
        .await;
        let agent_config = self.config.read().await.clone();
        let cfg = agent_config.skill_recommendation.clone();
        let (result, mut backend_used, mesh_degraded) = execute_skill_discovery_backend(
            #[cfg(test)]
            self.force_mesh_discovery_degraded_for_tests
                .load(std::sync::atomic::Ordering::SeqCst),
            #[cfg(not(test))]
            false,
            &self.history,
            &skills_root,
            query,
            &context_tags,
            limit,
            &cfg,
        )
        .await?;
        let mut normalized_intent = query.trim().to_string();
        let result = if agent_config.semantic.embedding.enabled {
            let _ = self.sync_semantic_document_indexes_once().await;
            match self
                .semantic_document_scores(
                    query,
                    "skill",
                    limit.saturating_mul(4).max(16),
                    &agent_config,
                )
                .await
            {
                Ok(semantic_scores) if !semantic_scores.is_empty() => {
                    let semantic_result =
                        super::skill_recommendation::discover_local_skills_with_semantic_scores(
                            &self.history,
                            &skills_root,
                            query,
                            &context_tags,
                            limit,
                            &cfg,
                            &semantic_scores,
                        )
                        .await?;
                    if fallback_result_is_better(&result, &semantic_result) {
                        semantic_result
                    } else {
                        result
                    }
                }
                Ok(_) => result,
                Err(error) => {
                    tracing::warn!(%error, "semantic skill discovery vector search failed");
                    result
                }
            }
        } else {
            result
        };
        let mut result = if cfg.llm_normalize_on_no_match
            && should_attempt_query_normalization(query, &result)
        {
            match self
                .normalize_skill_discovery_query(query, &context_tags, session_id)
                .await
            {
                Some((normalized_query, normalizer_agent_id)) => {
                    let (mut normalized_result, normalized_backend_used, normalized_mesh_degraded) =
                        execute_skill_discovery_backend(
                            #[cfg(test)]
                            self.force_mesh_discovery_degraded_for_tests
                                .load(std::sync::atomic::Ordering::SeqCst),
                            #[cfg(not(test))]
                            false,
                            &self.history,
                            &skills_root,
                            &normalized_query,
                            &context_tags,
                            limit,
                            &cfg,
                        )
                        .await?;
                    if fallback_result_is_better(&result, &normalized_result) {
                        backend_used = normalized_backend_used;
                        annotate_fallback_result(
                            &mut normalized_result,
                            &normalized_query,
                            normalizer_agent_id,
                        );
                        if normalized_mesh_degraded {
                            annotate_mesh_degraded_fallback(&mut normalized_result);
                        }
                        normalized_intent = normalized_query;
                        normalized_result
                    } else {
                        result
                    }
                }
                None => result,
            }
        } else {
            result
        };
        if should_attempt_semantic_skill_research(&result, &cfg) {
            if let Some(fallback) = self
                .attempt_semantic_skill_research(
                    query,
                    session_id,
                    &context_tags,
                    &skills_root,
                    &cfg,
                    limit,
                )
                .await
            {
                if fallback_result_is_better(&result, &fallback.result) {
                    result = fallback.result;
                }
            }
        }
        if mesh_degraded {
            annotate_mesh_degraded_fallback(&mut result);
        }
        Ok(SkillDiscoveryComputation {
            context_tags,
            cfg,
            result,
            normalized_intent,
            backend_used,
            mesh_degraded,
        })
    }

    async fn normalize_skill_discovery_query(
        &self,
        query: &str,
        context_tags: &[String],
        session_id: Option<SessionId>,
    ) -> Option<(String, &'static str)> {
        if let Some(normalized_query) = heuristic_skill_discovery_query_rewrite(query, context_tags)
        {
            return Some((normalized_query, LOCAL_SKILL_DISCOVERY_NORMALIZER_ID));
        }

        let normalizer_agent_id = normalization_agent_id_for_query(query);
        let prompt = build_skill_discovery_normalization_prompt(query, context_tags);
        let preferred_session_hint = session_id.as_ref().map(ToString::to_string);

        let response = match self
            .send_internal_agent_message(
                MAIN_AGENT_ID,
                normalizer_agent_id,
                &prompt,
                preferred_session_hint.as_deref(),
            )
            .await
        {
            Ok(result) => result.response,
            Err(error) => {
                tracing::warn!(
                    %error,
                    normalizer = %normalizer_agent_id,
                    "skill discovery normalization fallback failed"
                );
                return None;
            }
        };

        let normalized_query = parse_normalized_skill_query(&response)?;
        if normalized_query.eq_ignore_ascii_case(query.trim()) {
            return None;
        }

        Some((normalized_query, normalizer_agent_id))
    }

    async fn attempt_semantic_skill_research(
        &self,
        query: &str,
        session_id: Option<SessionId>,
        context_tags: &[String],
        skills_root: &PathBuf,
        cfg: &SkillRecommendationConfig,
        limit: usize,
    ) -> Option<SemanticResearchFallback> {
        let catalog = self
            .load_semantic_skill_catalog(skills_root, cfg)
            .await
            .ok()?;
        if catalog.is_empty() {
            return None;
        }

        let research_agent_id = normalization_agent_id_for_query(query);
        let prompt = build_skill_semantic_shortlist_prompt(query, context_tags, &catalog);
        let preferred_session_hint = session_id.as_ref().map(ToString::to_string);
        let response = match self
            .send_internal_agent_message(
                MAIN_AGENT_ID,
                research_agent_id,
                &prompt,
                preferred_session_hint.as_deref(),
            )
            .await
        {
            Ok(result) => result.response,
            Err(error) => {
                tracing::warn!(
                    %error,
                    researcher = %research_agent_id,
                    "skill discovery semantic shortlist fallback failed"
                );
                return None;
            }
        };

        let selected = parse_semantic_skill_shortlist(&response)?;
        if selected.is_empty() {
            return None;
        }

        let result =
            semantic_shortlist_to_result(&catalog, &selected, cfg, limit, research_agent_id);
        (!result.recommendations.is_empty()).then_some(SemanticResearchFallback { result })
    }

    async fn load_semantic_skill_catalog(
        &self,
        skills_root: &PathBuf,
        cfg: &SkillRecommendationConfig,
    ) -> Result<Vec<SemanticSkillCatalogEntry>> {
        let mut records = self.history.list_discoverable_skill_variants(512).await?;
        if records.is_empty() {
            super::skill_recommendation::sync_skill_catalog(&self.history, skills_root).await?;
            records = self.history.list_discoverable_skill_variants(512).await?;
        }

        let mut entries = Vec::new();
        for record in records {
            let (skill_path, metadata_relative_path) =
                super::skill_recommendation::resolve_skill_document_path(
                    skills_root,
                    &record.relative_path,
                );
            let Ok(content) = std::fs::read_to_string(&skill_path) else {
                continue;
            };
            entries.push(SemanticSkillCatalogEntry {
                metadata: super::skill_recommendation::extract_skill_metadata(
                    &metadata_relative_path,
                    &content,
                ),
                excerpt: content.lines().take(8).collect::<Vec<_>>().join("\n"),
                record,
            });
        }

        entries.sort_by(|left, right| {
            right
                .record
                .use_count
                .cmp(&left.record.use_count)
                .then_with(|| left.record.skill_name.cmp(&right.record.skill_name))
        });
        entries.truncate(cfg.llm_semantic_search_max_skills.max(1) as usize);
        Ok(entries)
    }
}

async fn execute_skill_discovery_backend(
    #[allow(unused_variables)] force_mesh_degraded_for_tests: bool,
    history: &HistoryStore,
    skills_root: &PathBuf,
    query: &str,
    context_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<(
    super::skill_recommendation::SkillDiscoveryResult,
    &'static str,
    bool,
)> {
    if cfg.discovery_backend.eq_ignore_ascii_case("mesh") {
        #[cfg(test)]
        if force_mesh_degraded_for_tests {
            let mut fallback = super::skill_recommendation::discover_local_skills(
                history,
                skills_root,
                query,
                context_tags,
                limit,
                cfg,
            )
            .await?;
            annotate_mesh_degraded_fallback(&mut fallback);
            return Ok((fallback, "legacy", true));
        }

        #[cfg(not(feature = "lancedb-vector"))]
        {
            let mut fallback = super::skill_recommendation::discover_local_skills(
                history,
                skills_root,
                query,
                context_tags,
                limit,
                cfg,
            )
            .await?;
            annotate_mesh_degraded_fallback(&mut fallback);
            return Ok((fallback, "legacy", true));
        }

        #[cfg(feature = "lancedb-vector")]
        {
            let result = super::skill_mesh::retrieval::discover_local_skills_via_mesh(
                history,
                skills_root,
                query,
                context_tags,
                limit,
                cfg,
            )
            .await?;
            if result.recommendations.is_empty()
                || matches!(
                    result.confidence,
                    super::skill_recommendation::SkillRecommendationConfidence::None
                )
            {
                let fallback = super::skill_recommendation::discover_local_skills(
                    history,
                    skills_root,
                    query,
                    context_tags,
                    limit,
                    cfg,
                )
                .await?;
                if !fallback.recommendations.is_empty()
                    || !matches!(
                        fallback.confidence,
                        super::skill_recommendation::SkillRecommendationConfidence::None
                    )
                {
                    return Ok((fallback, "legacy", false));
                }
            }
            return Ok((result, "mesh", false));
        }
    }

    let result = super::skill_recommendation::discover_local_skills(
        history,
        skills_root,
        query,
        context_tags,
        limit,
        cfg,
    )
    .await?;
    Ok((result, "legacy", false))
}

fn platform_daemon_binary_name() -> &'static str {
    if cfg!(windows) {
        "zorai-daemon.exe"
    } else {
        "zorai-daemon"
    }
}

fn resolve_daemon_worker_executable_candidate(
    current_exe: Option<&std::path::Path>,
    env_override: Option<&OsStr>,
) -> Option<PathBuf> {
    let override_path = env_override
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if override_path.is_some() {
        return override_path;
    }

    let current_exe = current_exe?;
    let daemon_name = OsStr::new(platform_daemon_binary_name());
    if current_exe
        .file_name()
        .is_some_and(|name| name == daemon_name)
    {
        return Some(current_exe.to_path_buf());
    }

    current_exe
        .parent()
        .map(|dir| dir.join(platform_daemon_binary_name()))
        .filter(|candidate| candidate.exists())
}

pub(crate) fn resolve_daemon_worker_executable() -> Result<PathBuf> {
    let current_exe = std::env::current_exe().context("resolve current executable")?;
    if let Some(candidate) = resolve_daemon_worker_executable_candidate(
        Some(current_exe.as_path()),
        std::env::var_os(ZORAI_DAEMON_BIN_ENV).as_deref(),
    ) {
        return Ok(candidate);
    }

    if let Ok(path) = which::which(platform_daemon_binary_name()) {
        return Ok(path);
    }

    anyhow::bail!(
        "resolve zorai-daemon executable: set {ZORAI_DAEMON_BIN_ENV} or place {} next to {}",
        platform_daemon_binary_name(),
        current_exe.display()
    );
}

pub(crate) async fn run_skill_discovery_worker_from_stdio() -> Result<()> {
    let mut request_bytes = Vec::new();
    tokio::io::stdin()
        .read_to_end(&mut request_bytes)
        .await
        .context("read async skill discovery request")?;
    let request = serde_json::from_slice::<AsyncSkillDiscoveryRequest>(&request_bytes)
        .context("parse async skill discovery request")?;
    let completion = compute_async_skill_discovery_completion(request).await;
    let response =
        serde_json::to_vec(&completion).context("serialize async skill discovery completion")?;
    tokio::io::stdout()
        .write_all(&response)
        .await
        .context("write async skill discovery completion")?;
    Ok(())
}

async fn compute_async_skill_discovery_completion(
    request: AsyncSkillDiscoveryRequest,
) -> AsyncSkillDiscoveryCompletion {
    let history =
        match crate::history::HistoryStore::open_for_data_root(&request.history_root).await {
            Ok(history) => history,
            Err(error) => {
                return build_failed_async_skill_discovery_completion(
                    &request.thread_id,
                    &request.query,
                    &error.to_string(),
                );
            }
        };
    let skills_root = history.data_dir().to_path_buf();
    match execute_skill_discovery_backend(
        false,
        &history,
        &skills_root,
        &request.query,
        &request.context_tags,
        request.limit,
        &request.cfg,
    )
    .await
    {
        Ok((mut result, _, mesh_degraded)) => {
            if mesh_degraded {
                annotate_mesh_degraded_fallback(&mut result);
            }
            AsyncSkillDiscoveryCompletion {
                thread_id: request.thread_id,
                query: request.query.clone(),
                state: build_latest_skill_discovery_state(&request.query, &result),
                context_tags: request.context_tags,
                cfg: request.cfg,
                result,
                error: None,
            }
        }
        Err(error) => build_failed_async_skill_discovery_completion(
            &request.thread_id,
            &request.query,
            &error.to_string(),
        ),
    }
}

async fn apply_async_skill_discovery_completion(
    engine: &AgentEngine,
    completion: AsyncSkillDiscoveryCompletion,
) {
    let Some(current_state) = engine
        .get_thread_skill_discovery_state(&completion.thread_id)
        .await
    else {
        return;
    };
    if !current_state.is_discovery_pending() || current_state.query != completion.query {
        return;
    }

    let mut state = completion.state.clone();
    preserve_noncompliant_mesh_state(&current_state, &mut state);
    engine
        .set_thread_skill_discovery_state(&completion.thread_id, state.clone())
        .await;
    let message = completion.error.as_ref().map_or_else(
        || skill_preflight_workflow_message(&state),
        |_| {
            format!(
                "Background skill discovery failed for `{}`. Next action: {}.",
                completion.query, state.recommended_action
            )
        },
    );
    engine.emit_workflow_notice(
        &completion.thread_id,
        "skill-preflight",
        message,
        serde_json::to_string(&completion).ok(),
    );
    let _ = engine.event_tx.send(AgentEvent::ThreadReloadRequired {
        thread_id: completion.thread_id,
    });
}

struct SkillDiscoveryComputation {
    context_tags: Vec<String>,
    cfg: SkillRecommendationConfig,
    result: super::skill_recommendation::SkillDiscoveryResult,
    normalized_intent: String,
    backend_used: &'static str,
    mesh_degraded: bool,
}

async fn resolve_skill_discovery_context_tags(
    workspace_root: Option<&PathBuf>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    plugin_manager: Option<&crate::plugin::PluginManager>,
) -> Vec<String> {
    let root = if let Some(session_id) = session_id {
        let sessions = session_manager.list().await;
        sessions
            .iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.cwd.clone())
            .map(PathBuf::from)
    } else {
        None
    }
    .or_else(|| workspace_root.cloned())
    .or_else(|| std::env::current_dir().ok());

    let mut tags = root
        .filter(|path| path.is_dir())
        .map(|path| super::semantic_env::infer_workspace_context_tags(&path))
        .unwrap_or_default();

    if let Some(plugin_manager) = plugin_manager {
        let plugins = plugin_manager.list_plugins().await;
        let mut tag_set = tags
            .into_iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        for plugin in plugins {
            if plugin.readiness_state == "ready" || plugin.readiness_state == "degraded" {
                if let Some(kind) = plugin.connector_kind.as_deref() {
                    tag_set.insert(kind.to_ascii_lowercase());
                }
                if let Some(category) = plugin.connector_category.as_deref() {
                    tag_set.insert(category.to_ascii_lowercase());
                }
            }
        }
        tags = tag_set.into_iter().collect();
    }

    tags
}

pub(crate) async fn resolve_skill_context_tags(
    workspace_root: Option<&PathBuf>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
) -> Vec<String> {
    resolve_skill_discovery_context_tags(workspace_root, session_manager, session_id, None).await
}

fn should_attempt_query_normalization(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
) -> bool {
    !query.trim().is_empty()
        && !query.contains(SKILL_DISCOVERY_NORMALIZER_MARKER)
        && (result.recommendations.is_empty()
            || matches!(
                result.confidence,
                super::skill_recommendation::SkillRecommendationConfidence::None
            ))
}

fn should_attempt_semantic_skill_research(
    result: &super::skill_recommendation::SkillDiscoveryResult,
    cfg: &SkillRecommendationConfig,
) -> bool {
    cfg.llm_semantic_search_on_no_match
        && result.recommendations.is_empty()
        && matches!(
            result.confidence,
            super::skill_recommendation::SkillRecommendationConfidence::None
        )
}

fn normalization_agent_id_for_query(query: &str) -> &'static str {
    let lower = query.to_ascii_lowercase();
    let governance_terms = [
        "approval",
        "audit",
        "compliance",
        "control-plane",
        "control plane",
        "governance",
        "guardrail",
        "policy",
        "risk",
        "safety",
    ];

    if governance_terms.iter().any(|term| lower.contains(term)) {
        WELES_AGENT_ID
    } else {
        CONCIERGE_AGENT_ID
    }
}

fn build_skill_discovery_normalization_prompt(query: &str, context_tags: &[String]) -> String {
    let workspace_tags = if context_tags.is_empty() {
        "none".to_string()
    } else {
        context_tags.join(", ")
    };

    format!(
        "{SKILL_DISCOVERY_NORMALIZER_MARKER}\nRewrite the operator request into a short local skill-discovery search query for zorai.\nReturn JSON only in the form {{\"query\":\"...\"}}.\n\nRules:\n- Focus on workflow intent, not the full task.\n- Use 3 to 8 concise keywords or trigger phrases.\n- Prefer local process terms over repository-specific prose.\n- Do not answer the task itself.\n- If the request is about governance, policy, audit, or safety, preserve that intent in compact workflow terms.\n- If no useful rewrite is possible, return {{\"query\":\"\"}}.\n\nWorkspace tags: {workspace_tags}\nOriginal request: {query}"
    )
}

fn build_skill_semantic_shortlist_prompt(
    query: &str,
    context_tags: &[String],
    catalog: &[SemanticSkillCatalogEntry],
) -> String {
    let workspace_tags = if context_tags.is_empty() {
        "none".to_string()
    } else {
        context_tags.join(", ")
    };

    let mut prompt = format!(
        "{SKILL_DISCOVERY_SEMANTIC_SHORTLIST_MARKER}\nSelect the best local zorai skills for this operator request.\nReturn JSON only in the form {{\"skills\":[\"skill-name\"]}}.\n\nRules:\n- Choose at most 3 skills.\n- Only return skill names from the catalog below.\n- Prefer semantic workflow fit over exact lexical overlap.\n- If nothing fits, return {{\"skills\":[]}}.\n\nWorkspace tags: {workspace_tags}\nOperator request: {query}\nCatalog:\n"
    );

    for entry in catalog {
        let summary = entry.metadata.summary.as_deref().unwrap_or("No summary.");
        let keywords = if entry.metadata.keywords.is_empty() {
            "none".to_string()
        } else {
            entry.metadata.keywords.join(", ")
        };
        let triggers = if entry.metadata.triggers.is_empty() {
            "none".to_string()
        } else {
            entry.metadata.triggers.join(", ")
        };
        let tags = if entry.record.context_tags.is_empty() {
            "none".to_string()
        } else {
            entry.record.context_tags.join(", ")
        };
        prompt.push_str(&format!(
            "- {skill} | summary={summary} | keywords={keywords} | triggers={triggers} | tags={tags}\n",
            skill = entry.record.skill_name
        ));
    }

    prompt
}

fn parse_normalized_skill_query(response: &str) -> Option<String> {
    let trimmed = strip_code_fences(response.trim());
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(query) = value.get("query").and_then(|value| value.as_str()) {
            return sanitize_normalized_skill_query(query);
        }
    }

    trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(sanitize_normalized_skill_query)
}

fn parse_semantic_skill_shortlist(response: &str) -> Option<Vec<String>> {
    let trimmed = strip_code_fences(response.trim());
    let mut skills = if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        value
            .get("skills")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .filter_map(sanitize_skill_shortlist_item)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        trimmed
            .lines()
            .filter_map(sanitize_skill_shortlist_item)
            .collect::<Vec<_>>()
    };
    skills.dedup();
    (!skills.is_empty()).then_some(skills)
}

fn strip_code_fences(input: &str) -> &str {
    let trimmed = input.trim();
    let stripped = trimmed.strip_prefix("```json").unwrap_or(trimmed);
    let stripped = stripped.strip_prefix("```").unwrap_or(stripped);
    stripped.strip_suffix("```").unwrap_or(stripped).trim()
}

fn sanitize_normalized_skill_query(query: &str) -> Option<String> {
    let collapsed = query
        .trim()
        .trim_matches('`')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if collapsed.is_empty() || !collapsed.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }

    let mut normalized = collapsed;
    if normalized.chars().count() > MAX_NORMALIZED_SKILL_QUERY_CHARS {
        normalized = normalized
            .chars()
            .take(MAX_NORMALIZED_SKILL_QUERY_CHARS)
            .collect::<String>()
            .trim()
            .to_string();
    }

    (!normalized.is_empty()).then_some(normalized)
}

fn sanitize_skill_shortlist_item(item: &str) -> Option<String> {
    let cleaned = item
        .trim()
        .trim_matches('`')
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn heuristic_skill_discovery_query_rewrite(query: &str, context_tags: &[String]) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let token_count = lower
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .filter(|token| token.len() >= 3)
        .count();
    if token_count < 10 {
        return None;
    }

    let has_audit_intent =
        lower.contains("audit") || lower.contains("inspect") || lower.contains("review");
    let has_diff_scope = lower.contains("git")
        || lower.contains("worktree")
        || lower.contains("diff")
        || lower.contains("patch");
    let has_governance_scope = lower.contains("governance")
        || lower.contains("policy")
        || lower.contains("compliance")
        || lower.contains("guardrail")
        || lower.contains("safety")
        || lower.contains("risk")
        || lower.contains("orchestration");

    if !(has_audit_intent && has_diff_scope && has_governance_scope) {
        return None;
    }

    let mut terms = Vec::new();
    push_rewrite_term(&mut terms, "audit");
    if lower.contains("git") {
        push_rewrite_term(&mut terms, "git");
    }
    if lower.contains("worktree") {
        push_rewrite_term(&mut terms, "worktree");
    }
    if lower.contains("diff") {
        push_rewrite_term(&mut terms, "diff");
    }

    for tag in context_tags {
        let normalized = tag.trim().to_ascii_lowercase();
        if normalized.is_empty() || !lower.contains(&normalized) {
            continue;
        }
        push_rewrite_term(&mut terms, &normalized);
        if terms.len() >= 5 {
            break;
        }
    }

    if lower.contains("orchestration") {
        push_rewrite_term(&mut terms, "orchestration");
    }
    if lower.contains("safety") {
        push_rewrite_term(&mut terms, "safety");
    }
    if lower.contains("governance") {
        push_rewrite_term(&mut terms, "governance");
    } else if lower.contains("policy") {
        push_rewrite_term(&mut terms, "policy");
    } else if lower.contains("compliance") {
        push_rewrite_term(&mut terms, "compliance");
    }

    sanitize_normalized_skill_query(&terms.join(" "))
}

fn push_rewrite_term(terms: &mut Vec<String>, term: &str) {
    if term.trim().is_empty() || terms.iter().any(|value| value == term) {
        return;
    }
    terms.push(term.to_string());
}

fn fallback_result_is_better(
    current: &super::skill_recommendation::SkillDiscoveryResult,
    fallback: &super::skill_recommendation::SkillDiscoveryResult,
) -> bool {
    let current_rank = recommendation_confidence_rank(current.confidence);
    let fallback_rank = recommendation_confidence_rank(fallback.confidence);
    if fallback_rank != current_rank {
        return fallback_rank > current_rank;
    }

    if fallback.recommendations.len() != current.recommendations.len() {
        return fallback.recommendations.len() > current.recommendations.len();
    }

    fallback
        .recommendations
        .first()
        .map(|item| item.score)
        .unwrap_or_default()
        > current
            .recommendations
            .first()
            .map(|item| item.score)
            .unwrap_or_default()
}

fn recommendation_confidence_rank(
    value: super::skill_recommendation::SkillRecommendationConfidence,
) -> u8 {
    match value {
        super::skill_recommendation::SkillRecommendationConfidence::None => 0,
        super::skill_recommendation::SkillRecommendationConfidence::Weak => 1,
        super::skill_recommendation::SkillRecommendationConfidence::Strong => 2,
    }
}

fn annotate_fallback_result(
    result: &mut super::skill_recommendation::SkillDiscoveryResult,
    normalized_query: &str,
    normalizer_agent_id: &str,
) {
    let agent_name = if normalizer_agent_id == LOCAL_SKILL_DISCOVERY_NORMALIZER_ID {
        "local heuristic"
    } else {
        canonical_agent_name(normalizer_agent_id)
    };
    let annotation = format!("normalized via {agent_name} as `{normalized_query}`");
    for recommendation in &mut result.recommendations {
        if recommendation.reason.trim().is_empty() {
            recommendation.reason = annotation.clone();
        } else {
            recommendation.reason = format!("{annotation}; {}", recommendation.reason);
        }
    }
}

fn annotate_mesh_degraded_fallback(result: &mut super::skill_recommendation::SkillDiscoveryResult) {
    let annotation = "mesh backend degraded; fell back to legacy discovery".to_string();
    for recommendation in &mut result.recommendations {
        if recommendation.reason.trim().is_empty() {
            recommendation.reason = annotation.clone();
        } else {
            recommendation.reason = format!("{annotation}; {}", recommendation.reason);
        }
    }
}

fn semantic_shortlist_to_result(
    catalog: &[SemanticSkillCatalogEntry],
    selected: &[String],
    cfg: &SkillRecommendationConfig,
    limit: usize,
    research_agent_id: &str,
) -> super::skill_recommendation::SkillDiscoveryResult {
    let agent_name = canonical_agent_name(research_agent_id);
    let shortlist_score = (cfg.strong_match_threshold - 0.08)
        .max(cfg.weak_match_threshold + 0.08)
        .min((cfg.strong_match_threshold - 0.01).max(cfg.weak_match_threshold));
    let secondary_score = (shortlist_score - 0.04).max(cfg.weak_match_threshold);

    let mut recommendations = Vec::new();
    for selected_skill in selected.iter().take(limit.max(1)) {
        let needle = selected_skill.trim();
        let Some(entry) = catalog.iter().find(|entry| {
            entry.record.skill_name.eq_ignore_ascii_case(needle)
                || entry.record.relative_path.eq_ignore_ascii_case(needle)
                || entry
                    .record
                    .relative_path
                    .rsplit('/')
                    .next()
                    .is_some_and(|stem| stem.eq_ignore_ascii_case(needle))
        }) else {
            continue;
        };

        let score = if recommendations.is_empty() {
            shortlist_score
        } else {
            secondary_score
        };
        recommendations.push(super::skill_recommendation::SkillRecommendation {
            record: entry.record.clone(),
            metadata: entry.metadata.clone(),
            excerpt: entry.excerpt.clone(),
            score,
            reason: format!(
                "semantic shortlist via {agent_name}; selected from local skill catalog"
            ),
        });
    }

    let confidence = if recommendations.is_empty() {
        super::skill_recommendation::SkillRecommendationConfidence::None
    } else {
        super::skill_recommendation::SkillRecommendationConfidence::Weak
    };
    let recommended_action = if recommendations.is_empty() {
        super::skill_recommendation::SkillRecommendationAction::None
    } else {
        super::skill_recommendation::SkillRecommendationAction::ReadSkill
    };

    super::skill_recommendation::SkillDiscoveryResult {
        recommendations,
        confidence,
        recommended_action,
    }
}

fn should_run_skill_preflight(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains(SKILL_DISCOVERY_NORMALIZER_MARKER) {
        return false;
    }
    if trimmed.len() >= 48 || trimmed.lines().count() > 1 {
        return true;
    }

    let normalized = trimmed.to_ascii_lowercase();
    [
        "fix",
        "debug",
        "build",
        "implement",
        "refactor",
        "investigate",
        "review",
        "goal",
        "thread",
        "workspace",
        "terminal",
        "session",
        "tool",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

pub(super) fn should_run_skill_preflight_for_turn(
    record_operator: bool,
    agent_task_turn: bool,
    content: &str,
) -> bool {
    (record_operator || agent_task_turn) && should_run_skill_preflight(content)
}

fn build_latest_skill_discovery_state(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
) -> LatestSkillDiscoveryState {
    let decision = policy_decision_for_legacy_discovery(result);
    let query = compact_skill_discovery_query_for_persistence(query);

    LatestSkillDiscoveryState {
        query: query.clone(),
        confidence_tier: decision.confidence_band.as_str().to_string(),
        recommended_skill: decision.recommended_skill,
        recommended_action: decision.recommended_action,
        mesh_next_step: Some(decision.next_step),
        mesh_requires_approval: decision.requires_approval,
        mesh_approval_id: decision
            .requires_approval
            .then(|| format!("skill-mesh-approval:{query}")),
        read_skill_identifier: decision.read_skill_identifier,
        skip_rationale: None,
        discovery_pending: false,
        skill_read_completed: false,
        compliant: false,
        updated_at: now_millis(),
    }
}

fn policy_decision_for_legacy_discovery(
    result: &super::skill_recommendation::SkillDiscoveryResult,
) -> super::skill_mesh::types::SkillMeshPolicyDecision {
    use super::skill_mesh::types::{
        SkillMeshConfidenceBand, SkillMeshNextStep, SkillMeshPolicyDecision,
    };

    let top_recommendation = result.recommendations.first();
    let recommended_skill = top_recommendation.map(|item| item.record.skill_name.clone());
    let read_skill_identifier = top_recommendation.map(|item| item.record.variant_id.clone());

    let confidence_band = match result.confidence {
        super::skill_recommendation::SkillRecommendationConfidence::Strong => {
            SkillMeshConfidenceBand::Strong
        }
        super::skill_recommendation::SkillRecommendationConfidence::Weak => {
            SkillMeshConfidenceBand::Weak
        }
        super::skill_recommendation::SkillRecommendationConfidence::None => {
            SkillMeshConfidenceBand::None
        }
    };

    match confidence_band {
        SkillMeshConfidenceBand::Strong => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::ReadSkill,
            recommended_action: recommended_skill
                .as_deref()
                .map(|skill| format!("read_skill {skill}"))
                .unwrap_or_else(|| SkillMeshNextStep::JustifySkillSkip.as_str().to_string()),
            recommended_skill,
            read_skill_identifier,
            requires_approval: false,
        },
        SkillMeshConfidenceBand::Weak => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::ChooseOrBypass,
            recommended_action: recommended_skill
                .as_deref()
                .map(|skill| format!("read_skill {skill}"))
                .unwrap_or_else(|| SkillMeshNextStep::JustifySkillSkip.as_str().to_string()),
            recommended_skill,
            read_skill_identifier,
            requires_approval: false,
        },
        SkillMeshConfidenceBand::None => SkillMeshPolicyDecision {
            confidence_band,
            next_step: SkillMeshNextStep::JustifySkillSkip,
            recommended_action: SkillMeshNextStep::JustifySkillSkip.as_str().to_string(),
            recommended_skill: None,
            read_skill_identifier: None,
            requires_approval: false,
        },
    }
}

pub(super) fn build_reflection_skill_activation_state(
    query: &str,
    skill_identifier: &str,
) -> Option<LatestSkillDiscoveryState> {
    let query = query.trim();
    let skill_identifier = skill_identifier.trim();
    if query.is_empty() || skill_identifier.is_empty() {
        return None;
    }

    Some(LatestSkillDiscoveryState {
        query: query.to_string(),
        confidence_tier: "strong".to_string(),
        recommended_skill: Some(skill_identifier.to_string()),
        recommended_action: format!("read_skill {skill_identifier}"),
        mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
        mesh_requires_approval: false,
        mesh_approval_id: None,
        read_skill_identifier: Some(skill_identifier.to_string()),
        skip_rationale: Some("goal reflection skill activation".to_string()),
        discovery_pending: false,
        skill_read_completed: false,
        compliant: false,
        updated_at: now_millis(),
    })
}

fn build_pending_skill_discovery_state(query: &str) -> LatestSkillDiscoveryState {
    let query = compact_skill_discovery_query_for_persistence(query);
    LatestSkillDiscoveryState {
        query,
        confidence_tier: "pending".to_string(),
        recommended_skill: None,
        recommended_action: "await_skill_discovery".to_string(),
        mesh_next_step: None,
        mesh_requires_approval: false,
        mesh_approval_id: None,
        read_skill_identifier: None,
        skip_rationale: None,
        discovery_pending: true,
        skill_read_completed: false,
        compliant: false,
        updated_at: now_millis(),
    }
}

fn build_failed_async_skill_discovery_completion(
    thread_id: &str,
    query: &str,
    error: &str,
) -> AsyncSkillDiscoveryCompletion {
    let mut state = build_latest_skill_discovery_state(
        query,
        &super::skill_recommendation::SkillDiscoveryResult::default(),
    );
    state.skip_rationale = Some(format!("background skill discovery failed: {error}"));
    AsyncSkillDiscoveryCompletion {
        thread_id: thread_id.to_string(),
        query: query.to_string(),
        state,
        context_tags: Vec::new(),
        cfg: SkillRecommendationConfig::default(),
        result: super::skill_recommendation::SkillDiscoveryResult::default(),
        error: Some(error.to_string()),
    }
}

fn build_skill_preflight_context_from_state(
    state: LatestSkillDiscoveryState,
) -> SkillPreflightContext {
    SkillPreflightContext {
        prompt_context: build_skill_gate_override_prompt(&state),
        workflow_message: skill_preflight_workflow_message(&state),
        workflow_details: serde_json::to_string(&state).ok(),
        state,
    }
}

fn build_skill_preflight_prompt(
    query: &str,
    result: &super::skill_recommendation::SkillDiscoveryResult,
    state: &LatestSkillDiscoveryState,
) -> String {
    let mut body = format!(
        "Daemon skill discovery already evaluated this request.\n- Query: {query}\n- Confidence: {}\n- Next action: {}\n",
        state.confidence_tier, state.recommended_action
    );

    match state.effective_mesh_next_step() {
        crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill => {
            body.push_str(
                "- Hard gate: do not call non-discovery tools until you read the recommended skill.\n",
            );
        }
        crate::agent::skill_mesh::types::SkillMeshNextStep::ChooseOrBypass => {
            body.push_str(&format!(
                "- Recommendation: prefer `{}` before other substantial tools. If you intentionally bypass the suggested local workflow, record why with `justify_skill_skip`.\n",
                state.recommended_action
            ));
        }
        crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip => {
            body.push_str(
                "- Recommendation: if you proceed without a local skill, record the rationale with `justify_skill_skip`; this is guidance, not a hard prerequisite for other tools.\n",
            );
        }
    }

    if result.recommendations.is_empty() {
        body.push_str(
            "- No installed skill cleared the weak threshold. If you continue, explain why no local skill fits; `justify_skill_skip` can record that rationale.\n",
        );
        return body;
    }

    for recommendation in &result.recommendations {
        let tags = if recommendation.record.context_tags.is_empty() {
            "none".to_string()
        } else {
            recommendation.record.context_tags.join(", ")
        };
        let summary = recommendation
            .metadata
            .summary
            .as_deref()
            .unwrap_or("No summary extracted.");
        let reasons = split_reasons(&recommendation.reason).join(", ");
        body.push_str(&format!(
            "\n- {} [{} | status={} | score={:.2} | tags={}]\n  Reasons: {}\n  Summary: {}\n  Path: {}\n",
            recommendation.record.skill_name,
            recommendation.record.variant_name,
            recommendation.record.status,
            recommendation.score,
            tags,
            reasons,
            summary,
            recommendation.record.relative_path,
        ));
    }

    body
}

pub(crate) fn skill_preflight_workflow_message(state: &LatestSkillDiscoveryState) -> String {
    if state.is_discovery_pending() {
        return format!(
            "Skill discovery is running asynchronously. Next action: {}.",
            state.recommended_action
        );
    }

    format!(
        "Skill discovery confidence: {}. Next action: {}.",
        state.confidence_tier, state.recommended_action
    )
}

pub(crate) fn build_skill_gate_override_prompt(state: &LatestSkillDiscoveryState) -> String {
    if state.is_discovery_pending() {
        return format!(
            "Skill discovery is running asynchronously in a subprocess for this thread.\n- Query: {}\n- Status: pending\n- Next action: {}\n\nContinue reasoning normally, but prefer to defer heavy tool usage until the background skill result arrives.",
            state.query, state.recommended_action,
        );
    }

    if state.mesh_requires_approval {
        let guidance = if state.skill_read_completed {
            format!(
                "The recommended skill has already been read. Do not call non-discovery tools until approval is resolved via {}.",
                state.recommended_action
            )
        } else {
            format!(
                "Do not call non-discovery tools until the required skill workflow is consulted and approval is resolved via {}.",
                state.recommended_action
            )
        };
        return format!(
            "Persisted mesh governance state still applies for this thread.\n- Confidence: {}\n- Next action: {}\n- Approval required: true\n- Skill already read: {}\n\n{}",
            state.confidence_tier, state.recommended_action, state.skill_read_completed, guidance,
        );
    }

    format!(
        "Persisted mesh gate state still applies for this thread.\n- Confidence: {}\n- Next action: {}\n- Approval required: false\n\nFollow this state over fresh legacy preflight defaults.",
        state.confidence_tier, state.recommended_action,
    )
}

fn skill_identifier_matches(expected: &str, actual: &str) -> bool {
    let expected = expected.trim();
    let actual = actual.trim();
    if expected.is_empty() || actual.is_empty() {
        return false;
    }

    expected.eq_ignore_ascii_case(actual)
        || actual
            .rsplit('/')
            .next()
            .is_some_and(|segment| expected.eq_ignore_ascii_case(segment))
        || actual
            .trim_end_matches(".md")
            .trim_end_matches("/SKILL")
            .ends_with(expected)
}

fn action_label(value: super::skill_recommendation::SkillRecommendationAction) -> &'static str {
    match value {
        super::skill_recommendation::SkillRecommendationAction::ReadSkill => {
            zorai_protocol::tool_names::READ_SKILL
        }
        super::skill_recommendation::SkillRecommendationAction::None => "none",
    }
}

fn candidate_confidence_label(score: f64, cfg: &SkillRecommendationConfig) -> &'static str {
    if score >= cfg.strong_match_threshold {
        "strong"
    } else if score >= cfg.weak_match_threshold {
        "weak"
    } else {
        "none"
    }
}

fn split_reasons(reason: &str) -> Vec<String> {
    let parts = reason
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if let Some(rest) = value.strip_prefix("matched request terms ") {
                format!("matched {rest}")
            } else if let Some(rest) = value.strip_prefix("matched workspace tags ") {
                format!("workspace {rest}")
            } else if value.starts_with("historical success ") {
                reason_usage_summary(value)
            } else {
                value.to_string()
            }
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        vec![reason.to_string()]
    } else {
        parts
    }
}

fn reason_usage_summary(value: &str) -> String {
    let words = value.split_whitespace().collect::<Vec<_>>();
    let uses = words
        .iter()
        .position(|word| *word == "across")
        .and_then(|index| words.get(index + 1))
        .and_then(|count| count.parse::<u32>().ok());
    let success_percent = words
        .get(2)
        .map(|value| value.trim_end_matches('%'))
        .and_then(|value| value.parse::<u32>().ok());

    match (uses, success_percent) {
        (Some(uses), Some(success_percent)) => {
            let successes = ((uses as f64) * (success_percent as f64 / 100.0)).round() as u32;
            format!("{successes}/{uses} successful uses")
        }
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::SkillRecommendationConfig;
    use crate::history::HistoryStore;
    use std::collections::VecDeque;
    use std::fs;
    use std::sync::{Arc, Mutex as StdMutex, OnceLock};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn current_dir_test_lock() -> &'static StdMutex<()> {
        crate::test_support::env_test_mutex()
    }

    fn mesh_discovery_test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    fn write_skill(root: &std::path::Path, skill_dir: &str, content: &str) {
        let path = root.join("skills").join(skill_dir).join("SKILL.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create skill parent");
        }
        fs::write(path, content).expect("write skill");
    }

    async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
        let mut buffer = Vec::with_capacity(65536);
        let mut temp = [0u8; 4096];
        let headers_end = loop {
            let read = socket.read(&mut temp).await?;
            if read == 0 {
                return Ok(String::new());
            }
            buffer.extend_from_slice(&temp[..read]);
            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };

        let headers = String::from_utf8_lossy(&buffer[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let mut parts = line.splitn(2, ':');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);

        while buffer.len().saturating_sub(headers_end) < content_length {
            let read = socket.read(&mut temp).await?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&temp[..read]);
        }

        let available = buffer.len().saturating_sub(headers_end).min(content_length);
        Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
    }

    async fn spawn_normalization_server(
        recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
        assistant_content: &str,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind normalization server");
        let addr = listener.local_addr().expect("normalization server addr");
        let response_json =
            serde_json::to_string(assistant_content).expect("assistant response should serialize");

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                let response_json = response_json.clone();
                tokio::spawn(async move {
                    let body = read_http_request_body(&mut socket)
                        .await
                        .expect("read normalization request");
                    recorded_bodies
                        .lock()
                        .expect("lock normalization bodies")
                        .push_back(body);

                    let response = format!(
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}}}}]}}\n\n",
                            "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n",
                            "data: [DONE]\n\n"
                        ),
                        response_json
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write normalization response");
                });
            }
        });

        format!("http://{addr}/v1")
    }

    #[test]
    fn skill_discovery_normalizer_routes_governance_queries_to_weles() {
        assert_eq!(
            normalization_agent_id_for_query(
                "audit control-plane guardrails against governance policy gaps"
            ),
            WELES_AGENT_ID
        );
    }

    #[test]
    fn skill_discovery_normalizer_parses_json_and_fenced_json() {
        assert_eq!(
            parse_normalized_skill_query("{\"query\":\"debug root cause workflow\"}"),
            Some("debug root cause workflow".to_string())
        );
        assert_eq!(
            parse_normalized_skill_query(
                "```json\n{\"query\":\"design planning architecture workflow\"}\n```"
            ),
            Some("design planning architecture workflow".to_string())
        );
    }

    #[test]
    fn local_skill_discovery_query_rewrite_extracts_compact_governance_terms() {
        assert_eq!(
            heuristic_skill_discovery_query_rewrite(
                "Audit modified git worktree files, inspect diffs, and map changed Rust files to orchestration safety governance RFC concepts to identify related vs unrelated changes",
                &["rust".to_string(), "async".to_string()],
            ),
            Some("audit git worktree diff rust orchestration safety governance".to_string())
        );
    }

    #[test]
    fn skill_recommendation_defaults_keep_llm_discovery_fallbacks_disabled() {
        let cfg = SkillRecommendationConfig::default();

        assert!(!cfg.llm_normalize_on_no_match);
        assert!(!cfg.llm_semantic_search_on_no_match);
    }

    #[test]
    fn daemon_internal_messages_do_not_run_skill_preflight() {
        assert!(!should_run_skill_preflight_for_turn(
            false,
            false,
            "HEARTBEAT SYNTHESIS\nYou are performing a scheduled heartbeat check for the operator."
        ));
        assert!(should_run_skill_preflight_for_turn(
            true,
            false,
            "debug daemon startup CPU and memory growth"
        ));
        assert!(should_run_skill_preflight_for_turn(
            false,
            true,
            "debug daemon startup CPU and memory growth"
        ));
    }

    #[test]
    fn persisted_skill_discovery_query_is_compact() {
        let huge_query = format!(
            "HEARTBEAT SYNTHESIS\n{}",
            "large heartbeat context ".repeat(200)
        );

        let state = build_pending_skill_discovery_state(&huge_query);

        assert!(
            state.query.chars().count() <= MAX_NORMALIZED_SKILL_QUERY_CHARS,
            "persisted query should be compact, got {} chars",
            state.query.chars().count()
        );
        assert!(state.query.starts_with("HEARTBEAT SYNTHESIS"));
    }

    #[test]
    fn resolve_daemon_worker_executable_candidate_prefers_env_override() {
        let _lock = current_dir_test_lock().lock().expect("cwd lock");
        let root = tempdir().expect("tempdir");
        let override_path = root.path().join("custom-daemon");
        let current_exe = root.path().join("host-process");

        let resolved = resolve_daemon_worker_executable_candidate(
            Some(current_exe.as_path()),
            Some(override_path.as_os_str()),
        )
        .expect("env override should resolve");

        assert_eq!(resolved, override_path);
    }

    #[test]
    fn resolve_daemon_worker_executable_candidate_prefers_sibling_daemon_binary() {
        let _lock = current_dir_test_lock().lock().expect("cwd lock");
        let root = tempdir().expect("tempdir");
        let current_exe = root.path().join("host-process");
        let sibling = root.path().join(platform_daemon_binary_name());
        fs::write(&current_exe, []).expect("write host executable");
        fs::write(&sibling, []).expect("write daemon sibling");

        let resolved =
            resolve_daemon_worker_executable_candidate(Some(current_exe.as_path()), None)
                .expect("sibling daemon should resolve");

        assert_eq!(resolved, sibling);
    }

    #[test]
    fn resolve_daemon_worker_executable_candidate_accepts_current_daemon_binary() {
        let _lock = current_dir_test_lock().lock().expect("cwd lock");
        let root = tempdir().expect("tempdir");
        let current_exe = root.path().join(platform_daemon_binary_name());
        fs::write(&current_exe, []).expect("write daemon executable");

        let resolved =
            resolve_daemon_worker_executable_candidate(Some(current_exe.as_path()), None)
                .expect("daemon executable should resolve");

        assert_eq!(resolved, current_exe);
    }

    #[cfg(test)]
    struct MeshDegradedGuard<'a> {
        engine: &'a AgentEngine,
        previous: bool,
    }

    #[cfg(test)]
    impl Drop for MeshDegradedGuard<'_> {
        fn drop(&mut self) {
            self.engine
                .force_mesh_discovery_degraded_for_tests
                .store(self.previous, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[cfg(test)]
    fn force_mesh_discovery_degraded_for_tests(
        engine: &AgentEngine,
        value: bool,
    ) -> MeshDegradedGuard<'_> {
        let previous = engine
            .force_mesh_discovery_degraded_for_tests
            .swap(value, std::sync::atomic::Ordering::SeqCst);
        MeshDegradedGuard { engine, previous }
    }

    #[test]
    fn no_match_state_requires_explicit_skill_skip_rationale() {
        let state = build_latest_skill_discovery_state(
            "obscure request with no local skill",
            &super::skill_recommendation::SkillDiscoveryResult::default(),
        );

        assert_eq!(state.confidence_tier, "none");
        assert_eq!(
            state.recommended_action,
            zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP
        );
        assert!(state.read_skill_identifier.is_none());
    }

    #[test]
    fn reflection_skill_activation_state_requires_read_before_progress() {
        let state = build_reflection_skill_activation_state(
            "stabilize the failing rust workflow",
            "systematic-debugging",
        )
        .expect("reflection activation state should be built");

        assert_eq!(state.confidence_tier, "strong");
        assert_eq!(
            state.recommended_skill.as_deref(),
            Some("systematic-debugging")
        );
        assert_eq!(state.recommended_action, "read_skill systematic-debugging");
        assert_eq!(
            state.mesh_next_step,
            Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill)
        );
        assert_eq!(
            state.read_skill_identifier.as_deref(),
            Some("systematic-debugging")
        );
        assert!(!state.compliant);
        assert!(!state.skill_read_completed);
    }

    #[tokio::test]
    async fn strong_match_state_tracks_variant_id_for_read_compliance() {
        let root = tempdir().expect("tempdir");
        let store = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");
        let skills_root = root.path().join("skills");

        write_skill(
            root.path(),
            "development/systematic-debugging",
            r#"---
name: systematic-debugging
description: Debug failures by tracing root cause before patching.
keywords: [debug, rust, panic]
triggers: [panic, failing test]
---

# Systematic Debugging
"#,
        );

        super::skill_recommendation::sync_skill_catalog(&store, &skills_root)
            .await
            .expect("sync skill catalog");
        let result = super::skill_recommendation::discover_local_skills(
            &store,
            &skills_root,
            "debug panic in rust service",
            &["rust".to_string()],
            3,
            &SkillRecommendationConfig::default(),
        )
        .await
        .expect("discover local skills");
        let expected_variant_id = result
            .recommendations
            .first()
            .expect("top recommendation")
            .record
            .variant_id
            .clone();

        let state = build_latest_skill_discovery_state("debug panic in rust service", &result);

        assert_eq!(
            state.recommended_skill.as_deref(),
            Some("systematic-debugging")
        );
        assert_eq!(
            state.read_skill_identifier.as_deref(),
            Some(expected_variant_id.as_str())
        );
    }

    #[tokio::test]
    async fn run_skill_discovery_uses_mesh_backend_when_configured() {
        let _lock = mesh_discovery_test_lock().lock().await;
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        write_skill(
            root.path(),
            "development/systematic-debugging",
            "# Systematic Debugging\nUse this workflow to debug panic failures in rust services.\n",
        );

        let mut config = AgentConfig::default();
        config.skill_recommendation.discovery_backend = "mesh".to_string();

        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let computation = engine
            .run_skill_discovery("debug panic in rust service", None, 3)
            .await
            .expect("mesh discovery should succeed");

        assert_eq!(computation.backend_used, "mesh");
        assert!(!computation.mesh_degraded);
    }

    #[tokio::test]
    async fn run_skill_discovery_uses_mesh_backend_by_default_after_cutover() {
        let _lock = mesh_discovery_test_lock().lock().await;
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        write_skill(
            root.path(),
            "development/systematic-debugging",
            "# Systematic Debugging\nUse this workflow to debug panic failures in rust services.\n",
        );

        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let computation = engine
            .run_skill_discovery("debug panic in rust service", None, 3)
            .await
            .expect("default discovery should succeed");

        assert_eq!(computation.backend_used, "mesh");
        assert!(!computation.mesh_degraded);
    }

    #[tokio::test]
    async fn run_skill_discovery_falls_back_when_mesh_is_degraded() {
        let _lock = mesh_discovery_test_lock().lock().await;
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        write_skill(
            root.path(),
            "development/systematic-debugging",
            "# Systematic Debugging\nUse this workflow to debug panic failures in rust services.\n",
        );

        let mut config = AgentConfig::default();
        config.skill_recommendation.discovery_backend = "mesh".to_string();

        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let _guard = force_mesh_discovery_degraded_for_tests(engine.as_ref(), true);
        let computation = engine
            .run_skill_discovery("debug panic in rust service", None, 3)
            .await
            .expect("mesh fallback should succeed");

        assert_eq!(computation.backend_used, "legacy");
        assert!(computation.mesh_degraded);
    }

    #[tokio::test]
    async fn read_compliance_falls_back_to_recommended_skill_when_variant_id_drifts() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-variant-drift-read-compliance";

        engine
            .set_thread_skill_discovery_state(
                thread_id,
                LatestSkillDiscoveryState {
                    query: "debug panic".to_string(),
                    confidence_tier: "strong".to_string(),
                    recommended_skill: Some("systematic-debugging".to_string()),
                    recommended_action: "read_skill systematic-debugging".to_string(),
                    mesh_next_step: Some(
                        crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill,
                    ),
                    mesh_requires_approval: false,
                    mesh_approval_id: None,
                    read_skill_identifier: Some("variant-systematic-debugging-v1".to_string()),
                    skip_rationale: None,
                    discovery_pending: false,
                    skill_read_completed: false,
                    compliant: false,
                    updated_at: 1,
                },
            )
            .await;

        let state = engine
            .record_thread_skill_read_compliance(thread_id, "systematic-debugging")
            .await
            .expect("state should exist");

        assert!(state.compliant);
        assert!(state.skill_read_completed);
    }

    #[tokio::test]
    async fn approval_resolution_clears_mesh_requires_approval_after_skill_read() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-approval-resolution";

        engine
            .set_thread_skill_discovery_state(
                thread_id,
                LatestSkillDiscoveryState {
                    query: "debug panic".to_string(),
                    confidence_tier: "strong".to_string(),
                    recommended_skill: Some("systematic-debugging".to_string()),
                    recommended_action: "request_approval systematic-debugging".to_string(),
                    mesh_next_step: Some(
                        crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill,
                    ),
                    mesh_requires_approval: true,
                    mesh_approval_id: Some("approval-1".to_string()),
                    read_skill_identifier: Some("systematic-debugging".to_string()),
                    skip_rationale: None,
                    discovery_pending: false,
                    skill_read_completed: true,
                    compliant: false,
                    updated_at: 1,
                },
            )
            .await;

        let state = engine
            .record_thread_skill_approval_resolution(thread_id, "approval-1")
            .await
            .expect("state should exist");

        assert!(state.compliant);
        assert!(!state.mesh_requires_approval);
    }

    #[tokio::test]
    async fn approval_denial_converts_mesh_gate_to_justify_skip_state() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-approval-denial";

        engine
            .set_thread_skill_discovery_state(
                thread_id,
                LatestSkillDiscoveryState {
                    query: "debug panic".to_string(),
                    confidence_tier: "strong".to_string(),
                    recommended_skill: Some("systematic-debugging".to_string()),
                    recommended_action: "request_approval systematic-debugging".to_string(),
                    mesh_next_step: Some(
                        crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill,
                    ),
                    mesh_requires_approval: true,
                    mesh_approval_id: Some("approval-denied".to_string()),
                    read_skill_identifier: Some("systematic-debugging".to_string()),
                    skip_rationale: None,
                    discovery_pending: false,
                    skill_read_completed: true,
                    compliant: false,
                    updated_at: 1,
                },
            )
            .await;

        let state = engine
            .record_thread_skill_approval_denial(thread_id, "approval-denied")
            .await
            .expect("state should exist");

        assert!(!state.mesh_requires_approval);
        assert_eq!(
            state.recommended_action,
            zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP
        );
        assert_eq!(
            state.mesh_next_step,
            Some(crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip)
        );
        assert!(!state.compliant);
    }

    #[tokio::test]
    async fn resolve_skill_context_tags_falls_back_to_current_dir_when_workspace_root_is_absent() {
        let _cwd_lock = current_dir_test_lock().lock().expect("cwd lock");
        let original_cwd = std::env::current_dir().expect("current dir");
        let cargo_root = tempdir().expect("tempdir cargo root");
        fs::write(
            cargo_root.path().join("Cargo.toml"),
            "[package]\nname = \"cwd-fallback\"\nversion = \"0.1.0\"\n[dependencies]\ntokio = \"1\"\n",
        )
        .expect("write cargo manifest");
        std::env::set_current_dir(cargo_root.path()).expect("set cargo cwd");

        let manager = SessionManager::new_test(cargo_root.path()).await;
        let expected = super::semantic_env::infer_workspace_context_tags(cargo_root.path());
        let tags = resolve_skill_context_tags(None, &manager, None).await;

        std::env::set_current_dir(&original_cwd).expect("restore cwd");

        assert_eq!(tags, expected);
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_uses_rarog_fallback_for_natural_language_queries(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url = spawn_normalization_server(
            recorded_bodies.clone(),
            "{\"query\":\"debug root cause workflow\"}",
        )
        .await;

        write_skill(
            root.path(),
            "development/superpowers/systematic-debugging",
            r#"---
name: systematic-debugging
description: Debug failures by tracing root cause before patching.
keywords: [debug, root, cause, workflow]
triggers: [bug fix, failure investigation]
---

# Systematic Debugging
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        config.skill_recommendation.llm_normalize_on_no_match = true;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .discover_skill_recommendations_public(
                "untangle what keeps exploding and prove where it starts before changing code",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].skill_name, "systematic-debugging");
        assert_ne!(result.confidence_tier, "none");
        assert_eq!(result.normalized_intent, "debug root cause workflow");
        assert!(result.candidates[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("normalized")));

        let request_body = recorded_bodies
            .lock()
            .expect("lock recorded bodies")
            .pop_front()
            .expect("fallback normalization should call the model");
        assert!(request_body.contains("zorai concierge"));
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_does_not_call_llm_by_default() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url =
            spawn_normalization_server(recorded_bodies.clone(), "{\"query\":\"debug workflow\"}")
                .await;

        write_skill(
            root.path(),
            "development/debug-rust-build",
            r#"---
name: debug-rust-build
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
triggers: [build failure, cargo test]
---

# Debug Rust Build
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let _result = engine
            .discover_skill_recommendations_public(
                "arrange a calendar handoff with meeting notes",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert!(
            recorded_bodies
                .lock()
                .expect("lock recorded bodies")
                .is_empty(),
            "default skill discovery must not call the configured model"
        );
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_can_use_llm_semantic_search_after_no_match() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url = spawn_normalization_server(
            recorded_bodies.clone(),
            "{\"skills\":[\"using-git-worktrees\"]}",
        )
        .await;

        write_skill(
            root.path(),
            "development/superpowers/using-git-worktrees",
            r#"---
name: using-git-worktrees
description: Create an isolated git worktree before risky feature work.
keywords: [git, worktree, branch, isolation]
triggers: [isolated workspace, dirty checkout, parallel feature work]
---

# Using Git Worktrees
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        config.skill_recommendation.llm_normalize_on_no_match = false;
        config.skill_recommendation.llm_semantic_search_on_no_match = true;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .discover_skill_recommendations_public(
                "prepare a separate sandbox so current edits remain untouched",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].skill_name, "using-git-worktrees");
        assert_eq!(result.confidence_tier, "weak");
        assert!(result.candidates[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("semantic shortlist")));

        let request_body = recorded_bodies
            .lock()
            .expect("lock recorded bodies")
            .pop_front()
            .expect("semantic search fallback should call the model");
        assert!(request_body.contains("using-git-worktrees"));
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_matches_compact_rust_compile_patch_queries() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;

        write_skill(
            root.path(),
            "development/debug-rust-build",
            r#"---
name: debug-rust-build
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
triggers: [build failure, cargo test]
---

# Debug Rust Build

Use this workflow when Rust compilation or cargo builds fail and need investigation before patching.
"#,
        );

        let mut config = AgentConfig::default();
        config.skill_recommendation.llm_normalize_on_no_match = false;
        config.skill_recommendation.llm_semantic_search_on_no_match = false;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        for query in [
            "rust compile patch",
            "cargo compile fix",
            "compile error rust patch",
        ] {
            let result = engine
                .discover_skill_recommendations_public(query, None, 3, None)
                .await
                .expect("skill discovery should succeed");

            assert_ne!(
                result.confidence_tier, "none",
                "query `{query}` should produce a skill recommendation"
            );
            assert_eq!(
                result
                    .candidates
                    .first()
                    .map(|candidate| candidate.skill_name.as_str()),
                Some("debug-rust-build"),
                "query `{query}` should rank the rust build skill first"
            );
            assert!(
                result.recommended_action.starts_with("read_skill "),
                "query `{query}` should recommend reading the matched skill, got `{}`",
                result.recommended_action
            );
        }
    }

    #[tokio::test]
    async fn discover_skill_recommendations_public_uses_weles_fallback_for_governance_queries() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let base_url = spawn_normalization_server(
            recorded_bodies.clone(),
            "{\"query\":\"design planning architecture workflow\"}",
        )
        .await;

        write_skill(
            root.path(),
            "development/superpowers/brainstorming",
            r#"---
name: brainstorming
description: Guide design and planning before implementation.
keywords: [design, planning, architecture, workflow]
triggers: [feature work, implementation planning]
---

# Brainstorming
"#,
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = base_url;
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        config.skill_recommendation.llm_normalize_on_no_match = true;

        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .discover_skill_recommendations_public(
                "check whether control-plane guardrails line up with policy rules and close the gaps",
                None,
                3,
                None,
            )
            .await
            .expect("skill discovery should succeed");

        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].skill_name, "brainstorming");
        assert_ne!(result.confidence_tier, "none");

        let request_body = recorded_bodies
            .lock()
            .expect("lock recorded bodies")
            .pop_front()
            .expect("fallback normalization should call the model");
        assert!(request_body
            .contains("You are Weles (weles) operating as the daemon-owned WELES subagent."));
    }
}
