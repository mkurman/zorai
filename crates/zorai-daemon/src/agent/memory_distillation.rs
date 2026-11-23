//! Episodic → Semantic Memory Distillation (Spec 01)
//!
//! Analyzes old, undistilled thread transcripts and extracts actionable
//! memory entries (preferences, conventions, patterns, corrections, lessons).
//! Candidates above the auto-apply confidence threshold are written to
//! MEMORY.md/USER.md with a `[distilled]` provenance prefix.

use super::*;
use crate::history::db;
use crate::history::{
    AgentMessageCursor, AgentMessageSpan, HistoryStore, MemoryDistillationProgressRow,
};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use zorai_protocol::{AgentDbMessage, InboxNotification};

const MIN_FACT_CHARS: usize = 24;
const MAX_FACT_CHARS: usize = 220;
const THREAD_BATCH_LIMIT: usize = 24;
const OLD_THREAD_AGE_MS: u64 = 60 * 60 * 1000;

/// Categories for distilled memory entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Operator habits, risk tolerance, session patterns.
    Preference,
    /// Workspace facts, coding conventions, tool patterns.
    Convention,
    /// Repeated behaviors, workflow habits.
    Pattern,
    /// Things the operator corrected (high value).
    Correction,
    /// Generalizable insights from task outcomes.
    Lesson,
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preference => write!(f, "preference"),
            Self::Convention => write!(f, "convention"),
            Self::Pattern => write!(f, "pattern"),
            Self::Correction => write!(f, "correction"),
            Self::Lesson => write!(f, "lesson"),
        }
    }
}

/// A candidate memory entry distilled from thread content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationCandidate {
    pub source_thread_id: String,
    pub source_message_range: Option<String>,
    pub source_message_span: Option<AgentMessageSpan>,
    pub last_processed_cursor: Option<AgentMessageCursor>,
    pub distilled_fact: String,
    pub target_file: String,
    pub category: MemoryCategory,
    pub confidence: f64,
    pub reasoning: String,
}

/// Configuration for the distillation pass.
#[derive(Debug, Clone)]
pub struct DistillationConfig {
    pub enabled: bool,
    pub interval_hours: u64,
    pub confidence_auto_apply: f64,
    pub confidence_review_queue: f64,
    pub max_entries_per_file: usize,
    pub agent_id: String,
    pub review_notification: bool,
}

impl Default for DistillationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 4,
            confidence_auto_apply: 0.7,
            confidence_review_queue: 0.5,
            max_entries_per_file: 50,
            agent_id: "rarog".into(),
            review_notification: true,
        }
    }
}

/// Result of a distillation pass.
#[derive(Debug, Default)]
pub struct DistillationResult {
    pub threads_analyzed: usize,
    pub candidates_generated: usize,
    pub auto_applied: usize,
    pub queued_for_review: usize,
    pub discarded: usize,
    pub review_notifications_emitted: usize,
}

/// Run a distillation pass over old, undistilled threads.
pub async fn run_distillation_pass(
    db: &HistoryStore,
    config: &DistillationConfig,
    agent_data_dir: &std::path::Path,
) -> anyhow::Result<DistillationResult> {
    if !config.enabled {
        return Ok(DistillationResult::default());
    }

    let now = now_millis();
    let cutoff = now.saturating_sub(OLD_THREAD_AGE_MS);
    let thread_ids = list_undistilled_threads(db, cutoff, THREAD_BATCH_LIMIT).await?;
    let mut result = DistillationResult::default();
    let mut applied_per_target: HashMap<String, usize> = HashMap::new();
    let mut queued_per_target: HashMap<String, usize> = HashMap::new();
    let mut review_candidates = Vec::new();

    for thread_id in thread_ids {
        result.threads_analyzed += 1;
        let progress = db.get_memory_distillation_progress(&thread_id).await?;
        let messages = db
            .list_messages_after_cursor(
                &thread_id,
                progress.as_ref().map(|row| &row.last_processed_cursor),
                Some(40),
            )
            .await?;
        if messages.is_empty() {
            if let Some(progress) = progress {
                let refreshed_progress = MemoryDistillationProgressRow {
                    last_run_at_ms: now as i64,
                    updated_at_ms: now as i64,
                    agent_id: config.agent_id.clone(),
                    ..progress
                };
                db.upsert_memory_distillation_progress(&refreshed_progress)
                    .await?;
            }
            continue;
        }
        let candidates = extract_candidates_from_messages(&thread_id, &messages);

        let mut thread_last_cursor = progress
            .as_ref()
            .map(|row| row.last_processed_cursor.clone())
            .or_else(|| messages.last().map(AgentMessageCursor::from_message));
        let mut thread_last_span = progress.and_then(|row| row.last_processed_span);

        for candidate in candidates {
            result.candidates_generated += 1;
            if let Some(cursor) = candidate.last_processed_cursor.clone() {
                thread_last_cursor = Some(cursor);
            }
            if let Some(span) = candidate.source_message_span.clone() {
                thread_last_span = Some(span);
            }
            let target_key = candidate.target_file.clone();
            let applied_count = *applied_per_target.get(&target_key).unwrap_or(&0);
            let queued_count = *queued_per_target.get(&target_key).unwrap_or(&0);
            let remaining_budget = config
                .max_entries_per_file
                .saturating_sub(applied_count + queued_count);

            if remaining_budget == 0 {
                result.discarded += 1;
                log_distillation_candidate(db, &candidate, false, &config.agent_id).await?;
                continue;
            }

            if candidate.confidence >= config.confidence_auto_apply {
                if apply_distilled_candidate(db, agent_data_dir, config, &candidate).await? {
                    *applied_per_target.entry(target_key).or_insert(0) += 1;
                    result.auto_applied += 1;
                    log_distillation_candidate(db, &candidate, true, &config.agent_id).await?;
                } else {
                    result.discarded += 1;
                    log_distillation_candidate(db, &candidate, false, &config.agent_id).await?;
                }
            } else if candidate.confidence >= config.confidence_review_queue {
                *queued_per_target.entry(target_key).or_insert(0) += 1;
                result.queued_for_review += 1;
                review_candidates.push(candidate.clone());
                log_distillation_candidate(db, &candidate, false, &config.agent_id).await?;
            } else {
                result.discarded += 1;
                log_distillation_candidate(db, &candidate, false, &config.agent_id).await?;
            }
        }

        if let Some(last_processed_cursor) = thread_last_cursor {
            let progress_row = MemoryDistillationProgressRow {
                source_thread_id: thread_id.clone(),
                last_processed_cursor,
                last_processed_span: thread_last_span,
                last_run_at_ms: now as i64,
                updated_at_ms: now as i64,
                agent_id: config.agent_id.clone(),
            };
            db.upsert_memory_distillation_progress(&progress_row)
                .await?;
        }
    }

    if config.review_notification && !review_candidates.is_empty() {
        emit_review_notification(db, &review_candidates, &config.agent_id).await?;
        result.review_notifications_emitted = 1;
    }

    Ok(result)
}

async fn list_undistilled_threads(
    db: &HistoryStore,
    cutoff_ms: u64,
    limit: usize,
) -> anyhow::Result<Vec<String>> {
    let rows = db
        .read_db
        .query(
            "SELECT threads.id
                 FROM agent_threads AS threads
                 JOIN (
                     SELECT current.thread_id,
                            current.created_at AS latest_message_activity_at,
                            current.id AS latest_message_id
                     FROM agent_messages AS current
                     WHERE current.deleted_at IS NULL
                       AND NOT EXISTS (
                         SELECT 1
                         FROM agent_messages AS newer
                         WHERE newer.thread_id = current.thread_id
                           AND newer.deleted_at IS NULL
                           AND (
                               newer.created_at > current.created_at
                               OR (
                                   newer.created_at = current.created_at
                                   AND newer.id > current.id
                               )
                           )
                     )
                 ) AS activity
                   ON activity.thread_id = threads.id
                 LEFT JOIN memory_distillation_progress AS progress
                   ON progress.source_thread_id = threads.id
                 LEFT JOIN (
                     SELECT source_thread_id, MAX(created_at_ms) AS last_applied_at
                     FROM memory_distillation_log
                     WHERE applied_to_memory = 1
                     GROUP BY source_thread_id
                 ) AS applied
                   ON applied.source_thread_id = threads.id
                 WHERE activity.latest_message_activity_at < ?1
                   AND (
                       progress.last_processed_created_at_ms IS NULL
                       OR progress.last_processed_created_at_ms < activity.latest_message_activity_at
                       OR (
                           progress.last_processed_created_at_ms = activity.latest_message_activity_at
                           AND progress.last_processed_message_id < activity.latest_message_id
                       )
                   )
                   AND (
                       applied.last_applied_at IS NULL
                       OR applied.last_applied_at < activity.latest_message_activity_at
                   )
                 ORDER BY activity.latest_message_activity_at ASC
                 LIMIT ?2",
            db::db_params![cutoff_ms as i64, limit as i64],
        )
        .await?;
    rows.iter().map(|row| row.get::<String>(0)).collect()
}

fn extract_candidates_from_messages(
    thread_id: &str,
    messages: &[AgentDbMessage],
) -> Vec<DistillationCandidate> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut supporting_ranges: Vec<BTreeSet<usize>> = Vec::new();
    let mut candidates: Vec<DistillationCandidate> = Vec::new();
    let last_user_message_index = messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| message.role.eq_ignore_ascii_case("user"))
        .map(|(index, _)| index);

    for (index, message) in messages.iter().enumerate() {
        if !message.role.eq_ignore_ascii_case("user") {
            continue;
        }

        for line in message.content.lines() {
            let Some(mut candidate) = candidate_from_line(thread_id, index, line) else {
                continue;
            };
            let dedupe_key = format!(
                "{}::{}",
                candidate.target_file,
                normalize_distilled_fact(&candidate.distilled_fact)
            );
            if let Some(existing_index) = seen.get(&dedupe_key).copied() {
                if supporting_ranges[existing_index].insert(index) {
                    let span =
                        build_source_message_span(messages, &supporting_ranges[existing_index]);
                    candidates[existing_index].source_message_range = format_source_message_range(
                        &supporting_ranges[existing_index],
                        last_user_message_index,
                    );
                    candidates[existing_index].source_message_span = span.clone();
                    candidates[existing_index].last_processed_cursor =
                        span.as_ref().map(AgentMessageSpan::end_cursor);
                }
            } else {
                let mut support = BTreeSet::new();
                support.insert(index);
                let span = build_source_message_span(messages, &support);
                candidate.source_message_range =
                    format_source_message_range(&support, last_user_message_index);
                candidate.source_message_span = span.clone();
                candidate.last_processed_cursor = span.as_ref().map(AgentMessageSpan::end_cursor);
                seen.insert(dedupe_key, candidates.len());
                candidates.push(candidate);
                supporting_ranges.push(support);
            }
        }
    }

    candidates
}

fn build_source_message_span(
    messages: &[AgentDbMessage],
    message_indices: &BTreeSet<usize>,
) -> Option<AgentMessageSpan> {
    let first = *message_indices.first()?;
    let last = *message_indices.last()?;
    let start = AgentMessageCursor::from_message(messages.get(first)?);
    let end = AgentMessageCursor::from_message(messages.get(last)?);

    if first == last {
        Some(AgentMessageSpan::LastTurn { message: end })
    } else {
        Some(AgentMessageSpan::Range { start, end })
    }
}

fn format_source_message_range(
    message_indices: &BTreeSet<usize>,
    last_user_message_index: Option<usize>,
) -> Option<String> {
    let first = *message_indices.first()?;
    let last = *message_indices.last()?;

    if first == last && Some(last) == last_user_message_index {
        Some("last_turn".to_string())
    } else {
        Some(format!("{}-{}", first + 1, last + 1))
    }
}

fn candidate_from_line(
    thread_id: &str,
    _message_index: usize,
    raw_line: &str,
) -> Option<DistillationCandidate> {
    let cleaned = sanitize_line(raw_line)?;
    if looks_ephemeral(&cleaned) {
        return None;
    }

    let lower = cleaned.to_ascii_lowercase();
    let workspace_scoped = has_workspace_markers(&lower);

    let (category, target_file, confidence, reasoning) = if lower.contains("package name")
        || lower.contains("cargo -p")
        || lower.contains("crate path")
        || lower.contains("workspace")
        || lower.contains("daemon package")
        || lower.contains("use `")
        || lower.contains("path is `")
    {
        (
            MemoryCategory::Convention,
            "MEMORY.md",
            0.86,
            "explicit workspace convention or implementation correction",
        )
    } else if lower.contains("i prefer")
        || lower.contains("prefer ")
        || lower.contains("summary-first")
        || lower.contains("be concise")
        || lower.contains("be direct")
        || lower.contains("do not ask")
        || lower.contains("don't ask")
        || lower.contains("verbose")
    {
        (
            MemoryCategory::Preference,
            "USER.md",
            0.78,
            "explicit operator preference phrasing",
        )
    } else if lower.contains("actually")
        || lower.contains("instead")
        || lower.contains("correction")
        || lower.contains("wrong")
        || lower.contains("use the cargo package name")
    {
        (
            MemoryCategory::Correction,
            if workspace_scoped {
                "MEMORY.md"
            } else {
                "USER.md"
            },
            if workspace_scoped { 0.84 } else { 0.72 },
            "high-signal correction language",
        )
    } else if lower.contains("usually")
        || lower.contains("often")
        || lower.contains("tend to")
        || lower.contains("responds well")
    {
        (
            MemoryCategory::Pattern,
            if workspace_scoped {
                "MEMORY.md"
            } else {
                "USER.md"
            },
            0.58,
            "stable-looking behavioral pattern",
        )
    } else {
        return None;
    };

    Some(DistillationCandidate {
        source_thread_id: thread_id.to_string(),
        source_message_range: None,
        source_message_span: None,
        last_processed_cursor: None,
        distilled_fact: cleaned,
        target_file: target_file.to_string(),
        category,
        confidence,
        reasoning: reasoning.to_string(),
    })
}

fn sanitize_line(raw_line: &str) -> Option<String> {
    let cleaned = raw_line
        .trim()
        .trim_start_matches(['-', '*', '>', '•', ' '])
        .trim();
    if cleaned.is_empty() || cleaned.ends_with('?') {
        return None;
    }

    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() < MIN_FACT_CHARS || collapsed.len() > MAX_FACT_CHARS {
        return None;
    }
    Some(collapsed)
}

fn looks_ephemeral(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("begin implementation")
        || lower.starts_with("please proceed")
        || lower.starts_with("thanks")
        || lower.starts_with("ok")
        || lower.starts_with("okay")
        || lower.contains("current task")
        || lower.contains("right now")
        || lower.contains("today")
        || lower.contains("immediately")
}

fn has_workspace_markers(lower: &str) -> bool {
    [
        "cargo",
        "crate",
        "workspace",
        "repo",
        "repository",
        "daemon",
        "sqlite",
        "rust",
        "package",
        "memory.md",
        "user.md",
        "soul.md",
        "thread",
        "agent",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

async fn apply_distilled_candidate(
    db: &HistoryStore,
    agent_data_dir: &std::path::Path,
    config: &DistillationConfig,
    candidate: &DistillationCandidate,
) -> anyhow::Result<bool> {
    let target = match candidate.target_file.as_str() {
        "SOUL.md" => MemoryTarget::Soul,
        "USER.md" => MemoryTarget::User,
        _ => MemoryTarget::Memory,
    };
    let scope_id = current_agent_scope_id();
    let paths = memory_paths_for_scope(agent_data_dir, &scope_id);
    let path = match target {
        MemoryTarget::Soul => paths.soul_path,
        MemoryTarget::Memory => paths.memory_path,
        MemoryTarget::User => paths.user_path,
    };
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if content_contains_equivalent_fact(&existing, &candidate.distilled_fact) {
        return Ok(false);
    }
    let note = format_distilled_note(&candidate.distilled_fact, now_millis());
    let prepared_existing =
        prepare_distilled_target_for_append(&existing, &note, target, config.max_entries_per_file);
    if prepared_existing != existing {
        tokio::fs::write(&path, &prepared_existing).await?;
    }

    let applied = apply_memory_update(
        agent_data_dir,
        db,
        target,
        MemoryUpdateMode::Append,
        &note,
        MemoryWriteContext {
            source_kind: "memory_distillation",
            thread_id: Some(candidate.source_thread_id.as_str()),
            task_id: None,
            goal_run_id: None,
        },
    )
    .await;

    match applied {
        Ok(_) => {
            trim_distilled_entries_to_limit(&path, config.max_entries_per_file).await?;
            Ok(true)
        }
        Err(error) => {
            tracing::warn!(
                thread_id = %candidate.source_thread_id,
                target = %candidate.target_file,
                "failed to apply distilled candidate: {error}"
            );
            Ok(false)
        }
    }
}

fn format_distilled_note(fact: &str, timestamp_ms: u64) -> String {
    let timestamp = chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms as i64)
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| timestamp_ms.to_string());
    format!("- [distilled][{timestamp}] {fact}")
}

fn content_contains_equivalent_fact(content: &str, fact: &str) -> bool {
    let normalized_fact = normalize_distilled_fact(fact);
    if normalized_fact.is_empty() {
        return false;
    }

    content.lines().any(|line| {
        normalized_line_fact(line)
            .as_ref()
            .is_some_and(|existing| existing == &normalized_fact)
    })
}

fn normalized_line_fact(line: &str) -> Option<String> {
    let cleaned = strip_distilled_markup(line);
    if cleaned.is_empty() || cleaned.starts_with('#') {
        return None;
    }

    let normalized = normalize_distilled_fact(&cleaned);
    (!normalized.is_empty()).then_some(normalized)
}

fn strip_distilled_markup(line: &str) -> String {
    let mut cleaned = line.trim();
    while let Some(rest) = cleaned.strip_prefix(['-', '*', '>', ' ']) {
        cleaned = rest.trim_start();
    }

    let bytes = cleaned.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && bytes.get(idx) == Some(&b'.') {
        cleaned = cleaned[idx + 1..].trim_start();
    }

    while let Some(rest) = cleaned.strip_prefix('[') {
        if let Some(end_idx) = rest.find(']') {
            cleaned = rest[end_idx + 1..].trim_start();
        } else {
            break;
        }
    }

    cleaned
        .trim_matches('`')
        .trim_matches('*')
        .trim_matches('_')
        .trim()
        .to_string()
}

fn normalize_distilled_fact(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '/' || ch == '.' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

async fn log_distillation_candidate(
    db: &HistoryStore,
    candidate: &DistillationCandidate,
    applied_to_memory: bool,
    agent_id: &str,
) -> anyhow::Result<()> {
    let source_thread_id = candidate.source_thread_id.clone();
    let source_message_range = candidate.source_message_range.clone();
    let source_message_span = candidate.source_message_span.clone();
    let distilled_fact = candidate.distilled_fact.clone();
    let target_file = candidate.target_file.clone();
    let category = candidate.category.to_string();
    let confidence = candidate.confidence;
    let created_at_ms = now_millis() as i64;
    let agent_id = agent_id.to_string();

    db.append_memory_distillation_log(
        &source_thread_id,
        source_message_range.as_deref(),
        source_message_span.as_ref(),
        &distilled_fact,
        &target_file,
        &category,
        confidence,
        created_at_ms,
        applied_to_memory,
        &agent_id,
    )
    .await
}

async fn emit_review_notification(
    db: &HistoryStore,
    candidates: &[DistillationCandidate],
    agent_id: &str,
) -> anyhow::Result<()> {
    let count = candidates.len();
    let preview = candidates
        .iter()
        .take(3)
        .map(|candidate| {
            format!(
                "- {} [{} → {}]",
                candidate.distilled_fact, candidate.category, candidate.target_file
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut digest = Sha256::new();
    for candidate in candidates {
        digest.update(candidate.source_thread_id.as_bytes());
        digest.update([0u8]);
        digest.update(candidate.distilled_fact.as_bytes());
        digest.update([0xffu8]);
    }
    let digest_hex = format!("{:x}", digest.finalize());
    let now = now_millis() as i64;

    db.upsert_notification(&InboxNotification {
        id: format!("distillation-review:{}:{}", agent_id, &digest_hex[..16]),
        source: "memory_distillation".to_string(),
        kind: "memory_distillation_review".to_string(),
        title: format!("Memory distillation queued {} review item(s)", count),
        body: if count > 3 {
            format!("{}\n- … and {} more", preview, count - 3)
        } else {
            preview
        },
        subtitle: Some(agent_id.to_string()),
        severity: "info".to_string(),
        created_at: now,
        updated_at: now,
        read_at: None,
        archived_at: None,
        deleted_at: None,
        actions: Vec::new(),
        metadata_json: Some(
            serde_json::json!({
                "candidate_count": count,
                "agent_id": agent_id,
                "target_files": candidates.iter().map(|c| c.target_file.as_str()).collect::<Vec<_>>(),
                "categories": candidates.iter().map(|c| c.category.to_string()).collect::<Vec<_>>()
            })
            .to_string(),
        ),
    })
    .await
}

async fn trim_distilled_entries_to_limit(
    path: &std::path::Path,
    max_entries: usize,
) -> anyhow::Result<()> {
    if max_entries == 0 {
        return Ok(());
    }

    let existing = tokio::fs::read_to_string(path).await.unwrap_or_default();
    let trimmed = trim_distilled_entries_in_content(&existing, max_entries);
    if trimmed != existing {
        tokio::fs::write(path, trimmed).await?;
    }
    Ok(())
}

fn trim_distilled_entries_in_content(content: &str, max_entries: usize) -> String {
    if max_entries == 0 {
        return content.to_string();
    }

    trim_distilled_entries_to_capacity(content, max_entries)
}

fn trim_distilled_entries_to_capacity(content: &str, max_entries: usize) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let distilled_indices = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| is_distilled_entry(line).then_some(idx))
        .collect::<Vec<_>>();
    if distilled_indices.len() <= max_entries {
        return content.to_string();
    }

    let remove_count = distilled_indices.len().saturating_sub(max_entries);
    let to_remove = distilled_indices
        .into_iter()
        .take(remove_count)
        .collect::<HashSet<_>>();
    let kept = lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| (!to_remove.contains(&idx)).then_some(line))
        .collect::<Vec<_>>()
        .join("\n");

    if content.ends_with('\n') && !kept.is_empty() {
        format!("{kept}\n")
    } else {
        kept
    }
}

fn prepare_distilled_target_for_append(
    content: &str,
    note: &str,
    target: MemoryTarget,
    max_entries: usize,
) -> String {
    let mut prepared = if max_entries > 0 {
        trim_distilled_entries_to_capacity(content, max_entries.saturating_sub(1))
    } else {
        content.to_string()
    };

    while append_distilled_content(&prepared, note).chars().count() > target.limit_chars() {
        let Some(trimmed) = trim_oldest_distilled_entry(&prepared) else {
            break;
        };
        prepared = trimmed;
    }

    prepared
}

fn trim_oldest_distilled_entry(content: &str) -> Option<String> {
    let lines = content.lines().collect::<Vec<_>>();
    let remove_idx = lines.iter().position(|line| is_distilled_entry(line))?;
    let kept = lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| (idx != remove_idx).then_some(line))
        .collect::<Vec<_>>()
        .join("\n");

    Some(if content.ends_with('\n') && !kept.is_empty() {
        format!("{kept}\n")
    } else {
        kept
    })
}

fn append_distilled_content(existing: &str, addition: &str) -> String {
    let existing = existing.trim_end();
    if existing.is_empty() {
        addition.to_string()
    } else {
        format!("{existing}\n\n{addition}")
    }
}

fn is_distilled_entry(line: &str) -> bool {
    line.trim_start().starts_with("- [distilled]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;
    use zorai_protocol::{AgentDbMessage, AgentDbThread};

    async fn create_thread_with_user_message(
        history: &HistoryStore,
        thread_id: &str,
        message_created_at: i64,
    ) -> anyhow::Result<()> {
        history
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("Rarog".to_string()),
                title: thread_id.to_string(),
                created_at: message_created_at,
                updated_at: message_created_at,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await?;

        history
            .add_message(&AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: format!("{thread_id}-m1"),
                thread_id: thread_id.to_string(),
                created_at: message_created_at,
                role: "user".to_string(),
                content: "Use the cargo package name `zorai-daemon` for `cargo -p`.".to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;

        Ok(())
    }

    #[test]
    fn memory_category_display() {
        assert_eq!(MemoryCategory::Preference.to_string(), "preference");
        assert_eq!(MemoryCategory::Correction.to_string(), "correction");
        assert_eq!(MemoryCategory::Lesson.to_string(), "lesson");
    }

    #[test]
    fn default_config_sane() {
        let cfg = DistillationConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.confidence_auto_apply, 0.7);
        assert_eq!(cfg.agent_id, "rarog");
    }

    #[test]
    fn extracts_workspace_convention_candidate() {
        let candidate = candidate_from_line(
            "thread-1",
            0,
            "Use the cargo package name `zorai-daemon`, not the crate path, when invoking `cargo -p`.",
        )
        .expect("candidate");
        assert_eq!(candidate.category, MemoryCategory::Convention);
        assert_eq!(candidate.target_file, "MEMORY.md");
        assert!(candidate.confidence >= 0.8);
    }

    #[test]
    fn extracts_operator_preference_candidate() {
        let candidate = candidate_from_line(
            "thread-1",
            0,
            "I prefer summary-first answers that still include the hard details below.",
        )
        .expect("candidate");
        assert_eq!(candidate.category, MemoryCategory::Preference);
        assert_eq!(candidate.target_file, "USER.md");
    }

    #[test]
    fn extract_candidates_uses_real_message_ranges_for_repeated_support() {
        let messages = vec![
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m1".into(),
                thread_id: "thread-1".into(),
                created_at: 1,
                role: "user".into(),
                content: "Use the cargo package name `zorai-daemon` for `cargo -p`.".into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m2".into(),
                thread_id: "thread-1".into(),
                created_at: 2,
                role: "assistant".into(),
                content: "Acknowledged.".into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m3".into(),
                thread_id: "thread-1".into(),
                created_at: 3,
                role: "user".into(),
                content: "Use the cargo package name `zorai-daemon` for `cargo -p`.".into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
        ];

        let candidates = extract_candidates_from_messages("thread-1", &messages);
        let candidate = candidates
            .into_iter()
            .find(|item| item.target_file == "MEMORY.md")
            .expect("candidate should exist");
        assert_eq!(candidate.source_message_range.as_deref(), Some("1-3"));
    }

    #[test]
    fn extract_candidates_marks_last_user_turn_as_last_turn() {
        let messages = vec![
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m1".into(),
                thread_id: "thread-1".into(),
                created_at: 1,
                role: "assistant".into(),
                content: "What do you prefer?".into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m2".into(),
                thread_id: "thread-1".into(),
                created_at: 2,
                role: "user".into(),
                content:
                    "I prefer summary-first answers that still include the hard details below."
                        .into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
        ];

        let candidates = extract_candidates_from_messages("thread-1", &messages);
        let candidate = candidates
            .into_iter()
            .find(|item| item.target_file == "USER.md")
            .expect("candidate should exist");
        assert_eq!(candidate.source_message_range.as_deref(), Some("last_turn"));
    }

    #[test]
    fn filters_ephemeral_lines() {
        assert!(candidate_from_line("thread-1", 0, "begin implementation").is_none());
        assert!(candidate_from_line("thread-1", 0, "Can you continue?").is_none());
    }

    #[test]
    fn trims_oldest_distilled_entries_only() {
        let content = "# Memory\n\n- durable fact\n- [distilled] oldest\n- [distilled] middle\n- [distilled] newest\n";
        let trimmed = trim_distilled_entries_in_content(content, 2);
        assert!(trimmed.contains("- durable fact"));
        assert!(!trimmed.contains("- [distilled] oldest"));
        assert!(trimmed.contains("- [distilled] middle"));
        assert!(trimmed.contains("- [distilled] newest"));
    }

    #[test]
    fn formatted_distilled_note_includes_rfc3339_timestamp() {
        let note = format_distilled_note("Use the cargo package name `zorai-daemon`.", 0);
        assert_eq!(
            note,
            "- [distilled][1970-01-01T00:00:00Z] Use the cargo package name `zorai-daemon`."
        );
    }

    #[test]
    fn equivalent_fact_detection_ignores_distilled_provenance_tags() {
        let content = concat!(
            "# Memory\n",
            "- [distilled] Use the cargo package name `zorai-daemon` for `cargo -p`.\n",
            "- [distilled][2026-04-12T09:10:11Z] Prefer summary-first answers with hard details below.\n"
        );

        assert!(content_contains_equivalent_fact(
            content,
            "Use the cargo package name `zorai-daemon` for `cargo -p`."
        ));
        assert!(content_contains_equivalent_fact(
            content,
            "Prefer summary-first answers with hard details below."
        ));
        assert!(!content_contains_equivalent_fact(
            content,
            "Use the crate path when invoking cargo."
        ));
    }

    #[tokio::test]
    async fn apply_distilled_candidate_adds_timestamp_and_skips_equivalent_duplicate(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        ensure_memory_files(&root).await?;

        let fact =
            "Prefer deterministic rust memory-distillation sequencing over ad-hoc extraction.";
        let candidate = DistillationCandidate {
            source_thread_id: "thread-1".into(),
            source_message_range: Some("msg#1".into()),
            source_message_span: None,
            last_processed_cursor: None,
            distilled_fact: fact.into(),
            target_file: "MEMORY.md".into(),
            category: MemoryCategory::Convention,
            confidence: 0.91,
            reasoning: "explicit operator correction".into(),
        };
        let config = DistillationConfig::default();

        assert!(apply_distilled_candidate(&history, &root, &config, &candidate).await?);

        let memory_path = memory_paths_for_scope(&root, &current_agent_scope_id()).memory_path;
        let initial = tokio::fs::read_to_string(&memory_path).await?;
        assert!(initial.contains("- [distilled]["));
        assert!(initial.contains(fact));

        assert!(!apply_distilled_candidate(&history, &root, &config, &candidate).await?);

        let final_content = tokio::fs::read_to_string(&memory_path).await?;
        assert_eq!(final_content.matches(fact).count(), 1);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn apply_distilled_candidate_trims_memory_to_fit_char_limit() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        ensure_memory_files(&root).await?;

        let paths = memory_paths_for_scope(&root, &current_agent_scope_id());
        let fact = "Prefer memory distillation to evict oldest distilled notes before rejecting a valid append.";
        let new_note = format_distilled_note(fact, 0);
        let limit = MemoryTarget::Memory.limit_chars();
        let marker = "old fact 0";
        let base_content = "# Memory\n\n- durable memory fact stays in place.\n";
        let mut content = None;

        for size in 1..=limit {
            let old_note = format_distilled_note(&format!("{marker} {}", "x".repeat(size)), 0);
            let candidate_content = append_distilled_content(base_content, &old_note);
            if candidate_content.chars().count() <= limit
                && append_distilled_content(&candidate_content, &new_note)
                    .chars()
                    .count()
                    > limit
            {
                content = Some(candidate_content);
                break;
            }
        }

        let content =
            content.expect("fixture should find a note that fits now but blocks the append");

        assert!(
            append_distilled_content(&content, &new_note)
                .chars()
                .count()
                > limit,
            "fixture should require pre-trimming before append"
        );
        tokio::fs::write(&paths.memory_path, &content).await?;

        let candidate = DistillationCandidate {
            source_thread_id: "thread-memory-trim".into(),
            source_message_range: Some("msg#1".into()),
            source_message_span: None,
            last_processed_cursor: None,
            distilled_fact: fact.into(),
            target_file: "MEMORY.md".into(),
            category: MemoryCategory::Convention,
            confidence: 0.91,
            reasoning: "limit-aware append".into(),
        };

        assert!(
            apply_distilled_candidate(&history, &root, &DistillationConfig::default(), &candidate)
                .await?
        );

        let final_content = tokio::fs::read_to_string(&paths.memory_path).await?;
        assert!(final_content.contains("durable memory fact stays in place."));
        assert!(final_content.contains(fact));
        assert!(final_content.chars().count() <= limit);
        assert!(
            !final_content.contains(marker),
            "oldest distilled entry should be evicted first"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn apply_distilled_candidate_targets_soul_file_when_requested() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        ensure_memory_files(&root).await?;

        let fact =
            "Dream-state SOUL hint: prefer blacksmith-grade toolcraft for deterministic workflows.";
        let candidate = DistillationCandidate {
            source_thread_id: "thread-soul".into(),
            source_message_range: Some("msg#1".into()),
            source_message_span: None,
            last_processed_cursor: None,
            distilled_fact: fact.into(),
            target_file: "SOUL.md".into(),
            category: MemoryCategory::Lesson,
            confidence: 0.93,
            reasoning: "identity-level distilled lesson".into(),
        };
        let config = DistillationConfig::default();

        assert!(apply_distilled_candidate(&history, &root, &config, &candidate).await?);

        let paths = memory_paths_for_scope(&root, &current_agent_scope_id());
        let soul = tokio::fs::read_to_string(&paths.soul_path).await?;
        let memory = tokio::fs::read_to_string(&paths.memory_path).await?;

        assert!(
            soul.contains(fact),
            "SOUL.md target should append the distilled fact to SOUL.md"
        );
        assert!(
            !memory.contains(fact),
            "SOUL.md-targeted candidate must not be redirected into MEMORY.md"
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn apply_distilled_candidate_trims_user_file_to_entry_limit() -> anyhow::Result<()> {
        // USER.md updates flow through a process-global sync-state mutex
        // (`UserProfileSyncState`). A prior test in the same process can
        // leave that state in `Reconciling` or `Dirty`, which causes
        // `handle_user_memory_append_with_reconcile` to stage the
        // distilled note instead of writing it through — the assertion
        // below would then see the file unchanged. Acquire the test
        // guard and reset to `Clean` so this test is robust to ordering.
        let _user_sync_guard =
            crate::agent::operator_profile::user_sync::acquire_user_sync_test_guard();
        crate::agent::operator_profile::user_sync::set_user_sync_state_for_test(
            crate::agent::operator_profile::user_sync::UserProfileSyncState::Clean,
        );
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        ensure_memory_files(&root).await?;

        let paths = memory_paths_for_scope(&root, &current_agent_scope_id());
        tokio::fs::write(
            &paths.user_path,
            concat!(
                "# User\n\n",
                "- durable user fact\n",
                "- [distilled][2026-01-01T00:00:00Z] Prefer terse answers first.\n",
                "- [distilled][2026-01-02T00:00:00Z] Ask one targeted clarification question when needed.\n"
            ),
        )
        .await?;

        let candidate = DistillationCandidate {
            source_thread_id: "thread-user-trim".into(),
            source_message_range: Some("msg#1".into()),
            source_message_span: None,
            last_processed_cursor: None,
            distilled_fact: "Prefer direct answers with concrete next actions.".into(),
            target_file: "USER.md".into(),
            category: MemoryCategory::Preference,
            confidence: 0.88,
            reasoning: "explicit operator preference".into(),
        };
        let config = DistillationConfig {
            max_entries_per_file: 2,
            ..DistillationConfig::default()
        };

        assert!(apply_distilled_candidate(&history, &root, &config, &candidate).await?);

        let user = tokio::fs::read_to_string(&paths.user_path).await?;
        assert!(user.contains("- durable user fact"));
        assert!(!user.contains("Prefer terse answers first."));
        assert!(user.contains("Ask one targeted clarification question when needed."));
        assert!(user.contains("Prefer direct answers with concrete next actions."));
        assert_eq!(user.matches("- [distilled]").count(), 2);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn list_undistilled_threads_keeps_reviewed_but_unapplied_threads_eligible(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;

        for (id, message_created_at) in [
            ("old-never-distilled", 1_000_i64),
            ("old-reviewed-only", 2_000_i64),
            ("old-applied", 3_000_i64),
            ("recent-thread", 9_500_i64),
        ] {
            create_thread_with_user_message(&history, id, message_created_at).await?;
        }

        log_distillation_candidate(
            &history,
            &DistillationCandidate {
                source_thread_id: "old-reviewed-only".into(),
                source_message_range: Some("1-1".into()),
                source_message_span: None,
                last_processed_cursor: None,
                distilled_fact: "Use the cargo package name `zorai-daemon` for `cargo -p`.".into(),
                target_file: "MEMORY.md".into(),
                category: MemoryCategory::Convention,
                confidence: 0.62,
                reasoning: "queued for review".into(),
            },
            false,
            "rarog",
        )
        .await?;

        log_distillation_candidate(
            &history,
            &DistillationCandidate {
                source_thread_id: "old-applied".into(),
                source_message_range: Some("1-1".into()),
                source_message_span: None,
                last_processed_cursor: None,
                distilled_fact: "Prefer summary-first answers with hard details below.".into(),
                target_file: "USER.md".into(),
                category: MemoryCategory::Preference,
                confidence: 0.88,
                reasoning: "auto applied".into(),
            },
            true,
            "rarog",
        )
        .await?;

        let selected = list_undistilled_threads(&history, 9_000, 10).await?;

        assert!(selected.contains(&"old-never-distilled".to_string()));
        assert!(selected.contains(&"old-reviewed-only".to_string()));
        assert!(!selected.contains(&"old-applied".to_string()));
        assert!(!selected.contains(&"recent-thread".to_string()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn list_undistilled_threads_reincludes_threads_updated_after_applied_distillation(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;

        for (id, message_created_at) in [
            ("applied-before-last-activity", 8_000_i64),
            ("applied-after-last-activity", 8_000_i64),
            ("never-applied", 7_000_i64),
        ] {
            create_thread_with_user_message(&history, id, message_created_at).await?;
        }

        history
            .conn
            .call(|conn| {
                conn.execute(
                    "INSERT INTO memory_distillation_log \
                     (source_thread_id, source_message_range, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        "applied-before-last-activity",
                        "1-1",
                        "Use the cargo package name `zorai-daemon` for `cargo -p`.",
                        "MEMORY.md",
                        "convention",
                        0.91_f64,
                        7_500_i64,
                        1_i64,
                        "rarog",
                    ],
                )?;
                conn.execute(
                    "INSERT INTO memory_distillation_log \
                     (source_thread_id, source_message_range, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        "applied-after-last-activity",
                        "1-1",
                        "Prefer summary-first answers with hard details below.",
                        "USER.md",
                        "preference",
                        0.88_f64,
                        8_500_i64,
                        1_i64,
                        "rarog",
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let selected = list_undistilled_threads(&history, 9_000, 10).await?;

        assert!(selected.contains(&"applied-before-last-activity".to_string()));
        assert!(selected.contains(&"never-applied".to_string()));
        assert!(!selected.contains(&"applied-after-last-activity".to_string()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn list_undistilled_threads_excludes_equal_timestamp_distillations() -> anyhow::Result<()>
    {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;

        create_thread_with_user_message(&history, "equal-timestamp", 8_000_i64).await?;

        history
            .conn
            .call(|conn| {
                conn.execute(
                    "INSERT INTO memory_distillation_log \
                     (source_thread_id, source_message_range, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        "equal-timestamp",
                        "1-1",
                        "Use the cargo package name `zorai-daemon` for `cargo -p`.",
                        "MEMORY.md",
                        "convention",
                        0.91_f64,
                        8_000_i64,
                        1_i64,
                        "rarog",
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let selected = list_undistilled_threads(&history, 9_000, 10).await?;

        assert!(!selected.contains(&"equal-timestamp".to_string()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn list_undistilled_threads_detects_same_timestamp_newer_message_after_progress(
    ) -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;

        create_thread_with_user_message(&history, "same-ts-newer-id", 8_000_i64).await?;

        history
            .upsert_memory_distillation_progress(&MemoryDistillationProgressRow {
                source_thread_id: "same-ts-newer-id".into(),
                last_processed_cursor: AgentMessageCursor {
                    created_at: 8_000_i64,
                    message_id: "same-ts-newer-id-m1".into(),
                },
                last_processed_span: Some(AgentMessageSpan::LastTurn {
                    message: AgentMessageCursor {
                        created_at: 8_000_i64,
                        message_id: "same-ts-newer-id-m1".into(),
                    },
                }),
                last_run_at_ms: 8_100_i64,
                updated_at_ms: 8_100_i64,
                agent_id: "rarog".into(),
            })
            .await?;

        history
            .add_message(&AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "same-ts-newer-id-m2".into(),
                thread_id: "same-ts-newer-id".into(),
                created_at: 8_000_i64,
                role: "user".into(),
                content: "Prefer summary-first answers with hard details below.".into(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;

        let selected = list_undistilled_threads(&history, 9_000, 10).await?;

        assert!(selected.contains(&"same-ts-newer-id".to_string()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn list_undistilled_threads_ignores_bookkeeping_only_thread_updates() -> anyhow::Result<()>
    {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;

        create_thread_with_user_message(&history, "bookkeeping-only", 7_000_i64).await?;

        history
            .conn
            .call(|conn| {
                conn.execute(
                    "INSERT INTO memory_distillation_log \
                     (source_thread_id, source_message_range, distilled_fact, target_file, category, confidence, created_at_ms, applied_to_memory, agent_id) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        "bookkeeping-only",
                        "1-1",
                        "Use the cargo package name `zorai-daemon` for `cargo -p`.",
                        "MEMORY.md",
                        "convention",
                        0.91_f64,
                        7_500_i64,
                        1_i64,
                        "rarog",
                    ],
                )?;
                conn.execute(
                    "UPDATE agent_threads SET updated_at = ?2 WHERE id = ?1",
                    rusqlite::params!["bookkeeping-only", 8_500_i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let selected = list_undistilled_threads(&history, 9_000, 10).await?;

        assert!(!selected.contains(&"bookkeeping-only".to_string()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn review_notification_is_persisted() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        let candidates = vec![
            DistillationCandidate {
                source_thread_id: "thread-1".into(),
                source_message_range: Some("msg#1".into()),
                source_message_span: None,
                last_processed_cursor: None,
                distilled_fact: "Use the cargo package name `zorai-daemon` for `cargo -p`.".into(),
                target_file: "MEMORY.md".into(),
                category: MemoryCategory::Convention,
                confidence: 0.62,
                reasoning: "explicit correction".into(),
            },
            DistillationCandidate {
                source_thread_id: "thread-2".into(),
                source_message_range: Some("msg#2".into()),
                source_message_span: None,
                last_processed_cursor: None,
                distilled_fact: "Prefer summary-first answers with hard details below.".into(),
                target_file: "USER.md".into(),
                category: MemoryCategory::Preference,
                confidence: 0.58,
                reasoning: "explicit operator preference".into(),
            },
        ];

        emit_review_notification(&history, &candidates, "rarog").await?;

        let notifications = history.list_notifications(false, Some(10)).await?;
        let notification = notifications
            .into_iter()
            .find(|item| item.kind == "memory_distillation_review")
            .expect("memory distillation review notification should exist");
        assert!(notification.title.contains("2 review item"));
        assert!(notification.body.contains("zorai-daemon"));
        assert!(notification.body.contains("summary-first"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[tokio::test]
    async fn run_distillation_pass_auto_applies_and_queues_review_with_progress_persistence(
    ) -> anyhow::Result<()> {
        // Same process-global user-sync-state caveat as
        // `apply_distilled_candidate_trims_user_file_to_entry_limit` — a
        // prior test can leave the state non-Clean and cause USER.md
        // updates to stage instead of writing through.
        let _user_sync_guard =
            crate::agent::operator_profile::user_sync::acquire_user_sync_test_guard();
        crate::agent::operator_profile::user_sync::set_user_sync_state_for_test(
            crate::agent::operator_profile::user_sync::UserProfileSyncState::Clean,
        );
        let root = std::env::temp_dir().join(format!("zorai-distill-test-{}", Uuid::new_v4()));
        let history = HistoryStore::new_test_store(&root).await?;
        ensure_memory_files(&root).await?;

        let thread_id = "thread-distill-e2e";
        let message_id = format!("{thread_id}-m1");
        let created_at = 1_000_i64;

        history
            .create_thread(&AgentDbThread {
                id: thread_id.to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("Rarog".to_string()),
                title: "memory distillation e2e".to_string(),
                created_at,
                updated_at: created_at,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            })
            .await?;

        history
            .add_message(&AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: message_id.clone(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "user".to_string(),
                content: concat!(
                    "Use the cargo package name `zorai-daemon` for `cargo -p`.\n",
                    "I prefer summary-first answers that still include the hard details below.\n",
                    "I usually respond well to concise progress checkpoints after each major step."
                )
                .to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;

        let result = run_distillation_pass(&history, &DistillationConfig::default(), &root).await?;

        assert_eq!(result.threads_analyzed, 1);
        assert_eq!(result.candidates_generated, 3);
        assert_eq!(result.auto_applied, 2);
        assert_eq!(result.queued_for_review, 1);
        assert_eq!(result.discarded, 0);
        assert_eq!(result.review_notifications_emitted, 1);

        let paths = memory_paths_for_scope(&root, &current_agent_scope_id());
        let memory = tokio::fs::read_to_string(&paths.memory_path).await?;
        let user = tokio::fs::read_to_string(&paths.user_path).await?;

        assert!(memory.contains("Use the cargo package name `zorai-daemon` for `cargo -p`."));
        assert!(user
            .contains("I prefer summary-first answers that still include the hard details below."));
        assert!(
            !user.contains(
                "I usually respond well to concise progress checkpoints after each major step."
            ),
            "borderline candidate should be queued for review, not auto-applied"
        );

        let notifications = history.list_notifications(false, Some(10)).await?;
        let review_notification = notifications
            .into_iter()
            .find(|item| item.kind == "memory_distillation_review")
            .expect("memory distillation review notification should exist");
        assert!(review_notification.body.contains(
            "I usually respond well to concise progress checkpoints after each major step."
        ));

        let progress = history
            .get_memory_distillation_progress(thread_id)
            .await?
            .expect("progress row should be persisted");
        assert_eq!(progress.source_thread_id, thread_id);
        assert_eq!(progress.last_processed_cursor.created_at, created_at);
        assert_eq!(progress.last_processed_cursor.message_id, message_id);

        let log_rows = history.list_memory_distillation_log(10).await?;
        let thread_rows = log_rows
            .into_iter()
            .filter(|row| row.source_thread_id == thread_id)
            .collect::<Vec<_>>();
        assert_eq!(thread_rows.len(), 3);
        assert_eq!(
            thread_rows
                .iter()
                .filter(|row| row.applied_to_memory)
                .count(),
            2
        );
        assert!(thread_rows.iter().any(|row| {
            !row.applied_to_memory
                && row.target_file == "USER.md"
                && row.category == "pattern"
                && row
                    .distilled_fact
                    .contains("I usually respond well to concise progress checkpoints")
        }));

        fs::remove_dir_all(root)?;
        Ok(())
    }
}
