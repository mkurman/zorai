use super::super::{now_millis, AgentEngine, Result};
use super::handles::slugify_handle;

const MODEL_HANDLE_STATE_PREFIX: &str = "model_handle:";
const MODEL_HANDLE_INDEX_PREFIX: &str = "model_handle_index:";

pub(crate) async fn ensure_goal_slug(
    agent: &AgentEngine,
    goal_id: &str,
    title: &str,
    thread_id: Option<&str>,
) -> Result<String> {
    ensure_scoped_handle(
        agent,
        "goal",
        goal_id,
        &goal_scopes(thread_id),
        &slugify_handle(title),
    )
    .await
}

pub(crate) async fn resolve_goal_id(
    agent: &AgentEngine,
    raw: &str,
    thread_id: Option<&str>,
) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        anyhow::bail!("missing goal handle");
    }
    if agent.get_goal_run(raw).await.is_some() {
        return Ok(raw.to_string());
    }
    lookup_scoped_handle(agent, "goal", raw, &goal_scopes(thread_id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("goal {raw} not found"))
}

pub(crate) async fn ensure_operation_slug(
    agent: &AgentEngine,
    thread_id: &str,
    operation_id: &str,
) -> Result<String> {
    ensure_numbered_handle(agent, "op", operation_id, thread_id, "op").await
}

pub(crate) async fn lookup_operation_id(
    agent: &AgentEngine,
    thread_id: &str,
    raw: &str,
) -> Result<Option<String>> {
    lookup_scoped_handle(agent, "op", raw, &[format!("thread:{}", thread_id.trim())]).await
}

pub(crate) async fn ensure_payload_slug(
    agent: &AgentEngine,
    thread_id: &str,
    payload_id: &str,
) -> Result<String> {
    ensure_numbered_handle(agent, "payload", payload_id, thread_id, "p").await
}

pub(crate) async fn resolve_payload_id(
    agent: &AgentEngine,
    thread_id: &str,
    raw: &str,
) -> Result<String> {
    let raw = raw.trim();
    if agent
        .history
        .get_offloaded_payload_metadata(raw)
        .await?
        .is_some()
    {
        return Ok(raw.to_string());
    }
    lookup_scoped_handle(
        agent,
        "payload",
        raw,
        &[format!("thread:{}", thread_id.trim())],
    )
    .await?
    .ok_or_else(|| anyhow::anyhow!("offloaded payload not found"))
}

pub(crate) async fn model_operation_id(
    agent: &AgentEngine,
    thread_id: &str,
    operation_id: &str,
) -> String {
    ensure_operation_slug(agent, thread_id, operation_id)
        .await
        .unwrap_or_else(|_| operation_id.to_string())
}

fn goal_scopes(thread_id: Option<&str>) -> Vec<String> {
    let mut scopes = vec!["goal".to_string()];
    if let Some(thread_id) = thread_id.map(str::trim).filter(|value| !value.is_empty()) {
        scopes.push(format!("thread:{thread_id}"));
    }
    scopes
}

async fn ensure_numbered_handle(
    agent: &AgentEngine,
    kind: &str,
    id: &str,
    thread_id: &str,
    prefix: &str,
) -> Result<String> {
    let scopes = vec![format!("thread:{}", thread_id.trim())];
    if let Some(existing) = stored_model_handle(agent, kind, id).await? {
        return Ok(existing);
    }
    let counter_key = format!("model_handle_counter:{kind}:{}", thread_id.trim());
    for _ in 0..64 {
        let next = next_counter(agent, &counter_key).await?;
        let candidate = format!("{prefix}{next}");
        if scoped_slug_available(agent, kind, &scopes, id, &candidate).await? {
            persist_model_handle(agent, kind, id, &scopes, &candidate).await?;
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not allocate a short handle for {kind} {id}")
}

async fn ensure_scoped_handle(
    agent: &AgentEngine,
    kind: &str,
    id: &str,
    scopes: &[String],
    base: &str,
) -> Result<String> {
    if let Some(existing) = stored_model_handle(agent, kind, id).await? {
        return Ok(existing);
    }
    for index in 0..64 {
        let candidate = if index == 0 {
            base.to_string()
        } else {
            format!("{base}-{}", index + 1)
        };
        if scoped_slug_available(agent, kind, scopes, id, &candidate).await? {
            persist_model_handle(agent, kind, id, scopes, &candidate).await?;
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not allocate a short handle for {kind} {id}")
}

async fn lookup_scoped_handle(
    agent: &AgentEngine,
    kind: &str,
    raw: &str,
    scopes: &[String],
) -> Result<Option<String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let mut candidates = vec![raw.to_ascii_lowercase()];
    let folded = slugify_handle(raw);
    if folded != candidates[0] {
        candidates.push(folded);
    }
    let mut found: Option<String> = None;
    for candidate in candidates {
        for scope in scopes {
            let key = format!("{MODEL_HANDLE_INDEX_PREFIX}{kind}:{scope}:{candidate}");
            let Some(id) = agent.history.get_consolidation_state(&key).await? else {
                continue;
            };
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            if let Some(previous) = found.as_deref() {
                if previous != id {
                    anyhow::bail!("handle `{raw}` matches more than one {kind}; pass the full id");
                }
            } else {
                found = Some(id.to_string());
            }
        }
        if found.is_some() {
            break;
        }
    }
    Ok(found)
}

async fn stored_model_handle(agent: &AgentEngine, kind: &str, id: &str) -> Result<Option<String>> {
    let key = format!("{MODEL_HANDLE_STATE_PREFIX}{kind}:{id}");
    Ok(agent
        .history
        .get_consolidation_state(&key)
        .await?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

async fn persist_model_handle(
    agent: &AgentEngine,
    kind: &str,
    id: &str,
    scopes: &[String],
    slug: &str,
) -> Result<()> {
    let now = now_millis();
    let state_key = format!("{MODEL_HANDLE_STATE_PREFIX}{kind}:{id}");
    agent
        .history
        .set_consolidation_state(&state_key, slug, now)
        .await?;
    for scope in scopes {
        let key = format!("{MODEL_HANDLE_INDEX_PREFIX}{kind}:{scope}:{slug}");
        agent.history.set_consolidation_state(&key, id, now).await?;
    }
    Ok(())
}

async fn scoped_slug_available(
    agent: &AgentEngine,
    kind: &str,
    scopes: &[String],
    id: &str,
    slug: &str,
) -> Result<bool> {
    for scope in scopes {
        let key = format!("{MODEL_HANDLE_INDEX_PREFIX}{kind}:{scope}:{slug}");
        if let Some(owner) = agent.history.get_consolidation_state(&key).await? {
            let owner = owner.trim();
            if !owner.is_empty() && owner != id {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

async fn next_counter(agent: &AgentEngine, key: &str) -> Result<u32> {
    let current = agent
        .history
        .get_consolidation_state(key)
        .await?
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let next = current.saturating_add(1).max(1);
    agent
        .history
        .set_consolidation_state(key, &next.to_string(), now_millis())
        .await?;
    Ok(next)
}
