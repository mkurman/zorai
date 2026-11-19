use super::*;
use crate::agent::context::structural_memory::ThreadStructuralMemory;
use crate::agent::handoff::EpisodeRef;

const RESONANCE_CACHE_TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct ResonanceContextSnapshot {
    pub query: String,
    pub episodic_refs: Vec<EpisodeRef>,
    pub negative_constraints: Vec<String>,
    pub collaboration_context: Vec<serde_json::Value>,
    pub structural_memory: Option<ThreadStructuralMemory>,
    pub source_thread_id: Option<String>,
    pub source_task_id: Option<String>,
    pub created_at_ms: u64,
    pub last_accessed_at_ms: u64,
    pub hit_count: u64,
}

impl AgentEngine {
    pub(crate) async fn get_resonance_context_snapshot(
        &self,
        query: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Option<ResonanceContextSnapshot> {
        let now = now_millis();
        let key = resonance_cache_key(query, thread_id, task_id);
        let mut cache = self.resonance_context_cache.write().await;

        if let Some(entry) = cache.get_mut(&key) {
            if now.saturating_sub(entry.created_at_ms) > RESONANCE_CACHE_TTL_MS {
                cache.remove(&key);
            } else {
                entry.last_accessed_at_ms = now;
                entry.hit_count = entry.hit_count.saturating_add(1);
                return Some(entry.clone());
            }
        }

        let normalized_query = normalize_resonance_query(query);
        let normalized_thread = thread_id.unwrap_or("-");
        let related_key = cache.iter().find_map(|(candidate_key, entry)| {
            if now.saturating_sub(entry.created_at_ms) > RESONANCE_CACHE_TTL_MS {
                return None;
            }
            if normalize_resonance_query(&entry.query) != normalized_query {
                return None;
            }
            if entry.source_thread_id.as_deref().unwrap_or("-") != normalized_thread {
                return None;
            }
            if task_id.is_some() && entry.source_task_id.as_deref() == task_id {
                return None;
            }
            Some(candidate_key.clone())
        });

        if let Some(related_key) = related_key {
            if let Some(entry) = cache.get_mut(&related_key) {
                entry.last_accessed_at_ms = now;
                entry.hit_count = entry.hit_count.saturating_add(1);
                return Some(entry.clone());
            }
        }

        cache.retain(|_, value| now.saturating_sub(value.created_at_ms) <= RESONANCE_CACHE_TTL_MS);
        None
    }

    pub(crate) async fn put_resonance_context_snapshot(
        &self,
        query: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        snapshot: ResonanceContextSnapshot,
    ) {
        let now = now_millis();
        let key = resonance_cache_key(query, thread_id, task_id);
        let mut cache = self.resonance_context_cache.write().await;
        cache.retain(|_, value| now.saturating_sub(value.created_at_ms) <= RESONANCE_CACHE_TTL_MS);
        cache.insert(key, snapshot);
    }

    pub(crate) async fn build_resonance_context_snapshot(
        &self,
        query: &str,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        episodic_limit: usize,
    ) -> ResonanceContextSnapshot {
        let now = now_millis();
        let episodic_refs = match self.retrieve_relevant_episodes(query, episodic_limit).await {
            Ok(episodes) => episodes
                .into_iter()
                .map(|ep| EpisodeRef {
                    episode_id: ep.id,
                    summary: ep.summary,
                    outcome: format!("{:?}", ep.outcome),
                })
                .collect::<Vec<_>>(),
            Err(error) => {
                tracing::warn!(%error, "resonance: failed to retrieve episodes");
                Vec::new()
            }
        };
        let negative_constraints = match self.query_active_constraints(Some(query)).await {
            Ok(constraints) => constraints
                .into_iter()
                .map(|constraint| constraint.description)
                .collect::<Vec<_>>(),
            Err(error) => {
                tracing::warn!(%error, "resonance: failed to query constraints");
                Vec::new()
            }
        };
        let collaboration_context = if let Some(task_id) = task_id {
            let parent_task_id = match self
                .history
                .agent_task_goal_context(task_id)
                .await
                .ok()
                .flatten()
                .and_then(|task| task.parent_task_id)
            {
                Some(parent_task_id) => Some(parent_task_id),
                None => {
                    let tasks = self.tasks.lock().await;
                    tasks
                        .iter()
                        .find(|task| task.id == task_id)
                        .and_then(|task| task.parent_task_id.clone())
                }
            };
            if let Some(parent_task_id) = parent_task_id {
                let collaboration = self.collaboration.read().await;
                collaboration
                    .get(&parent_task_id)
                    .map(|session| {
                        session
                            .contributions
                            .iter()
                            .filter(|entry| entry.task_id != task_id)
                            .filter_map(|entry| serde_json::to_value(entry).ok())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let structural_memory = if let Some(thread_id) = thread_id {
            self.get_thread_structural_memory(thread_id).await
        } else {
            None
        };

        ResonanceContextSnapshot {
            query: query.to_string(),
            episodic_refs,
            negative_constraints,
            collaboration_context,
            structural_memory,
            source_thread_id: thread_id.map(str::to_string),
            source_task_id: task_id.map(str::to_string),
            created_at_ms: now,
            last_accessed_at_ms: now,
            hit_count: 0,
        }
    }
}

fn resonance_cache_key(query: &str, thread_id: Option<&str>, task_id: Option<&str>) -> String {
    format!(
        "{}::{}::{}",
        normalize_resonance_query(query),
        thread_id.unwrap_or("-"),
        task_id.unwrap_or("-")
    )
}

fn normalize_resonance_query(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}
