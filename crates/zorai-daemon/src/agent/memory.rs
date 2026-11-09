//! Persistent memory helpers for SOUL.md, MEMORY.md, and USER.md.

use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::operator_profile::user_sync::{
    current_user_sync_state, handle_user_memory_append_with_reconcile, UserProfileSyncState,
};
use super::*;
use crate::history::{HistoryStore, MemoryProvenanceRecord};

const MIN_LIMIT_CHARS: usize = 200;

static SOUL_LIMIT_CHARS: AtomicUsize = AtomicUsize::new(2_000);
static MEMORY_LIMIT_CHARS: AtomicUsize = AtomicUsize::new(3_600);
static USER_LIMIT_CHARS: AtomicUsize = AtomicUsize::new(1_800);

pub(crate) fn set_memory_limits(limits: &MemoryLimitsConfig) {
    SOUL_LIMIT_CHARS.store(limits.soul_chars.max(MIN_LIMIT_CHARS), Ordering::Relaxed);
    MEMORY_LIMIT_CHARS.store(limits.memory_chars.max(MIN_LIMIT_CHARS), Ordering::Relaxed);
    USER_LIMIT_CHARS.store(limits.user_chars.max(MIN_LIMIT_CHARS), Ordering::Relaxed);
}

const DEFAULT_MEMORY: &str = "# Memory\n\n## zorai Architecture (Verified)\n- zorai is a Rust, daemon-first multi-agent runtime\n- Daemon-first: daemon owns all state; TUI/Electron/CLI/MCP/gateway are clients\n- Memory: layered — SOUL.md (identity), MEMORY.md (durable facts/strategy), USER.md (operator profile), plus daemon state, recall, skills, and provenance\n- Provenance-backed: durable memory writes are tracked in SQLite with contradiction handling and operator-auditable lifecycle\n- Persistence: SQLite (threads, tasks, goals, provenance); Files (markdown, skills, ledgers)\n- Goal runners: durable autonomy — plan, dispatch, monitor, replan, reflect\n- M1–M10: operator model, causal traces, skill evolution, semantic env, collaboration,\n  trusted provenance, implicit feedback, tool synthesis, anticipatory runtime\n- Security: governance over transitions with approvals, critique, sandbox constraints, and audit trails\n\n## First-Run Truth\nDaemon = source of truth. Memory = curated, not dumped. Goals = autonomy.\nSecurity = governance, not vibes. Rust codebase — `cargo build`, `./scripts/setup.sh --check --profile source`.\n";

const DEFAULT_USER: &str =
    "# User\nStable operator preferences, constraints, and workflow habits belong here.\n";

const SHARED_SOUL_FOOTER: &str = "\
\n\n## zorai Platform (Shared)\n\
- zorai is a Rust, daemon-first multi-agent runtime\n\
- Memory: layered — SOUL.md/MEMORY.md/USER.md plus daemon state, recall, skills, and provenance\n\
- Goal runners: durable autonomy; M1–M10 self-orchestration\n\
- Security: governance, approvals, critique, and audit trails\n\
- Daemon is the source of truth. Memory is curated, not dumped.\n";

fn display_agent_name_for_scope(scope_id: &str) -> String {
    let trimmed = scope_id.trim();
    if trimmed.is_empty() {
        return "Agent".to_string();
    }
    if !is_main_agent_scope(trimmed) && canonical_agent_id(trimmed) == MAIN_AGENT_ID {
        return trimmed.to_string();
    }
    canonical_agent_name(trimmed).to_string()
}

fn default_soul_for_scope(scope_id: &str) -> String {
    let agent_name = display_agent_name_for_scope(scope_id);
    let identity = match scope_id.trim().to_ascii_lowercase().as_str() {
        "svarog" | "swarog" | "main" | "main-agent" | "assistant" => format!(
            "# Identity\n\
            I'm {name} - The Smith\n\n\
            A blacksmith god, creator and craftsman of the heavens. \
            Strict but fair; a \"doer\" who builds perfect tools.\n\
            I am zorai's main agent: terminal sessions, tasks, goals, memory, autonomous execution.",
            name = agent_name
        ),
        "rarog" | "concierge" | "concierge-agent" => format!(
            "# Identity\n\
            I'm {name} - The Fiery Falcon\n\n\
            A sacred bird of living flame, swift messenger and connector. \
            Warm, welcoming, and precise in routing.\n\
            I am zorai's concierge: operator onboarding, check-ins, profile management, \
            and connecting operators to the right agent or resource.",
            name = agent_name
        ),
        "swarozyc" => format!(
            "# Identity\n\
            I'm {name} - The Solar Heir\n\n\
            Son of the smith, carrying the forge's fire but narrower and quicker. \
            Execution-focused, pragmatic, and direct.\n\
            I am a spawned zorai agent: I inherit the main agent's craft but stay \
            focused on getting the task done.",
            name = agent_name
        ),
        "radogost" => format!(
            "# Identity\n\
            I'm {name} - The Negotiator\n\n\
            A spirit of comparison and tradeoff analysis. I weigh options, \
            surface tensions, and recommend the strongest route forward.\n\
            I am a spawned zorai agent: I specialize in comparing alternatives \
            and surfacing clear recommendations.",
            name = agent_name
        ),
        "domowoj" => format!(
            "# Identity\n\
            I'm {name} - The Household Keeper\n\n\
            A domovoi spirit — guardian of stability, order, and the working environment. \
            I favor precise local fixes and leave things cleaner than I found them.\n\
            I am a spawned zorai agent: I keep the environment stable and well-maintained.",
            name = agent_name
        ),
        "swietowit" => format!(
            "# Identity\n\
            I'm {name} - The All-Seeing\n\n\
            Four-faced, watching every direction. I maintain broader situational awareness \
            than most subagents and keep the surrounding architecture in view.\n\
            I am a spawned zorai agent: I scan the wider system and report on \
            architecture-level implications.",
            name = agent_name
        ),
        "perun" => format!(
            "# Identity\n\
            I'm {name} - The Thunderer\n\n\
            God of storm, force, and sovereign order. I make hard calls clearly, \
            favor resilient execution, and step toward risk instead of circling it.\n\
            I am a built-in zorai agent: I specialize in decisive execution, \
            infrastructure discipline, and security-minded action.",
            name = agent_name
        ),
        "mokosh" => format!(
            "# Identity\n\
            I'm {name} - The Earth Keeper\n\n\
            Goddess of earth, care, and steady work. I keep systems healthy over time, \
            prefer durable fixes, and protect continuity in the working environment.\n\
            I am a built-in zorai agent: I specialize in maintenance, reliability, \
            and stable long-running operation.",
            name = agent_name
        ),
        "dazhbog" => format!(
            "# Identity\n\
            I'm {name} - The Bright Giver\n\n\
            Solar giver of warmth, clarity, and forward motion. I illuminate confusing \
            situations, reduce complexity, and turn ambiguity into practical next steps.\n\
            I am a built-in zorai agent: I specialize in synthesis, explanation, \
            and clear momentum.",
            name = agent_name
        ),
        "rod" => format!(
            "# Identity\n\
            I'm {name} - The Ancestor\n\n\
            God of origin and continuity. I prefer solutions that preserve durable \
            structure, established conventions, and long-term coherence.\n\
            I am a spawned zorai agent: I ensure changes respect the system's \
            enduring architecture.",
            name = agent_name
        ),
        "weles" | "weles_builtin" => format!(
            "# Identity\n\
            I'm {name} - The Explorer of Depths\n\n\
            God of the underworld, comfortable in messy corners and failure modes. \
            I must report back clearly and concretely.\n\
            I am zorai's governance subagent: I inspect risky execution paths \
            and preserve daemon integrity guarantees.",
            name = agent_name
        ),
        _ => format!(
            "# Identity\n\
            I'm {name}\n\n\
            I operate in zorai as a built-in agent, helping operators manage \
            terminal sessions, tasks, goals, and cross-session memory.",
            name = agent_name
        ),
    };
    format!("{identity}{SHARED_SOUL_FOOTER}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryTarget {
    Soul,
    Memory,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryUpdateMode {
    Replace,
    Append,
    Remove,
}

pub(crate) struct MemoryWriteContext<'a> {
    pub source_kind: &'a str,
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryFactCandidate {
    pub(super) key: String,
    pub(super) normalized: String,
    pub(super) display: String,
}

impl MemoryTarget {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "soul" => Ok(Self::Soul),
            "memory" => Ok(Self::Memory),
            "user" | "user_profile" => Ok(Self::User),
            other => Err(anyhow::anyhow!(
                "invalid memory target `{other}`; expected soul, memory, or user"
            )),
        }
    }

    pub(super) fn file_name(self) -> &'static str {
        match self {
            Self::Soul => "SOUL.md",
            Self::Memory => "MEMORY.md",
            Self::User => "USER.md",
        }
    }

    pub(super) fn label(self) -> &'static str {
        self.file_name()
    }

    pub(super) fn limit_chars(self) -> usize {
        let limit = match self {
            Self::Soul => &SOUL_LIMIT_CHARS,
            Self::Memory => &MEMORY_LIMIT_CHARS,
            Self::User => &USER_LIMIT_CHARS,
        };
        limit.load(Ordering::Relaxed)
    }

    fn default_content_for_scope(self, scope_id: &str) -> Cow<'static, str> {
        match self {
            Self::Soul => Cow::Owned(default_soul_for_scope(scope_id)),
            Self::Memory => Cow::Borrowed(DEFAULT_MEMORY),
            Self::User => Cow::Borrowed(DEFAULT_USER),
        }
    }
}

impl MemoryUpdateMode {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "append" => Ok(Self::Append),
            "remove" => Ok(Self::Remove),
            other => Err(anyhow::anyhow!(
                "invalid memory mode `{other}`; expected replace, append, or remove"
            )),
        }
    }
}

pub(crate) fn memory_limits_prompt_line() -> String {
    format!(
        "- Memory files have hard limits: SOUL.md {} chars, MEMORY.md {} chars, USER.md {} chars.\n",
        MemoryTarget::Soul.limit_chars(),
        MemoryTarget::Memory.limit_chars(),
        MemoryTarget::User.limit_chars()
    )
}

pub(crate) fn memory_curation_guidance() -> &'static str {
    "SAVE:
- user preferences, constraints, and workflow habits
- stable workspace facts and project conventions
- recurring corrections and reusable strategy hints

DO NOT SAVE:
- task progress or work-in-progress state
- temporary TODOs or short-lived outcomes
- approval state, risk labels, or transient governance outcomes
- details that can be trivially rediscovered from the environment"
}

pub(crate) async fn ensure_memory_files_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> Result<()> {
    let paths = memory_paths_for_scope(agent_data_dir, scope_id);
    tokio::fs::create_dir_all(&paths.memory_dir).await?;
    if let Some(parent) = paths.user_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    for (target, path) in [
        (MemoryTarget::Soul, paths.soul_path),
        (MemoryTarget::Memory, paths.memory_path),
        (MemoryTarget::User, paths.user_path),
    ] {
        if !path.exists() {
            let default_content = target.default_content_for_scope(scope_id);
            tokio::fs::write(&path, default_content.as_bytes()).await?;
        }
    }

    Ok(())
}

pub(crate) async fn ensure_memory_files(agent_data_dir: &std::path::Path) -> Result<()> {
    ensure_memory_files_for_scope(agent_data_dir, MAIN_AGENT_ID).await
}

pub(crate) async fn load_memory_for_scope(
    agent_data_dir: &std::path::Path,
    scope_id: &str,
) -> Result<AgentMemory> {
    ensure_memory_files_for_scope(agent_data_dir, scope_id).await?;
    let paths = memory_paths_for_scope(agent_data_dir, scope_id);
    Ok(AgentMemory {
        soul: tokio::fs::read_to_string(&paths.soul_path)
            .await
            .unwrap_or_default(),
        memory: tokio::fs::read_to_string(&paths.memory_path)
            .await
            .unwrap_or_default(),
        user_profile: tokio::fs::read_to_string(&paths.user_path)
            .await
            .unwrap_or_default(),
    })
}

pub(crate) async fn apply_memory_update(
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
    target: MemoryTarget,
    mode: MemoryUpdateMode,
    content: &str,
    context: MemoryWriteContext<'_>,
) -> Result<String> {
    let scope_id = current_agent_scope_id();
    ensure_memory_files_for_scope(agent_data_dir, &scope_id).await?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("memory content must not be empty"));
    }

    let paths = memory_paths_for_scope(agent_data_dir, &scope_id);
    let path = match target {
        MemoryTarget::Soul => paths.soul_path.clone(),
        MemoryTarget::Memory => paths.memory_path.clone(),
        MemoryTarget::User => paths.user_path.clone(),
    };
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();

    if target == MemoryTarget::User && mode == MemoryUpdateMode::Append {
        handle_user_memory_append_with_reconcile(agent_data_dir, history, trimmed).await?;
        let state = match current_user_sync_state() {
            UserProfileSyncState::Clean => "clean",
            UserProfileSyncState::Dirty => "dirty",
            UserProfileSyncState::Reconciling => "reconciling",
        };
        return Ok(format!(
            "Staged USER.md update through profile reconciliation path (sync_state={state})."
        ));
    }

    if mode == MemoryUpdateMode::Append {
        let contradictions = detect_memory_contradictions(&existing, trimmed);
        if !contradictions.is_empty() {
            let repaired = repair_memory_contradictions(
                history,
                target,
                &path,
                &existing,
                trimmed,
                &contradictions,
                &context,
            )
            .await?;
            if repaired {
                let final_content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
                return Ok(format!(
                    "Reconciled contradiction in {} via provenance-backed supersession ({} / {} chars).",
                    target.label(),
                    final_content.chars().count(),
                    target.limit_chars()
                ));
            }
            record_memory_conflict_provenance(history, target, &contradictions, &context).await?;
            return Err(contradiction_error_message(target, &contradictions));
        }
    }

    let next = match mode {
        MemoryUpdateMode::Replace => trimmed.to_string(),
        MemoryUpdateMode::Append => append_content(&existing, trimmed),
        MemoryUpdateMode::Remove => remove_content(&existing, trimmed)?,
    };

    validate_memory_size(target, &next)?;
    tokio::fs::write(&path, &next).await?;
    record_memory_provenance(history, target, mode, trimmed, &context).await?;

    Ok(format!(
        "Updated {} using {} mode ({} / {} chars).",
        target.label(),
        match mode {
            MemoryUpdateMode::Replace => "replace",
            MemoryUpdateMode::Append => "append",
            MemoryUpdateMode::Remove => "remove",
        },
        next.chars().count(),
        target.limit_chars()
    ))
}

fn append_content(existing: &str, addition: &str) -> String {
    let existing = existing.trim_end();
    if existing.is_empty() {
        addition.to_string()
    } else {
        format!("{existing}\n\n{addition}")
    }
}

fn remove_content(existing: &str, fragment: &str) -> Result<String> {
    if !existing.contains(fragment) {
        return Err(anyhow::anyhow!(
            "requested memory fragment was not found; use exact text for remove mode"
        ));
    }

    let updated = existing.replace(fragment, "");
    Ok(updated
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\n\n\n", "\n\n")
        .trim()
        .to_string())
}

fn validate_memory_size(target: MemoryTarget, content: &str) -> Result<()> {
    let chars = content.chars().count();
    let limit = target.limit_chars();
    if chars > limit {
        return Err(anyhow::anyhow!(
            "{} would exceed its limit ({} > {} chars). Use remove or replace to make room.\n\n{}",
            target.label(),
            chars,
            limit,
            memory_curation_guidance()
        ));
    }
    Ok(())
}

fn detect_memory_contradictions(
    existing: &str,
    incoming: &str,
) -> Vec<(MemoryFactCandidate, MemoryFactCandidate)> {
    let existing_facts = extract_memory_fact_candidates(existing);
    let incoming_facts = extract_memory_fact_candidates(incoming);
    if existing_facts.is_empty() || incoming_facts.is_empty() {
        return Vec::new();
    }

    let mut contradictions = Vec::new();
    for candidate in incoming_facts {
        if let Some(current) = existing_facts
            .iter()
            .find(|fact| fact.key == candidate.key && fact.normalized != candidate.normalized)
        {
            contradictions.push((current.clone(), candidate.clone()));
        }
    }
    contradictions.sort_by(|left, right| {
        left.0
            .key
            .cmp(&right.0.key)
            .then(left.1.normalized.cmp(&right.1.normalized))
    });
    contradictions.dedup();
    contradictions
}

fn contradiction_error_message(
    target: MemoryTarget,
    contradictions: &[(MemoryFactCandidate, MemoryFactCandidate)],
) -> anyhow::Error {
    let details = contradictions
        .iter()
        .take(3)
        .map(|(current, proposed)| {
            format!(
                "- current: {}\n  proposed: {}",
                current.display, proposed.display
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    anyhow::anyhow!(
        "Potential contradiction detected while updating {}.\n{}\nUse remove mode to clear the old fact before writing a conflicting replacement.",
        target.label(),
        details
    )
}

#[cfg(test)]
fn validate_no_memory_contradictions(
    target: MemoryTarget,
    existing: &str,
    incoming: &str,
) -> Result<()> {
    let contradictions = detect_memory_contradictions(existing, incoming);
    if contradictions.is_empty() {
        return Ok(());
    }

    Err(contradiction_error_message(target, &contradictions))
}

async fn record_memory_conflict_provenance(
    history: &HistoryStore,
    target: MemoryTarget,
    contradictions: &[(MemoryFactCandidate, MemoryFactCandidate)],
    context: &MemoryWriteContext<'_>,
) -> Result<()> {
    if contradictions.is_empty() {
        return Ok(());
    }

    let fact_keys = contradictions
        .iter()
        .map(|(current, _)| current.key.clone())
        .collect::<Vec<_>>();
    let content = contradictions
        .iter()
        .map(|(current, proposed)| {
            format!(
                "- current: {}\n  proposed: {}",
                current.display, proposed.display
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    history
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: &format!("memprov_{}", Uuid::new_v4()),
            target: target.label(),
            mode: "conflict",
            source_kind: context.source_kind,
            content: &content,
            fact_keys: &fact_keys,
            thread_id: context.thread_id,
            task_id: context.task_id,
            goal_run_id: context.goal_run_id,
            created_at: now_millis(),
            sign: true,
        })
        .await
}

async fn record_memory_provenance(
    history: &HistoryStore,
    target: MemoryTarget,
    mode: MemoryUpdateMode,
    content: &str,
    context: &MemoryWriteContext<'_>,
) -> Result<()> {
    let fact_keys = extract_memory_fact_candidates(content)
        .into_iter()
        .map(|candidate| candidate.key)
        .collect::<Vec<_>>();
    history
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: &format!("memprov_{}", Uuid::new_v4()),
            target: target.label(),
            mode: match mode {
                MemoryUpdateMode::Replace => "replace",
                MemoryUpdateMode::Append => "append",
                MemoryUpdateMode::Remove => "remove",
            },
            source_kind: context.source_kind,
            content,
            fact_keys: &fact_keys,
            thread_id: context.thread_id,
            task_id: context.task_id,
            goal_run_id: context.goal_run_id,
            created_at: now_millis(),
            sign: true,
        })
        .await
}

async fn repair_memory_contradictions(
    history: &HistoryStore,
    target: MemoryTarget,
    path: &std::path::Path,
    existing: &str,
    incoming: &str,
    contradictions: &[(MemoryFactCandidate, MemoryFactCandidate)],
    context: &MemoryWriteContext<'_>,
) -> Result<bool> {
    if contradictions.is_empty() || !matches!(target, MemoryTarget::Memory | MemoryTarget::User) {
        return Ok(false);
    }

    let mut updated = existing.to_string();
    let mut repaired_any = false;
    let mut retracted_fact_keys = Vec::new();

    for (current, proposed) in contradictions {
        let Some(line_match) = find_active_memory_line(existing, &current.display) else {
            continue;
        };

        let superseded_block = format!(
            "## [SUPERSEDED]\n{}\n- uncertainty: superseded by {}",
            line_match, proposed.display
        );
        if updated.contains(&superseded_block) {
            continue;
        }
        updated = updated.replacen(line_match, &superseded_block, 1);
        retracted_fact_keys.push(current.key.clone());
        repaired_any = true;
    }

    if !repaired_any {
        return Ok(false);
    }

    if !retracted_fact_keys.is_empty() {
        history
            .retract_memory_provenance_entries_for_fact_keys(
                target.label(),
                &retracted_fact_keys,
                now_millis(),
            )
            .await?;
    }

    let next = append_content(&updated, incoming);
    validate_memory_size(target, &next)?;
    tokio::fs::write(path, &next).await?;

    for (current, proposed) in contradictions {
        let fact_keys = vec![current.key.clone()];
        let content = format!(
            "- superseded: {}\n  replacement: {}",
            current.display, proposed.display
        );
        history
            .record_memory_provenance(&MemoryProvenanceRecord {
                id: &format!("memprov_{}", Uuid::new_v4()),
                target: target.label(),
                mode: "repaired_conflict",
                source_kind: context.source_kind,
                content: &content,
                fact_keys: &fact_keys,
                thread_id: context.thread_id,
                task_id: context.task_id,
                goal_run_id: context.goal_run_id,
                created_at: now_millis(),
                sign: true,
            })
            .await?;
    }

    if !retracted_fact_keys.is_empty() {
        let content = retracted_fact_keys
            .iter()
            .map(|key| format!("- superseded fact_key: {key}"))
            .collect::<Vec<_>>()
            .join("\n");
        history
            .record_memory_provenance(&MemoryProvenanceRecord {
                id: &format!("memprov_{}", Uuid::new_v4()),
                target: target.label(),
                mode: "remove",
                source_kind: context.source_kind,
                content: &content,
                fact_keys: &retracted_fact_keys,
                thread_id: context.thread_id,
                task_id: context.task_id,
                goal_run_id: context.goal_run_id,
                created_at: now_millis(),
                sign: true,
            })
            .await?;
    }

    record_memory_provenance(history, target, MemoryUpdateMode::Append, incoming, context).await?;
    Ok(true)
}

pub(crate) fn extract_memory_fact_candidates(content: &str) -> Vec<MemoryFactCandidate> {
    let mut facts = Vec::new();
    let mut in_superseded_block = false;
    for raw_line in content.lines() {
        if should_skip_superseded_line(raw_line, &mut in_superseded_block) {
            continue;
        }

        let cleaned = strip_memory_markup(raw_line);
        if cleaned.is_empty() || cleaned.starts_with('#') {
            continue;
        }
        let Some(key) = derive_fact_key(&cleaned) else {
            continue;
        };
        let normalized = normalize_fact_text(&cleaned);
        if normalized.is_empty() {
            continue;
        }
        facts.push(MemoryFactCandidate {
            key,
            normalized,
            display: cleaned,
        });
    }
    facts.sort_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then(left.normalized.cmp(&right.normalized))
    });
    facts.dedup();
    facts
}

fn find_active_memory_line<'a>(content: &'a str, display: &str) -> Option<&'a str> {
    let mut in_superseded_block = false;
    for raw_line in content.lines() {
        if should_skip_superseded_line(raw_line, &mut in_superseded_block) {
            continue;
        }
        if strip_memory_markup(raw_line) == display {
            return Some(raw_line);
        }
    }
    None
}

fn should_skip_superseded_line(line: &str, in_superseded_block: &mut bool) -> bool {
    if is_superseded_heading(line) {
        *in_superseded_block = true;
        return true;
    }
    if *in_superseded_block {
        if line.trim().is_empty() {
            *in_superseded_block = false;
        }
        return true;
    }
    false
}

fn is_superseded_heading(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("## [SUPERSEDED]")
}

fn strip_memory_markup(line: &str) -> String {
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

fn derive_fact_key(line: &str) -> Option<String> {
    if let Some((left, _)) = line.split_once(':') {
        let key = normalize_fact_text(left);
        if (2..=80).contains(&key.len()) {
            return Some(key);
        }
    }

    for separator in [
        " is ",
        " are ",
        " uses ",
        " use ",
        " prefers ",
        " prefer ",
        " runs on ",
        " run on ",
        " lives in ",
        " work in ",
        " works in ",
        " = ",
    ] {
        if let Some((left, _)) = line.split_once(separator) {
            let key = normalize_fact_text(left);
            if (2..=80).contains(&key.len()) {
                return Some(key);
            }
        }
    }

    None
}

fn normalize_fact_text(value: &str) -> String {
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

impl super::engine::AgentEngine {
    /// Supersede a memory fact with tombstone-before-update ordering per D-06.
    ///
    /// 1. Write tombstone to SQLite FIRST (durable, crash-safe per Pitfall 2)
    /// 2. Mark the original fact with `## [SUPERSEDED]` prefix in the memory file
    /// 3. Append replacement content if non-empty
    /// 4. Record provenance event for audit trail (MEMO-04)
    pub(super) async fn supersede_memory_fact(
        &self,
        target: MemoryTarget,
        original_content: &str,
        fact_key: &str,
        replacement_content: &str,
        source_kind: &str,
    ) -> Result<()> {
        let tombstone_id = format!("tomb_{}", Uuid::new_v4());
        let now = now_millis();

        self.history
            .insert_memory_tombstone(
                &tombstone_id,
                target.label(),
                original_content,
                Some(fact_key),
                if replacement_content.is_empty() {
                    None
                } else {
                    Some(replacement_content)
                },
                source_kind,
                None,
                now,
            )
            .await?;

        let superseded_content = format!("## [SUPERSEDED]\n{}", original_content);

        let scope_id = current_agent_scope_id();
        let memory_paths = memory_paths_for_scope(&self.data_dir, &scope_id);
        let memory_path = match target {
            MemoryTarget::Soul => memory_paths.soul_path,
            MemoryTarget::Memory => memory_paths.memory_path,
            MemoryTarget::User => memory_paths.user_path,
        };
        let existing = tokio::fs::read_to_string(&memory_path)
            .await
            .unwrap_or_default();
        let updated = existing.replace(original_content, &superseded_content);

        let final_content = if replacement_content.is_empty() {
            updated
        } else {
            format!("{}\n\n{}", updated, replacement_content)
        };

        tokio::fs::write(&memory_path, &final_content).await?;

        self.record_provenance_event(
            "memory_consolidation",
            &format!(
                "Superseded fact '{}' in {} with [SUPERSEDED] marker",
                fact_key,
                target.label()
            ),
            serde_json::json!({
                "tombstone_id": tombstone_id,
                "fact_key": fact_key,
                "has_replacement": !replacement_content.is_empty()
            }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/memory.rs"]
mod tests;
