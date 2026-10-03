use super::super::{now_millis, task_by_id_for_tool_scope, AgentEngine, Result};
use super::AskParentRecord;
use crate::agent::types::AgentTask;

const TASK_SLUG_STATE_PREFIX: &str = "task_slug:";
const TASK_SLUG_INDEX_PREFIX: &str = "task_slug_index:";
const MAX_SLUG_LEN: usize = 32;

pub(crate) fn slugify_handle(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_hyphen = false;
    for ch in raw.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_hyphen = false;
        } else if !prev_hyphen && !out.is_empty() {
            out.push('-');
            prev_hyphen = true;
        }
        if out.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "task".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn next_ask_slug(records: &[(String, AskParentRecord)]) -> String {
    let max = records
        .iter()
        .filter_map(|(_, record)| parse_ask_slug(&record.slug))
        .max()
        .unwrap_or(0);
    format!("q{}", max + 1)
}

fn parse_ask_slug(slug: &str) -> Option<u32> {
    let rest = slug.strip_prefix('q')?;
    if rest.is_empty() || !rest.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

pub(super) fn ask_ref_matches(key: &str, record: &AskParentRecord, requested: &str) -> bool {
    if super::split_ask_key(key).is_some_and(|(_, id)| id == requested) {
        return true;
    }
    !record.slug.is_empty() && record.slug.eq_ignore_ascii_case(requested.trim())
}

pub(crate) async fn ensure_task_slug(agent: &AgentEngine, task: &AgentTask) -> Result<String> {
    let stored_key = format!("{TASK_SLUG_STATE_PREFIX}{}", task.id);
    if let Some(existing) = agent.history.get_consolidation_state(&stored_key).await? {
        let existing = existing.trim();
        if !existing.is_empty() {
            index_task_slug(agent, task, existing).await?;
            return Ok(existing.to_string());
        }
    }
    let base = slugify_handle(&task.title);
    let slug = claim_unique_slug(agent, task, &base).await?;
    let now = now_millis();
    agent
        .history
        .set_consolidation_state(&stored_key, &slug, now)
        .await?;
    for scope in scope_keys(task) {
        let index_key = format!("{TASK_SLUG_INDEX_PREFIX}{scope}:{slug}");
        agent
            .history
            .set_consolidation_state(&index_key, &task.id, now)
            .await?;
    }
    Ok(slug)
}

pub(crate) async fn resolve_child_reference(
    agent: &AgentEngine,
    raw: &str,
    caller_thread_id: &str,
    caller_task_id: Option<&str>,
) -> Result<AgentTask> {
    let raw = raw.trim();
    if raw.is_empty() {
        anyhow::bail!("missing child task reference");
    }
    if let Some(task) = task_by_id_for_tool_scope(agent, raw).await {
        return Ok(task);
    }
    let mut candidates = vec![raw.to_ascii_lowercase()];
    let folded = slugify_handle(raw);
    if folded != candidates[0] {
        candidates.push(folded);
    }
    let mut found: Option<String> = None;
    for candidate in candidates {
        for scope in caller_scope_keys(caller_thread_id, caller_task_id) {
            let key = format!("{TASK_SLUG_INDEX_PREFIX}{scope}:{candidate}");
            let Some(task_id) = agent.history.get_consolidation_state(&key).await? else {
                continue;
            };
            let task_id = task_id.trim();
            if task_id.is_empty() {
                continue;
            }
            if let Some(previous) = found.as_deref() {
                if previous != task_id {
                    anyhow::bail!(
                        "child handle `{raw}` matches more than one task; pass the full task id"
                    );
                }
            } else {
                found = Some(task_id.to_string());
            }
        }
        if found.is_some() {
            break;
        }
    }
    let Some(task_id) = found else {
        anyhow::bail!("child task {raw} not found");
    };
    task_by_id_for_tool_scope(agent, &task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("child task {raw} not found"))
}

fn scope_keys(task: &AgentTask) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(id) = nonempty(task.parent_task_id.as_deref()) {
        keys.push(format!("task:{id}"));
    }
    if let Some(id) = nonempty(task.parent_thread_id.as_deref()) {
        keys.push(format!("thread:{id}"));
    }
    if let Some(id) = nonempty(task.thread_id.as_deref()) {
        let scope = format!("thread:{id}");
        if !keys.contains(&scope) {
            keys.push(scope);
        }
    }
    if keys.is_empty() {
        keys.push(format!("task:{}", task.id));
    }
    keys
}

async fn index_task_slug(agent: &AgentEngine, task: &AgentTask, slug: &str) -> Result<()> {
    let now = now_millis();
    for scope in scope_keys(task) {
        let key = format!("{TASK_SLUG_INDEX_PREFIX}{scope}:{slug}");
        if let Some(owner) = agent.history.get_consolidation_state(&key).await? {
            let owner = owner.trim();
            if !owner.is_empty() && owner != task.id {
                continue;
            }
        }
        agent
            .history
            .set_consolidation_state(&key, &task.id, now)
            .await?;
    }
    Ok(())
}

pub(crate) async fn stored_task_slug(agent: &AgentEngine, task_id: &str) -> Option<String> {
    let key = format!("{TASK_SLUG_STATE_PREFIX}{task_id}");
    agent
        .history
        .get_consolidation_state(&key)
        .await
        .ok()
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn caller_scope_keys(caller_thread_id: &str, caller_task_id: Option<&str>) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(id) = caller_task_id.map(str::trim).filter(|id| !id.is_empty()) {
        keys.push(format!("task:{id}"));
    }
    if let Some(id) = nonempty(Some(caller_thread_id)) {
        keys.push(format!("thread:{id}"));
    }
    keys
}

async fn claim_unique_slug(agent: &AgentEngine, task: &AgentTask, base: &str) -> Result<String> {
    for index in 0..64 {
        let candidate = if index == 0 {
            base.to_string()
        } else {
            format!("{base}-{}", index + 1)
        };
        if slug_available(agent, task, &candidate).await? {
            return Ok(candidate);
        }
    }
    Ok(format!(
        "{base}-{}",
        &task.id[task.id.len().saturating_sub(6)..]
    ))
}

async fn slug_available(agent: &AgentEngine, task: &AgentTask, slug: &str) -> Result<bool> {
    for scope in scope_keys(task) {
        let key = format!("{TASK_SLUG_INDEX_PREFIX}{scope}:{slug}");
        if let Some(owner) = agent.history.get_consolidation_state(&key).await? {
            let owner = owner.trim();
            if !owner.is_empty() && owner != task.id {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::slugify_handle;

    #[test]
    fn slugify_handle_keeps_a_short_ascii_label() {
        assert_eq!(slugify_handle("Researcher"), "researcher");
        assert_eq!(
            slugify_handle("SEPIQ structural candidate"),
            "sepiq-structural-candidate"
        );
        assert_eq!(slugify_handle("  "), "task");
        assert_eq!(slugify_handle(&"a".repeat(80)).len(), 32);
    }
}
