//! Handoff broker orchestration — routes tasks to specialist subagents,
//! assembles context bundles, and validates specialist output.
//!
//! This module wires together all Plan 01 types and Plan 02 logic into
//! the live agent pipeline, making handoffs work end-to-end.

use anyhow::{Context, Result};
use uuid::Uuid;

use super::profiles::{
    catalog_embedding_signature, cosine_similarity, match_specialist, profile_embedding_text,
    select_by_embedding, select_specialist, SPECIALIST_EMBEDDING_FLOOR,
    SPECIALIST_EMBEDDING_MARGIN,
};
#[cfg(test)]
use super::ValidationResult;
use super::{AcceptanceCriteria, ContextBundle, HandoffResult, RoutingMethod, SpecialistProfile};
use crate::agent::background_workers::domain_routing::select_snapshot_candidate;
use crate::agent::background_workers::protocol::{
    BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult,
};
use crate::agent::background_workers::run_background_worker_command;
use crate::agent::engine::AgentEngine;
use crate::agent::types::RoutingMode;
use crate::agent::TaskStatus;

/// Maximum handoff depth before escalating to operator (HAND-08).
const MAX_HANDOFF_DEPTH: u8 = 3;

/// Token ceiling for context bundles (HAND-03 locked decision).
const CONTEXT_BUNDLE_TOKEN_CEILING: u32 = 2000;

/// Maximum episodic refs to include in a context bundle.
const MAX_EPISODIC_REFS: usize = 3;

fn validate_match_threshold(threshold: f64) -> Result<()> {
    if !threshold.is_finite() {
        anyhow::bail!("handoff match_threshold must be finite");
    }
    if !(0.0..=1.0).contains(&threshold) {
        anyhow::bail!("handoff match_threshold must be within 0.0..=1.0, got {threshold}");
    }
    Ok(())
}

impl AgentEngine {
    async fn task_by_id_for_handoff_context(
        &self,
        task_id: &str,
    ) -> Option<crate::agent::types::AgentTask> {
        match self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: Some(task_id.to_string()),
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: false,
                order_by_recent_activity_desc: false,
                limit: Some(1),
                ids: Vec::new(),
                parent_task_ids: Vec::new(),
            })
            .await
            .into_iter()
            .next()
        {
            Some(task) => Some(task),
            None => {
                let tasks = self.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).cloned()
            }
        }
    }

    /// Assemble a context bundle for a specialist handoff.
    ///
    /// Pulls episodic refs, negative constraints, parent context summary,
    /// and partial outputs into a token-limited bundle.
    pub(crate) async fn assemble_context_bundle(
        &self,
        task_description: &str,
        parent_task_id: Option<&str>,
        _goal_run_id: Option<&str>,
        thread_id: &str,
        acceptance_criteria_str: &str,
        current_depth: u8,
    ) -> Result<ContextBundle> {
        let mut bundle = ContextBundle::new(
            task_description.to_string(),
            acceptance_criteria_str.to_string(),
        );
        bundle.handoff_depth = current_depth;

        if let Some(snapshot) = self
            .get_resonance_context_snapshot(task_description, Some(thread_id), parent_task_id)
            .await
        {
            bundle.episodic_refs = snapshot.episodic_refs;
            bundle.negative_constraints = snapshot.negative_constraints;
            if !snapshot.collaboration_context.is_empty() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&snapshot.collaboration_context)
                        .unwrap_or_else(|_| "[]".to_string()),
                    status: "resonance_shared_context".to_string(),
                });
            }
            if let Some(structural_memory) = snapshot.structural_memory {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&structural_memory)
                        .unwrap_or_else(|_| "{}".to_string()),
                    status: "resonance_structural_memory".to_string(),
                });
            }
        } else {
            let snapshot = self
                .build_resonance_context_snapshot(
                    task_description,
                    Some(thread_id),
                    parent_task_id,
                    MAX_EPISODIC_REFS,
                )
                .await;
            bundle.episodic_refs = snapshot.episodic_refs.clone();
            bundle.negative_constraints = snapshot.negative_constraints.clone();
            if !snapshot.collaboration_context.is_empty() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&snapshot.collaboration_context)
                        .unwrap_or_else(|_| "[]".to_string()),
                    status: "resonance_shared_context".to_string(),
                });
            }
            if let Some(structural_memory) = snapshot.structural_memory.clone() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&structural_memory)
                        .unwrap_or_else(|_| "{}".to_string()),
                    status: "resonance_structural_memory".to_string(),
                });
            }
            self.put_resonance_context_snapshot(
                task_description,
                Some(thread_id),
                parent_task_id,
                snapshot,
            )
            .await;
        }

        if let Some(ptid) = parent_task_id {
            if let Some(parent) = self.task_by_id_for_handoff_context(ptid).await {
                if let Some(ref result) = parent.result {
                    bundle.partial_outputs.push(super::PartialOutput {
                        step_index: 0,
                        content: result.clone(),
                        status: format!("{:?}", parent.status),
                    });
                }
            }
        }

        match self.history.list_messages(thread_id, Some(3)).await {
            Ok(recent_messages) => {
                let joined = recent_messages
                    .iter()
                    .map(|message| message.content.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ");
                bundle.parent_context_summary =
                    crate::agent::goal_parsing::summarize_text(&joined, 500);
            }
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    "failed to load persisted parent context summary for handoff: {error}"
                );
            }
        }

        bundle.recompute_estimated_tokens();
        bundle.enforce_token_ceiling(CONTEXT_BUNDLE_TOKEN_CEILING);

        Ok(bundle)
    }

    /// Route a task to a specialist subagent through the handoff broker.
    ///
    /// Flow: check depth -> match specialist -> assemble bundle -> audit -> enqueue task
    #[allow(clippy::too_many_arguments)]
    /// Embedding-based specialist routing — the primary path when an embedding
    /// model is configured. Embeds the task query and scores it against the
    /// cached specialist-catalog embeddings, returning the best
    /// `candidate_profiles` index when the match clears the confidence bar.
    /// Returns `None` whenever embeddings are unavailable or no specialist is a
    /// confident match, so the caller falls back to capability-tag routing.
    async fn embedding_route_specialist(
        &self,
        catalog: &[SpecialistProfile],
        candidate_profiles: &[SpecialistProfile],
        query: &str,
        config: &crate::agent::types::AgentConfig,
    ) -> Option<(usize, f64)> {
        if !config.semantic.embedding.enabled
            || query.trim().is_empty()
            || candidate_profiles.is_empty()
        {
            return None;
        }
        let model = config.semantic.embedding.model.trim().to_string();
        if model.is_empty() {
            return None;
        }

        let signature = catalog_embedding_signature(catalog, &model);
        let cached = {
            let cache = self.specialist_embedding_cache.lock().await;
            if cache.model == model
                && cache.signature == signature
                && candidate_profiles
                    .iter()
                    .all(|profile| cache.vectors.contains_key(&profile.id))
            {
                Some(
                    candidate_profiles
                        .iter()
                        .map(|profile| cache.vectors.get(&profile.id).cloned().unwrap_or_default())
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        };

        let candidate_vectors = match cached {
            Some(vectors) => vectors,
            None => {
                let texts = catalog
                    .iter()
                    .map(profile_embedding_text)
                    .collect::<Vec<_>>();
                let embeddings = self.embed_texts(config, &texts).await?;
                if embeddings.len() != catalog.len() {
                    return None;
                }
                let map = catalog
                    .iter()
                    .zip(embeddings)
                    .map(|(profile, vector)| (profile.id.clone(), vector))
                    .collect::<std::collections::HashMap<_, _>>();
                let selected = candidate_profiles
                    .iter()
                    .map(|profile| map.get(&profile.id).cloned().unwrap_or_default())
                    .collect::<Vec<_>>();
                let mut cache = self.specialist_embedding_cache.lock().await;
                cache.model = model;
                cache.signature = signature;
                cache.vectors = map;
                selected
            }
        };

        let query_embedding = self
            .embed_texts(config, &[query.trim().to_string()])
            .await?
            .into_iter()
            .next()?;

        let scored = candidate_vectors
            .iter()
            .enumerate()
            .map(|(idx, vector)| (idx, cosine_similarity(&query_embedding, vector)))
            .collect::<Vec<_>>();

        select_by_embedding(
            &scored,
            SPECIALIST_EMBEDDING_FLOOR,
            SPECIALIST_EMBEDDING_MARGIN,
        )
    }

    pub(crate) async fn route_handoff(
        &self,
        task_description: &str,
        capability_tags: &[String],
        parent_task_id: Option<&str>,
        goal_run_id: Option<&str>,
        thread_id: &str,
        acceptance_criteria_str: &str,
        current_depth: u8,
    ) -> Result<HandoffResult> {
        if current_depth >= MAX_HANDOFF_DEPTH {
            anyhow::bail!(
                "Handoff depth limit ({MAX_HANDOFF_DEPTH} hops) reached -- escalating to operator"
            );
        }

        let broker = self.handoff_broker.read().await;
        let profiles = broker.profiles.clone();
        let threshold = broker.match_threshold;
        drop(broker);
        validate_match_threshold(threshold)?;
        let full_config = self.get_config().await;
        let routing_cfg = full_config.routing.clone();
        let required_tags = crate::agent::morphogenesis::task_router::classify_domains(
            task_description,
            capability_tags,
        );

        let occupied_statuses = [TaskStatus::InProgress, TaskStatus::AwaitingApproval]
            .into_iter()
            .filter_map(|status| {
                serde_json::to_value(status)
                    .ok()
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
            })
            .collect::<Vec<_>>();
        let occupied_query = crate::history::AgentTaskListQuery {
            id: None,
            status: None,
            statuses: occupied_statuses,
            source: Some("handoff".to_string()),
            thread_id: None,
            thread_ids: Vec::new(),
            goal_run_id: None,
            parent_task_id: None,
            awaiting_approval_id: None,
            supervisor_config_present: false,
            exclude_terminal_statuses: false,
            order_by_recent_activity_desc: false,
            limit: None,
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        };
        let occupied_titles = match self
            .history
            .list_agent_task_titles_filtered(&occupied_query)
            .await
        {
            Ok(titles) => titles
                .into_iter()
                .map(|(_, title)| title)
                .collect::<Vec<_>>(),
            Err(error) => {
                tracing::warn!("failed to query occupied handoff task titles: {error}");
                self.list_tasks_filtered(&occupied_query)
                    .await
                    .into_iter()
                    .map(|task| task.title)
                    .collect::<Vec<_>>()
            }
        };
        let occupied_specialists = occupied_titles
            .into_iter()
            .filter_map(|title| {
                title
                    .strip_prefix('[')
                    .and_then(|rest| rest.split(']').next())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
            .collect::<std::collections::HashSet<_>>();
        let available_profiles = profiles
            .iter()
            .filter(|profile| !occupied_specialists.contains(profile.role.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let routing_profiles = if available_profiles.is_empty() {
            profiles.clone()
        } else {
            available_profiles
        };

        let routing_snapshot =
            if routing_cfg.enabled && matches!(routing_cfg.method, RoutingMode::Probabilistic) {
                let score_rows = self
                    .load_capability_score_rows(&required_tags)
                    .await
                    .context("loading capability score rows for handoff routing")?;
                let morphogenesis = self
                    .load_morphogenesis_affinities(&required_tags)
                    .await
                    .context("loading morphogenesis affinities for handoff routing")?;
                let result = run_background_worker_command(
                    BackgroundWorkerKind::Routing,
                    BackgroundWorkerCommand::TickRouting {
                        profiles: routing_profiles.clone(),
                        required_tags: required_tags.clone(),
                        score_rows,
                        morphogenesis,
                        routing: routing_cfg.clone(),
                        now_ms: crate::history::now_ts() * 1000,
                    },
                )
                .await
                .context("building worker-backed routing snapshot")?;

                match result {
                    BackgroundWorkerResult::RoutingTick { snapshot } => Some(snapshot),
                    BackgroundWorkerResult::Error { message } => {
                        anyhow::bail!("routing worker returned error: {message}");
                    }
                    other => {
                        anyhow::bail!("routing worker returned unexpected response: {other:?}");
                    }
                }
            } else {
                None
            };

        let learned_selection = routing_snapshot.as_ref().and_then(|snapshot| {
            select_snapshot_candidate(snapshot, routing_cfg.confidence_threshold)
        });

        // Learned/probabilistic routing takes precedence; only reach for the
        // embedding model (a live network call) when learned routing has no
        // confident candidate. Both sit above capability-tag fallback.
        let semantic_selection = if learned_selection.is_some() {
            None
        } else {
            self.embedding_route_specialist(
                &profiles,
                &routing_profiles,
                task_description,
                &full_config,
            )
            .await
        };

        let (selection, routing_confidence_threshold) =
            if let Some((profile_idx, routing_score)) = learned_selection {
                (
                    super::profiles::RoutingSelection {
                        profile_idx,
                        routing_method: RoutingMethod::Probabilistic,
                        routing_score,
                        fallback_used: false,
                    },
                    routing_cfg.confidence_threshold,
                )
            } else if let Some((profile_idx, routing_score)) = semantic_selection {
                (
                    super::profiles::RoutingSelection {
                        profile_idx,
                        routing_method: RoutingMethod::Semantic,
                        routing_score,
                        fallback_used: false,
                    },
                    SPECIALIST_EMBEDDING_FLOOR as f64,
                )
            } else if matches!(routing_cfg.method, RoutingMode::Probabilistic) {
                if let Some((idx, score)) =
                    match_specialist(&routing_profiles, &required_tags, threshold)
                {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: idx,
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: score,
                            fallback_used: true,
                        },
                        routing_cfg.confidence_threshold,
                    )
                } else {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: routing_profiles.len().saturating_sub(1),
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: 0.0,
                            fallback_used: true,
                        },
                        routing_cfg.confidence_threshold,
                    )
                }
            } else if matches!(routing_cfg.method, RoutingMode::Deterministic) {
                if let Some((idx, score)) =
                    match_specialist(&routing_profiles, &required_tags, threshold)
                {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: idx,
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: score,
                            fallback_used: false,
                        },
                        threshold,
                    )
                } else {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: routing_profiles.len().saturating_sub(1),
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: 0.0,
                            fallback_used: true,
                        },
                        threshold,
                    )
                }
            } else {
                let fallback = select_specialist(&routing_profiles, &required_tags, threshold)
                    .context("selecting specialist profile for handoff")?;
                (fallback, threshold)
            };
        let profile_idx = selection.profile_idx;
        let routing_method = selection.routing_method;
        let routing_score = selection.routing_score;
        let fallback_used = selection.fallback_used;

        let specialist = &routing_profiles[profile_idx];
        let specialist_id = specialist.id.clone();
        let specialist_name = specialist.name.clone();
        let specialist_role = specialist.role.clone();
        let system_prompt_snippet = specialist.system_prompt_snippet.clone();
        let routing_rationale = if fallback_used {
            format!(
                "routing fallback selected {} via {} routing because no available candidate cleared the threshold (score {:.2})",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        } else if routing_profiles.len() != profiles.len() {
            format!(
                "routing selected {} via {} routing with score {:.2} after availability filtering skipped occupied specialists",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        } else {
            format!(
                "routing selected {} via {} routing with score {:.2}",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        };
        let matched_capability_tags = required_tags
            .iter()
            .filter(|required_tag| {
                specialist
                    .capabilities
                    .iter()
                    .any(|capability| capability.tag == required_tag.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let specialization_diagnostics = serde_json::json!({
            "matched_capability_tags": matched_capability_tags,
            "semantic_routing_influenced": matches!(routing_method, RoutingMethod::Semantic),
            "learned_routing_influenced": matches!(routing_method, RoutingMethod::Probabilistic),
            "morphogenesis_influenced": routing_snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .ranked_candidates
                    .iter()
                    .any(|candidate| candidate.morphogenesis_affinity > 0.0)
            }),
            "fallback_used": fallback_used,
            "availability_filtered": routing_profiles.len() != profiles.len(),
            "occupied_specialists": occupied_specialists,
            "routing_snapshot_top_candidate": routing_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.ranked_candidates.first())
                .map(|candidate| serde_json::json!({
                    "profile_id": candidate.profile_id,
                    "learned_weight": candidate.learned_weight,
                    "morphogenesis_affinity": candidate.morphogenesis_affinity,
                    "final_weight": candidate.final_weight,
                }))
                .unwrap_or(serde_json::Value::Null),
            "routing_confidence": {
                "score": routing_score,
                "threshold": routing_confidence_threshold,
                "cleared_threshold": !fallback_used && routing_score >= routing_confidence_threshold,
            },
        });

        let bundle = self
            .assemble_context_bundle(
                task_description,
                parent_task_id,
                goal_run_id,
                thread_id,
                acceptance_criteria_str,
                current_depth,
            )
            .await
            .context("assembling context bundle for handoff")?;

        let bundle_tokens = bundle.estimated_tokens;

        let handoff_log_id = Uuid::new_v4().to_string();

        let bundle_json = serde_json::to_string(&bundle).unwrap_or_else(|_| "{}".to_string());
        let criteria_json = serde_json::to_string(&AcceptanceCriteria {
            description: acceptance_criteria_str.to_string(),
            structural_checks: vec!["non_empty".to_string()],
            require_llm_validation: false,
        })
        .unwrap_or_else(|_| "{}".to_string());
        let capability_tags_json =
            serde_json::to_string(capability_tags).unwrap_or_else(|_| "[]".to_string());

        if let Err(e) = self
            .log_handoff_detail(
                &handoff_log_id,
                parent_task_id.unwrap_or("none"),
                &specialist_id,
                None,
                task_description,
                &criteria_json,
                &bundle_json,
                &capability_tags_json,
                current_depth,
                "dispatched",
                None,
                routing_method.as_str(),
                routing_score,
                fallback_used,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to log detail: {e}");
        }

        if let Err(e) = self
            .record_handoff_audit(
                parent_task_id.unwrap_or("none"),
                &specialist_id,
                "pending",
                task_description,
                "dispatched",
                None,
                None,
                &handoff_log_id,
                routing_method.as_str(),
                &capability_tags_json,
                routing_score,
                fallback_used,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to record audit: {e}");
        }

        let full_description = {
            let mut desc = format!(
                "[Handoff to {specialist_name} ({specialist_role})]\n\n\
                 ## Task\n{task_description}\n\n\
                 ## Acceptance Criteria\n{acceptance_criteria_str}\n"
            );
            if let Some(ref snippet) = system_prompt_snippet {
                desc.push_str(&format!("\n## Specialist Instructions\n{snippet}\n"));
            }
            if !bundle.negative_constraints.is_empty() {
                desc.push_str("\n## Known Constraints (DO NOT attempt)\n");
                for constraint in &bundle.negative_constraints {
                    desc.push_str(&format!("- {constraint}\n"));
                }
            }
            if !bundle.episodic_refs.is_empty() {
                desc.push_str("\n## Relevant Past Episodes\n");
                for ep in &bundle.episodic_refs {
                    desc.push_str(&format!(
                        "- [{}] {} ({})\n",
                        ep.episode_id, ep.summary, ep.outcome
                    ));
                }
            }
            desc
        };

        let task = self
            .enqueue_task(
                format!(
                    "[{specialist_role}] {}",
                    crate::agent::goal_parsing::summarize_text(task_description, 72)
                ),
                full_description,
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "handoff",
                goal_run_id.map(str::to_string),
                parent_task_id.map(str::to_string),
                Some(thread_id.to_string()),
                None,
            )
            .await;

        let task_id = task.id.clone();

        if let Err(e) = self.bind_handoff_task_id(&handoff_log_id, &task_id).await {
            tracing::warn!("handoff broker: failed to bind to_task_id for handoff log: {e}");
        }
        if let Err(e) = self
            .update_handoff_outcome(&handoff_log_id, "dispatched", None, None)
            .await
        {
            tracing::warn!("handoff broker: failed to update handoff outcome: {e}");
        }

        Ok(HandoffResult {
            task_id,
            specialist_profile_id: specialist_id,
            specialist_name,
            handoff_log_id,
            context_bundle_tokens: bundle_tokens,
            routing_method,
            routing_score,
            fallback_used,
            routing_rationale,
            specialization_diagnostics,
        })
    }

    /// Validate specialist output against acceptance criteria (HAND-05).
    ///
    /// Looks up the completed task, runs structural validation, records
    /// the validation outcome to the WORM audit trail.
    #[cfg(test)]
    pub(crate) async fn validate_specialist_output(
        &self,
        handoff_log_id: &str,
        task_id: &str,
        acceptance_criteria: &AcceptanceCriteria,
    ) -> Result<ValidationResult> {
        let task_result = self
            .task_by_id_for_handoff_context(task_id)
            .await
            .and_then(|task| task.result);

        let output = match task_result {
            Some(result) => result,
            None => {
                let result = ValidationResult {
                    passed: false,
                    failures: vec!["specialist task has no result output".to_string()],
                    needs_llm_validation: false,
                };

                if let Err(e) = self
                    .update_handoff_outcome(
                        handoff_log_id,
                        "rejected",
                        None,
                        Some("no result output"),
                    )
                    .await
                {
                    tracing::warn!("handoff broker: failed to update outcome: {e}");
                }

                if let Err(e) = self
                    .record_handoff_audit(
                        "validation",
                        "validator",
                        task_id,
                        "output validation",
                        "rejected",
                        None,
                        None,
                        handoff_log_id,
                        "deterministic",
                        "[]",
                        0.0,
                        false,
                    )
                    .await
                {
                    tracing::warn!("handoff broker: failed to record validation audit: {e}");
                }

                return Ok(result);
            }
        };

        let result = acceptance_criteria.validate_structural(&output);

        let outcome = if result.passed {
            "accepted"
        } else {
            "rejected"
        };
        let error_msg = if result.passed {
            None
        } else {
            Some(result.failures.join("; "))
        };

        if let Err(e) = self
            .update_handoff_outcome(handoff_log_id, outcome, None, error_msg.as_deref())
            .await
        {
            tracing::warn!("handoff broker: failed to update validation outcome: {e}");
        }

        if let Err(e) = self
            .record_handoff_audit(
                "validation",
                "validator",
                task_id,
                "output validation",
                outcome,
                None,
                None,
                handoff_log_id,
                "deterministic",
                "[]",
                0.0,
                false,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to record validation audit: {e}");
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{AgentConfig, AgentMessage, AgentThread, TaskPriority, TaskStatus};
    use crate::history::now_ts;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn probabilistic_routing_falls_back_to_deterministic_when_learned_score_is_below_threshold(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Probabilistic;
        config.routing.confidence_threshold = 0.3;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let now_ms = now_ts() * 1000;
        engine
            .record_capability_outcome(
                "researcher",
                &["research".to_string()],
                "failure",
                0.1,
                0,
                0.3,
            )
            .await
            .expect("record researcher outcome");
        engine
            .record_capability_outcome(
                "generalist",
                &["research".to_string()],
                "failure",
                0.1,
                0,
                0.3,
            )
            .await
            .expect("record generalist outcome");

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-fallback",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_eq!(result.routing_method, RoutingMethod::Deterministic);
        assert!(result.fallback_used);
        assert!(result.routing_score >= 0.3);
        assert!(result.routing_rationale.contains("fallback selected"));
        assert!(result
            .specialization_diagnostics
            .get("routing_confidence")
            .and_then(|v| v.get("threshold"))
            .and_then(|v| v.as_f64())
            .is_some_and(|threshold| (threshold - 0.3).abs() < f64::EPSILON));
        assert!(result
            .specialization_diagnostics
            .get("routing_confidence")
            .and_then(|v| v.get("cleared_threshold"))
            .and_then(|v| v.as_bool())
            .is_some_and(|cleared| !cleared));
        assert!(
            result.specialist_profile_id == "researcher"
                || result.specialist_profile_id == "generalist"
        );
        let rows = engine
            .load_capability_score_rows(&["research".to_string()])
            .await
            .expect("capability rows should load");
        assert!(rows
            .iter()
            .any(|row| row.last_attempt_ms.is_some_and(|ts| ts <= now_ms)));
    }

    #[tokio::test]
    async fn routing_skips_occupied_specialist_profiles() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Deterministic;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let mut occupied = engine
            .enqueue_task(
                "[researcher] existing task".to_string(),
                "occupied specialist".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "handoff",
                None,
                None,
                Some("thread-occupied-specialist".to_string()),
                None,
            )
            .await;
        occupied.status = TaskStatus::InProgress;
        occupied.title = "[researcher] already running".to_string();
        occupied.description = "existing occupied researcher task".to_string();
        engine.tasks.lock().await.push_back(occupied);
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-availability",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_ne!(
            result.specialist_profile_id, "researcher",
            "occupied highest-affinity specialist should be skipped"
        );
        assert!(
            result.routing_rationale.contains("availability")
                || result.specialist_profile_id == "generalist",
            "routing result should reflect availability-aware fallback: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn deterministic_routing_maps_caller_tags_to_domain_specialist() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Deterministic;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let result = engine
            .route_handoff(
                "Compare how clinicians differentiate two conditions: summarize the diagnosis, \
                 patient phenotype, and clinical guidelines for each.",
                &["medical_research".to_string()],
                None,
                None,
                "thread-routing-medical",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_eq!(
            result.specialist_profile_id, "medical-research",
            "a caller-tagged medical task must reach the medical specialist, not fall back to generalist"
        );
        assert!(!result.fallback_used);
        assert!(result
            .specialization_diagnostics
            .get("matched_capability_tags")
            .and_then(|v| v.as_array())
            .is_some_and(|tags| !tags.is_empty()));
    }

    #[tokio::test]
    async fn validate_specialist_output_uses_persisted_task_result_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let mut task = engine
            .enqueue_task(
                "[researcher] completed task".to_string(),
                "completed specialist task".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "handoff",
                None,
                None,
                Some("thread-validate-specialist-output".to_string()),
                None,
            )
            .await;
        task.status = TaskStatus::Completed;
        task.result = Some("specialist output reports success".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            tasks.clear();
            tasks.push_back(task.clone());
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let criteria = AcceptanceCriteria {
            description: "validate persisted result".to_string(),
            structural_checks: vec!["contains:success".to_string()],
            require_llm_validation: false,
        };
        let result = engine
            .validate_specialist_output("handoff-log-persisted-result", &task.id, &criteria)
            .await
            .expect("validation should run");

        assert!(result.passed, "persisted task result should be validated");
    }

    #[tokio::test]
    async fn routing_prefers_specialist_with_stronger_morphogenesis_affinity() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Probabilistic;
        config.routing.confidence_threshold = 0.3;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        for _ in 0..3 {
            engine
                .record_morphogenesis_outcome(
                    "researcher",
                    &["research".to_string()],
                    crate::agent::morphogenesis::types::MorphogenesisOutcome::Success,
                )
                .await
                .expect("record morphogenesis outcome");
        }

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-morphogenesis",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_eq!(result.specialist_profile_id, "researcher");
        assert_eq!(result.routing_method, RoutingMethod::Probabilistic);
        assert!(!result.fallback_used);
        assert!(result
            .specialization_diagnostics
            .get("morphogenesis_influenced")
            .and_then(|value| value.as_bool())
            .is_some_and(|value| value));
        assert!(result
            .specialization_diagnostics
            .get("routing_snapshot_top_candidate")
            .and_then(|value| value.get("morphogenesis_affinity"))
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0));
    }

    #[tokio::test]
    async fn related_task_reuses_resonance_cached_shared_context_within_thread() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.threads.write().await.insert(
            "thread-resonance-related".to_string(),
            AgentThread {
                id: "thread-resonance-related".to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "resonance related thread".to_string(),
                messages: vec![AgentMessage::user("inspect handoff cache", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-resonance-related".to_string()),
                Some("daemon".to_string()),
            )
            .await;

        let mut cached_child = parent.clone();
        cached_child.id = "task-cached-child".to_string();
        cached_child.source = "subagent".to_string();
        cached_child.parent_task_id = Some(parent.id.clone());
        cached_child.priority = TaskPriority::Normal;
        cached_child.status = TaskStatus::Queued;

        let mut listening_child = cached_child.clone();
        listening_child.id = "task-listening-child".to_string();

        engine.tasks.lock().await.push_back(cached_child.clone());
        engine.tasks.lock().await.push_back(listening_child.clone());

        engine.collaboration.write().await.insert(
            parent.id.clone(),
            crate::agent::collaboration::CollaborationSession {
                id: "collab-related-1".to_string(),
                parent_task_id: parent.id.clone(),
                thread_id: Some("thread-resonance-related".to_string()),
                goal_run_id: None,
                mission: "shared mission".to_string(),
                agents: Vec::new(),
                contributions: vec![crate::agent::collaboration::Contribution {
                    id: "contrib-related-1".to_string(),
                    task_id: "peer-task".to_string(),
                    topic: "debugging".to_string(),
                    position: "reuse cached context across siblings".to_string(),
                    confidence: 0.9,
                    evidence: vec!["peer evidence".to_string()],
                    created_at: 1,
                }],
                disagreements: Vec::new(),
                consensus: None,
                bids: Vec::new(),
                role_assignment: None,
                call_metadata: None,
                updated_at: 1,
            },
        );

        let snapshot = engine
            .build_resonance_context_snapshot(
                "inspect   handoff   cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
                3,
            )
            .await;
        engine
            .put_resonance_context_snapshot(
                "inspect   handoff   cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
                snapshot,
            )
            .await;

        let bundle = engine
            .assemble_context_bundle(
                "inspect handoff cache",
                Some(listening_child.id.as_str()),
                None,
                "thread-resonance-related",
                "non_empty",
                1,
            )
            .await
            .expect("bundle should assemble");

        assert!(bundle
            .partial_outputs
            .iter()
            .any(|output| output.status == "resonance_shared_context"));

        let cached = engine
            .get_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
            )
            .await
            .expect("cached resonance snapshot should exist");
        assert!(cached.hit_count >= 1);
    }

    #[tokio::test]
    async fn assemble_context_bundle_reuses_resonance_cached_shared_context() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.threads.write().await.insert(
            "thread-resonance".to_string(),
            AgentThread {
                id: "thread-resonance".to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "resonance thread".to_string(),
                messages: vec![AgentMessage::user("inspect handoff cache", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-resonance".to_string()),
                Some("daemon".to_string()),
            )
            .await;

        let mut child = parent.clone();
        child.id = "task-child".to_string();
        child.source = "subagent".to_string();
        child.parent_task_id = Some(parent.id.clone());
        child.priority = TaskPriority::Normal;
        child.status = TaskStatus::Queued;
        engine.tasks.lock().await.push_back(child.clone());

        engine.collaboration.write().await.insert(
            parent.id.clone(),
            crate::agent::collaboration::CollaborationSession {
                id: "collab-1".to_string(),
                parent_task_id: parent.id.clone(),
                thread_id: Some("thread-resonance".to_string()),
                goal_run_id: None,
                mission: "shared mission".to_string(),
                agents: Vec::new(),
                contributions: vec![crate::agent::collaboration::Contribution {
                    id: "contrib-1".to_string(),
                    task_id: "peer-task".to_string(),
                    topic: "debugging".to_string(),
                    position: "reuse cached context".to_string(),
                    confidence: 0.8,
                    evidence: vec!["peer evidence".to_string()],
                    created_at: 1,
                }],
                disagreements: Vec::new(),
                consensus: None,
                bids: Vec::new(),
                role_assignment: None,
                call_metadata: None,
                updated_at: 1,
            },
        );

        let snapshot = engine
            .build_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
                3,
            )
            .await;
        engine
            .put_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
                snapshot,
            )
            .await;

        let bundle = engine
            .assemble_context_bundle(
                "inspect handoff cache",
                Some(child.id.as_str()),
                None,
                "thread-resonance",
                "non_empty",
                1,
            )
            .await
            .expect("bundle should assemble");

        assert!(bundle
            .partial_outputs
            .iter()
            .any(|output| output.status == "resonance_shared_context"));
        let cached = engine
            .get_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
            )
            .await
            .expect("cached resonance snapshot should exist");
        assert!(cached.hit_count >= 1);
    }

    #[tokio::test]
    async fn assemble_context_bundle_includes_persisted_parent_result_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.threads.write().await.insert(
            "thread-handoff-parent-result".to_string(),
            AgentThread {
                id: "thread-handoff-parent-result".to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "handoff parent result".to_string(),
                messages: vec![AgentMessage::user("reuse parent result", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let mut parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-handoff-parent-result".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        parent.status = TaskStatus::Completed;
        parent.result = Some("parent completed result".to_string());
        {
            let mut tasks = engine.tasks.lock().await;
            if let Some(existing) = tasks.iter_mut().find(|task| task.id == parent.id) {
                *existing = parent.clone();
            }
        }
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let bundle = engine
            .assemble_context_bundle(
                "build on parent result",
                Some(parent.id.as_str()),
                None,
                "thread-handoff-parent-result",
                "non_empty",
                1,
            )
            .await
            .expect("bundle should assemble");

        assert!(bundle.partial_outputs.iter().any(|output| {
            output.content == "parent completed result" && output.status == "Completed"
        }));
    }

    #[tokio::test]
    async fn resonance_snapshot_uses_persisted_task_parent_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-resonance-persisted-parent".to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let child = engine
            .enqueue_task(
                "Child".to_string(),
                "Use collaboration context".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some(parent.id.clone()),
                Some("thread-resonance-persisted-parent".to_string()),
                Some("daemon".to_string()),
            )
            .await;

        engine.collaboration.write().await.insert(
            parent.id.clone(),
            crate::agent::collaboration::CollaborationSession {
                id: "collab-persisted-parent".to_string(),
                parent_task_id: parent.id.clone(),
                thread_id: Some("thread-resonance-persisted-parent".to_string()),
                goal_run_id: None,
                mission: "shared mission".to_string(),
                agents: Vec::new(),
                contributions: vec![crate::agent::collaboration::Contribution {
                    id: "contrib-persisted-parent".to_string(),
                    task_id: "peer-task".to_string(),
                    topic: "debugging".to_string(),
                    position: "reuse persisted parent context".to_string(),
                    confidence: 0.8,
                    evidence: vec!["peer evidence".to_string()],
                    created_at: 1,
                }],
                disagreements: Vec::new(),
                consensus: None,
                bids: Vec::new(),
                role_assignment: None,
                call_metadata: None,
                updated_at: 1,
            },
        );
        engine.tasks.lock().await.clear();

        let snapshot = engine
            .build_resonance_context_snapshot(
                "inspect persisted parent",
                Some("thread-resonance-persisted-parent"),
                Some(child.id.as_str()),
                3,
            )
            .await;

        assert_eq!(snapshot.collaboration_context.len(), 1);
        assert_eq!(
            snapshot.collaboration_context[0]["position"].as_str(),
            Some("reuse persisted parent context")
        );
    }
}
