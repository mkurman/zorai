mod metadata;
mod ranking;
mod types;

use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::{
    derive_skill_metadata, GuidelineDocumentRecord, HistoryStore, SkillVariantRecord,
};
use anyhow::Result;
use base64::Engine;
use ranking::{rank_skill_candidates, rank_skill_candidates_with_semantic_scores};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use types::{GraphSkillSignal, SkillCandidateInput};

pub(crate) use metadata::extract_skill_metadata;
pub(crate) use types::{
    SkillDiscoveryResult, SkillDocumentMetadata, SkillRecommendation, SkillRecommendationAction,
    SkillRecommendationConfidence,
};

const MAX_SKILL_EXCERPT_LINES: usize = 40;
const MAX_SKILL_EXCERPT_CHARS: usize = 2400;
const DISCOVERY_CURSOR_PREFIX: &str = "skill-discovery:";

pub(crate) async fn discover_local_skills(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    let records = history.list_discoverable_skill_variants(512).await?;
    let candidates = if records.is_empty() {
        schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
        collect_filesystem_skill_candidates(skills_root)?
    } else {
        let candidates = collect_registered_skill_candidates(history, skills_root, records).await?;
        if candidates.is_empty() {
            schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
            collect_filesystem_skill_candidates(skills_root)?
        } else {
            candidates
        }
    };

    let graph_signals = load_graph_backed_skill_signals(history, query).await?;

    Ok(rank_skill_candidates(
        candidates,
        query,
        workspace_tags,
        &graph_signals,
        limit,
        cfg,
    ))
}

pub(crate) async fn discover_local_skills_with_semantic_scores(
    history: &HistoryStore,
    skills_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
    semantic_scores: &HashMap<String, f64>,
) -> Result<SkillDiscoveryResult> {
    let records = history.list_discoverable_skill_variants(512).await?;
    let candidates = if records.is_empty() {
        schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
        collect_filesystem_skill_candidates(skills_root)?
    } else {
        let candidates = collect_registered_skill_candidates(history, skills_root, records).await?;
        if candidates.is_empty() {
            schedule_background_skill_catalog_sync(history.clone(), skills_root.to_path_buf());
            collect_filesystem_skill_candidates(skills_root)?
        } else {
            candidates
        }
    };

    let graph_signals = load_graph_backed_skill_signals(history, query).await?;

    Ok(rank_skill_candidates_with_semantic_scores(
        candidates,
        query,
        workspace_tags,
        &graph_signals,
        semantic_scores,
        limit,
        cfg,
    ))
}

pub(crate) async fn discover_local_guidelines(
    history: &HistoryStore,
    guidelines_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
) -> Result<SkillDiscoveryResult> {
    let candidates = load_guideline_candidates(history, guidelines_root).await?;
    let graph_signals = HashMap::new();

    let result = rank_skill_candidates(
        candidates,
        query,
        workspace_tags,
        &graph_signals,
        limit,
        cfg,
    );
    Ok(result)
}

pub(crate) async fn discover_local_guidelines_with_semantic_scores(
    history: &HistoryStore,
    guidelines_root: &Path,
    query: &str,
    workspace_tags: &[String],
    limit: usize,
    cfg: &SkillRecommendationConfig,
    semantic_scores: &HashMap<String, f64>,
) -> Result<SkillDiscoveryResult> {
    let candidates = load_guideline_candidates(history, guidelines_root).await?;
    let graph_signals = HashMap::new();

    Ok(rank_skill_candidates_with_semantic_scores(
        candidates,
        query,
        workspace_tags,
        &graph_signals,
        semantic_scores,
        limit,
        cfg,
    ))
}

const MAX_GRAPH_SIGNAL_HOPS: u8 = 2;
const MAX_GRAPH_SIGNAL_SEEDS: usize = 8;

async fn load_graph_backed_skill_signals(
    history: &HistoryStore,
    query: &str,
) -> Result<HashMap<String, GraphSkillSignal>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(HashMap::new());
    }

    let seed_nodes = load_graph_seed_nodes(history, trimmed).await?;
    let mut signals = HashMap::new();
    let mut queue = seed_nodes
        .iter()
        .cloned()
        .map(|node_id| (node_id, 0u8, f64::INFINITY))
        .collect::<VecDeque<_>>();
    let mut best_depth = seed_nodes
        .into_iter()
        .map(|node_id| (node_id, 0u8))
        .collect::<HashMap<_, _>>();

    while let Some((node_id, depth, path_score)) = queue.pop_front() {
        if depth >= MAX_GRAPH_SIGNAL_HOPS {
            continue;
        }

        let neighbors = history.list_memory_graph_neighbors(&node_id, 64).await?;
        for row in neighbors {
            let next_depth = depth + 1;
            let next_score = if path_score.is_infinite() {
                row.via_edge.weight
            } else {
                path_score.min(row.via_edge.weight)
            };

            if row.node.node_type == "skill_variant"
                && row.via_edge.relation_type == "intent_prefers_skill"
            {
                let Some(variant_id) = row.node.id.strip_prefix("skill:") else {
                    continue;
                };
                let incoming = GraphSkillSignal {
                    score: next_score,
                    distance: next_depth,
                };
                let entry = signals.entry(variant_id.to_string()).or_insert(incoming);
                if incoming.score > entry.score
                    || (incoming.score == entry.score && incoming.distance > entry.distance)
                {
                    *entry = incoming;
                }
                continue;
            }

            if next_depth >= MAX_GRAPH_SIGNAL_HOPS || row.node.node_type != "intent" {
                continue;
            }

            let node_id = row.node.id.clone();
            let should_enqueue = match best_depth.get(&node_id) {
                Some(existing) if *existing <= next_depth => false,
                _ => true,
            };
            if should_enqueue {
                best_depth.insert(node_id.clone(), next_depth);
                queue.push_back((node_id, next_depth, next_score));
            }
        }
    }

    Ok(signals)
}

async fn load_graph_seed_nodes(history: &HistoryStore, query: &str) -> Result<Vec<String>> {
    let intent_node_id = format!("intent:{}", query.to_ascii_lowercase());
    let exact = query.to_ascii_lowercase();
    let like = format!("%{}%", exact);
    let matched = history
        .read_db
        .query(
            "SELECT id FROM memory_nodes
             WHERE node_type != 'skill_variant'
               AND (
                    lower(label) = ?1 OR
                    lower(label) LIKE ?2 OR
                    lower(COALESCE(summary_text, '')) LIKE ?2
               )
             ORDER BY
                CASE WHEN lower(label) = ?1 THEN 0 ELSE 1 END,
                access_count DESC,
                last_accessed_ms DESC,
                id ASC
             LIMIT ?3",
            crate::history::db::db_params![exact, like, MAX_GRAPH_SIGNAL_SEEDS as i64],
        )
        .await?
        .iter()
        .map(|row| row.get::<String>(0))
        .collect::<Result<Vec<_>>>()?;

    let mut seeds = Vec::with_capacity(1 + matched.len());
    seeds.push(intent_node_id);
    for node_id in matched {
        if !seeds.iter().any(|existing| existing == &node_id) {
            seeds.push(node_id);
        }
    }
    Ok(seeds)
}

pub(crate) fn schedule_background_skill_catalog_sync(history: HistoryStore, skills_root: PathBuf) {
    tokio::spawn(async move {
        if let Err(error) = sync_skill_catalog(&history, &skills_root).await {
            tracing::warn!(
                %error,
                skills_root = %skills_root.display(),
                "background skill catalog sync failed during discovery"
            );
        }
    });
}

pub(crate) async fn sync_skill_catalog(history: &HistoryStore, skills_root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    for path in files {
        history.register_skill_document(&path).await?;
    }
    Ok(())
}

async fn load_guideline_candidates(
    history: &HistoryStore,
    guidelines_root: &Path,
) -> Result<Vec<SkillCandidateInput>> {
    let records = history.list_discoverable_guideline_documents(512).await?;
    if records.is_empty() {
        schedule_background_guideline_catalog_sync(history.clone(), guidelines_root.to_path_buf());
        return collect_filesystem_guideline_candidates(guidelines_root);
    }
    Ok(collect_registered_guideline_candidates(records))
}

fn schedule_background_guideline_catalog_sync(history: HistoryStore, guidelines_root: PathBuf) {
    tokio::spawn(async move {
        if let Err(error) = sync_guideline_catalog(&history, &guidelines_root).await {
            tracing::warn!(
                %error,
                guidelines_root = %guidelines_root.display(),
                "background guideline catalog sync failed during discovery"
            );
        }
    });
}

pub(crate) async fn sync_guideline_catalog(
    history: &HistoryStore,
    guidelines_root: &Path,
) -> Result<()> {
    let sync_started_at = crate::history::now_ts() as i64;
    let mut files = Vec::new();
    collect_guideline_documents(guidelines_root, &mut files)?;
    for path in files {
        let relative_path = path
            .strip_prefix(guidelines_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let Some(content) = read_recommendation_document(&path, "guideline")? else {
            continue;
        };
        history
            .register_guideline_document(&relative_path, &content, sync_started_at)
            .await?;
    }
    history
        .prune_stale_guideline_documents(sync_started_at)
        .await?;
    Ok(())
}

fn collect_registered_guideline_candidates(
    records: Vec<GuidelineDocumentRecord>,
) -> Vec<SkillCandidateInput> {
    let mut candidates = Vec::with_capacity(records.len());
    for record in records {
        let derived = derive_skill_metadata(&record.relative_path, &record.excerpt);
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&record.relative_path, &record.excerpt),
            excerpt: excerpt_skill(&record.excerpt),
            record: synthetic_guideline_variant_record(&record.relative_path, &derived),
        });
    }
    candidates
}

pub(crate) fn resolve_skill_document_path(
    skills_root: &Path,
    relative_path: &str,
) -> (PathBuf, String) {
    let normalized = relative_path.replace('\\', "/");
    let candidate = skills_root.join(&normalized);
    if candidate.exists() {
        return (candidate, normalized);
    }

    if let Some(stripped) = normalized.strip_prefix("builtin/") {
        let migrated = skills_root.join(stripped);
        if migrated.exists() {
            return (migrated, stripped.to_string());
        }
    }

    if let Some(resolved) = resolve_skill_document_by_suffix(skills_root, &normalized) {
        return resolved;
    }

    (candidate, normalized)
}

fn resolve_skill_document_by_suffix(
    skills_root: &Path,
    relative_path: &str,
) -> Option<(PathBuf, String)> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files).ok()?;

    for suffix in relative_path_suffixes(relative_path) {
        let matches = files
            .iter()
            .filter_map(|path| {
                let relative = path
                    .strip_prefix(skills_root)
                    .ok()?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == suffix || relative.ends_with(&format!("/{suffix}")) {
                    Some((path.clone(), relative))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return matches.into_iter().next();
        }
    }

    None
}

fn relative_path_suffixes(relative_path: &str) -> Vec<String> {
    let mut suffixes = Vec::new();
    let mut current = relative_path.trim_matches('/').to_string();
    while !current.is_empty() {
        suffixes.push(current.clone());
        let Some((_, tail)) = current.split_once('/') else {
            break;
        };
        if !tail.contains('/') && tail.eq_ignore_ascii_case("SKILL.md") {
            break;
        }
        current = tail.to_string();
    }
    suffixes
}

pub(crate) async fn discover_community_skills(
    data_dir: &Path,
    registry_url: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<zorai_protocol::CommunitySkillEntry>> {
    let client = RegistryClient::new(registry_url.to_string(), data_dir);
    let _ = client.refresh_index().await;
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    let mut queries = vec![query.trim().to_string()];
    queries.extend(
        query
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
            .map(str::trim)
            .filter(|token| token.len() >= 3)
            .map(ToOwned::to_owned),
    );

    for search_query in queries {
        if search_query.is_empty() {
            continue;
        }
        for entry in client.search(&search_query).await? {
            if seen.insert(entry.name.clone()) {
                merged.push(entry);
            }
            if merged.len() >= limit.max(1) {
                return Ok(merged
                    .into_iter()
                    .map(|entry| to_community_entry(&entry))
                    .collect());
            }
        }
    }

    Ok(merged
        .into_iter()
        .map(|entry| to_community_entry(&entry))
        .collect())
}

fn collect_skill_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_documents(&path, out)?;
        } else if should_include_skill_document(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_guideline_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str.starts_with('.') || name_str.contains(':') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_guideline_documents(&path, out)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn should_include_skill_document(path: &Path) -> bool {
    should_include_skill_relative_path(&path.to_string_lossy())
}

fn should_include_skill_relative_path(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    file_name.eq_ignore_ascii_case("skill.md")
        || (path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"))
            && path
                .components()
                .any(|component| component.as_os_str() == "generated"))
}

async fn collect_registered_skill_candidates(
    history: &HistoryStore,
    skills_root: &Path,
    records: Vec<SkillVariantRecord>,
) -> Result<Vec<SkillCandidateInput>> {
    let mut candidates = Vec::new();
    for record in records {
        if matches!(record.status.as_str(), "archived" | "merged" | "draft") {
            continue;
        }
        if !should_include_skill_relative_path(&record.relative_path) {
            continue;
        }

        let (skill_path, metadata_relative_path) =
            resolve_skill_document_path(skills_root, &record.relative_path);
        let Some(content) = read_recommendation_document(&skill_path, "skill")? else {
            if let Err(error) = history.retire_skill_variant(&record.variant_id).await {
                tracing::warn!(
                    variant_id = %record.variant_id,
                    relative_path = %record.relative_path,
                    %error,
                    "failed to retire stale skill variant after missing/unreadable file"
                );
            } else {
                tracing::info!(
                    variant_id = %record.variant_id,
                    relative_path = %record.relative_path,
                    "retired stale skill variant: backing file missing or unreadable"
                );
            }
            continue;
        };
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&metadata_relative_path, &content),
            excerpt: excerpt_skill(&content),
            record,
        });
    }
    Ok(candidates)
}

pub(crate) fn list_filesystem_skill_variants(
    skills_root: &Path,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<SkillVariantRecord>> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    files.sort();
    let needle = query.map(|value| value.to_ascii_lowercase());
    let now = crate::history::now_ts();
    let mut entries = Vec::new();
    for path in files {
        let relative_path = path
            .strip_prefix(skills_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let skill_name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("skill")
            .to_string();
        if let Some(needle) = needle.as_deref() {
            if !skill_name.to_ascii_lowercase().contains(needle)
                && !relative_path.to_ascii_lowercase().contains(needle)
            {
                continue;
            }
        }
        entries.push(SkillVariantRecord {
            variant_id: format!("fs:{relative_path}"),
            skill_name: skill_name.clone(),
            variant_name: "canonical".to_string(),
            relative_path,
            parent_variant_id: None,
            version: "v1.0".to_string(),
            context_tags: Vec::new(),
            use_count: 0,
            success_count: 0,
            failure_count: 0,
            fitness_score: 0.0,
            status: "active".to_string(),
            last_used_at: None,
            created_at: now,
            updated_at: now,
        });
        if entries.len() >= limit {
            break;
        }
    }
    Ok(entries)
}

fn collect_filesystem_skill_candidates(skills_root: &Path) -> Result<Vec<SkillCandidateInput>> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;

    let mut candidates = Vec::new();
    for path in files {
        let relative_path = path
            .strip_prefix(skills_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let Some(content) = read_recommendation_document(&path, "skill")? else {
            continue;
        };
        let derived = derive_skill_metadata(&relative_path, &content);
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&relative_path, &content),
            excerpt: excerpt_skill(&content),
            record: synthetic_skill_variant_record(&relative_path, &derived),
        });
    }

    Ok(candidates)
}

fn collect_filesystem_guideline_candidates(
    guidelines_root: &Path,
) -> Result<Vec<SkillCandidateInput>> {
    let mut files = Vec::new();
    collect_guideline_documents(guidelines_root, &mut files)?;

    let mut candidates = Vec::new();
    for path in files {
        let relative_path = path
            .strip_prefix(guidelines_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let Some(content) = read_recommendation_document(&path, "guideline")? else {
            continue;
        };
        let derived = derive_skill_metadata(&relative_path, &content);
        candidates.push(SkillCandidateInput {
            metadata: extract_skill_metadata(&relative_path, &content),
            excerpt: excerpt_skill(&content),
            record: synthetic_guideline_variant_record(&relative_path, &derived),
        });
    }

    Ok(candidates)
}

const RECOMMENDATION_HEAD_BYTES: u64 = 16 * 1024;

fn read_recommendation_document(path: &Path, document_kind: &str) -> Result<Option<String>> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(
                path = %path.display(),
                document_kind,
                "recommendation document disappeared before discovery could read it"
            );
            return Ok(None);
        }
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                document_kind,
                %error,
                "failed to open recommendation document; skipping so discovery can continue"
            );
            return Ok(None);
        }
    };
    let mut buf = String::with_capacity(RECOMMENDATION_HEAD_BYTES as usize);
    if let Err(error) = file
        .take(RECOMMENDATION_HEAD_BYTES)
        .read_to_string(&mut buf)
    {
        if buf.is_empty() {
            tracing::warn!(
                path = %path.display(),
                document_kind,
                %error,
                "failed to read recommendation document; skipping so discovery can continue"
            );
            return Ok(None);
        }
    }
    Ok(Some(buf))
}

fn synthetic_skill_variant_record(
    relative_path: &str,
    derived: &crate::history::DerivedSkillMetadata,
) -> SkillVariantRecord {
    let normalized = relative_path.replace('\\', "/");
    let now = crate::history::now_ts();

    SkillVariantRecord {
        variant_id: format!("fs:{normalized}"),
        skill_name: if derived.skill_name.is_empty() {
            "skill".to_string()
        } else {
            derived.skill_name.clone()
        },
        variant_name: derived.variant_name.clone(),
        relative_path: normalized,
        parent_variant_id: None,
        version: "v1.0".to_string(),
        context_tags: derived.context_tags.clone(),
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        fitness_score: 0.0,
        status: "active".to_string(),
        last_used_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn synthetic_guideline_variant_record(
    relative_path: &str,
    derived: &crate::history::DerivedSkillMetadata,
) -> SkillVariantRecord {
    let normalized = relative_path.replace('\\', "/");
    let now = crate::history::now_ts();

    SkillVariantRecord {
        variant_id: format!("guideline:fs:{normalized}"),
        skill_name: if derived.skill_name.is_empty() {
            "guideline".to_string()
        } else {
            derived.skill_name.clone()
        },
        variant_name: derived.variant_name.clone(),
        relative_path: normalized,
        parent_variant_id: None,
        version: "v1.0".to_string(),
        context_tags: derived.context_tags.clone(),
        use_count: 0,
        success_count: 0,
        failure_count: 0,
        fitness_score: 0.0,
        status: "active".to_string(),
        last_used_at: None,
        created_at: now,
        updated_at: now,
    }
}

pub(super) fn page_public_discovery_result(
    query: &str,
    normalized_intent: &str,
    context_tags: &[String],
    result: &SkillDiscoveryResult,
    cfg: &SkillRecommendationConfig,
    cursor: Option<&str>,
    limit: usize,
) -> Result<zorai_protocol::SkillDiscoveryResultPublic> {
    page_public_discovery_result_with_action(
        query,
        normalized_intent,
        context_tags,
        result,
        cfg,
        cursor,
        limit,
        zorai_protocol::tool_names::READ_SKILL,
        None,
    )
}

pub(super) fn page_public_discovery_result_with_action(
    query: &str,
    normalized_intent: &str,
    context_tags: &[String],
    result: &SkillDiscoveryResult,
    cfg: &SkillRecommendationConfig,
    cursor: Option<&str>,
    limit: usize,
    action_verb: &str,
    source_kind_override: Option<&str>,
) -> Result<zorai_protocol::SkillDiscoveryResultPublic> {
    let limit = limit.clamp(1, 100);
    let start_index = decode_page_cursor(cursor)?
        .as_deref()
        .and_then(|variant_id| {
            result
                .recommendations
                .iter()
                .position(|item| item.record.variant_id == variant_id)
                .map(|index| index + 1)
        })
        .unwrap_or(0);
    let page = result
        .recommendations
        .iter()
        .skip(start_index)
        .take(limit)
        .collect::<Vec<_>>();
    let next_cursor = if start_index + page.len() < result.recommendations.len() {
        page.last()
            .map(|recommendation| encode_page_cursor(&recommendation.record.variant_id))
    } else {
        None
    };

    let top_skill_name = result
        .recommendations
        .first()
        .map(|recommendation| recommendation.record.skill_name.as_str());

    Ok(zorai_protocol::SkillDiscoveryResultPublic {
        query: query.to_string(),
        normalized_intent: normalized_intent.to_string(),
        required: !matches!(result.recommended_action, SkillRecommendationAction::None),
        confidence_tier: confidence_label(result.confidence).to_string(),
        recommended_action: recommended_action_label_with_verb(
            result.recommended_action,
            top_skill_name,
            action_verb,
        ),
        requires_approval: false,
        mesh_state: "fresh".to_string(),
        rationale: result
            .recommendations
            .first()
            .map(|recommendation| split_reasons(&recommendation.reason))
            .unwrap_or_default(),
        capability_family: context_tags.to_vec(),
        explicit_rationale_required: false,
        workspace_tags: context_tags.to_vec(),
        candidates: page
            .into_iter()
            .map(
                |recommendation| zorai_protocol::SkillDiscoveryCandidatePublic {
                    variant_id: recommendation.record.variant_id.clone(),
                    skill_name: recommendation.record.skill_name.clone(),
                    variant_name: recommendation.record.variant_name.clone(),
                    relative_path: recommendation.record.relative_path.clone(),
                    status: recommendation.record.status.clone(),
                    score: recommendation.score,
                    confidence_tier: candidate_confidence_label(recommendation.score, cfg)
                        .to_string(),
                    reasons: split_reasons(&recommendation.reason),
                    matched_intents: vec![query.to_string()],
                    matched_trigger_phrases: recommendation.metadata.triggers.clone(),
                    context_tags: recommendation.record.context_tags.clone(),
                    risk_level: if recommendation.metadata.built_in {
                        "low".to_string()
                    } else {
                        "medium".to_string()
                    },
                    trust_tier: if recommendation.metadata.built_in {
                        "trusted_builtin".to_string()
                    } else {
                        "trusted_local".to_string()
                    },
                    source_kind: source_kind_override
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| {
                            if recommendation.record.relative_path.contains("generated/") {
                                "generated".to_string()
                            } else {
                                "builtin".to_string()
                            }
                        }),
                    recommended_action: recommended_action_label_with_verb(
                        result.recommended_action,
                        Some(recommendation.record.skill_name.as_str()),
                        action_verb,
                    ),
                    use_count: recommendation.record.use_count,
                    success_count: recommendation.record.success_count,
                    failure_count: recommendation.record.failure_count,
                    canonical_pack: recommendation.metadata.canonical_pack,
                    delivery_modes: recommendation.metadata.delivery_modes.clone(),
                    prerequisite_hints: recommendation.metadata.prerequisite_hints.clone(),
                    prerequisite_connectors: recommendation
                        .metadata
                        .prerequisite_connectors
                        .clone(),
                    source_links: recommendation.metadata.source_links.clone(),
                    mobile_safe: recommendation.metadata.mobile_safe,
                    approval_behavior: recommendation.metadata.approval_behavior.clone(),
                },
            )
            .collect(),
        next_cursor,
    })
}

fn encode_page_cursor(variant_id: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(variant_id.as_bytes());
    format!("{DISCOVERY_CURSOR_PREFIX}{encoded}")
}

fn decode_page_cursor(cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let payload = cursor
        .strip_prefix(DISCOVERY_CURSOR_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid discovery cursor"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| anyhow::anyhow!("invalid discovery cursor: {error}"))?;
    let value = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("invalid discovery cursor: {error}"))?;
    Ok(Some(value))
}

fn confidence_label(value: SkillRecommendationConfidence) -> &'static str {
    match value {
        SkillRecommendationConfidence::Strong => "strong",
        SkillRecommendationConfidence::Weak => "weak",
        SkillRecommendationConfidence::None => "none",
    }
}

fn action_label(value: SkillRecommendationAction) -> &'static str {
    match value {
        SkillRecommendationAction::ReadSkill => zorai_protocol::tool_names::READ_SKILL,
        SkillRecommendationAction::None => "none",
    }
}

fn recommended_action_label_with_verb(
    action: SkillRecommendationAction,
    top_skill_name: Option<&str>,
    action_verb: &str,
) -> String {
    match (action, top_skill_name) {
        (SkillRecommendationAction::ReadSkill, Some(skill_name)) => {
            format!("{action_verb} {skill_name}")
        }
        _ => {
            if matches!(action, SkillRecommendationAction::ReadSkill) {
                action_verb.to_string()
            } else {
                action_label(action).to_string()
            }
        }
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

    match (uses, Some(success_percent).flatten()) {
        (Some(uses), Some(success_percent)) => {
            let successes = ((uses as f64) * (success_percent as f64 / 100.0)).round() as u32;
            format!("{successes}/{uses} successful uses")
        }
        _ => value.to_string(),
    }
}

fn excerpt_skill(content: &str) -> String {
    let mut excerpt = content
        .lines()
        .take(MAX_SKILL_EXCERPT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if excerpt.len() > MAX_SKILL_EXCERPT_CHARS {
        let new_len = excerpt
            .char_indices()
            .take_while(|(idx, ch)| *idx + ch.len_utf8() <= MAX_SKILL_EXCERPT_CHARS)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0);
        excerpt.truncate(new_len);
        excerpt.push_str("\n...");
    } else if content.lines().count() > MAX_SKILL_EXCERPT_LINES {
        excerpt.push_str("\n...");
    }
    excerpt
}

#[cfg(test)]
#[path = "../tests/skill_recommendation.rs"]
mod tests;
