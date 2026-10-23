use super::*;
use crate::history::ThreadSearchCandidate;

const THREAD_SEARCH_RRF_K: f64 = 60.0;
const THREAD_SEARCH_SEMANTIC_MESSAGES: usize = 80;

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ThreadSearchResult {
    pub thread_id: String,
    pub title: String,
    pub updated_at: i64,
    pub snippet: Option<String>,
    pub matched: Vec<&'static str>,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ThreadSearchResponse {
    pub query: String,
    pub semantic: bool,
    pub semantic_error: Option<String>,
    pub results: Vec<ThreadSearchResult>,
}

pub(super) fn fuse_thread_search_results(
    sources: Vec<(&'static str, Vec<ThreadSearchCandidate>)>,
    limit: usize,
) -> Vec<ThreadSearchResult> {
    let mut fused: Vec<ThreadSearchResult> = Vec::new();
    for (source, candidates) in sources {
        for (rank, candidate) in candidates.into_iter().enumerate() {
            let score = 1.0 / (THREAD_SEARCH_RRF_K + rank as f64 + 1.0);
            match fused
                .iter_mut()
                .find(|result| result.thread_id == candidate.thread_id)
            {
                Some(result) => {
                    result.score += score;
                    result.matched.push(source);
                    if result.snippet.is_none() {
                        result.snippet = candidate.snippet;
                    }
                }
                None => fused.push(ThreadSearchResult {
                    thread_id: candidate.thread_id,
                    title: candidate.title,
                    updated_at: candidate.updated_at,
                    snippet: candidate.snippet,
                    matched: vec![source],
                    score,
                }),
            }
        }
    }
    fused.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then(right.updated_at.cmp(&left.updated_at))
    });
    fused.truncate(limit);
    fused
}

fn thread_search_hidden_prefixes() -> Vec<String> {
    vec![
        crate::agent::agent_identity::PARTICIPANT_PLAYGROUND_THREAD_PREFIX.to_string(),
        crate::agent::thread_handoffs::INTERNAL_HANDOFF_THREAD_PREFIX.to_string(),
    ]
}

impl AgentEngine {
    pub async fn search_threads(&self, query: &str, limit: usize) -> Result<ThreadSearchResponse> {
        let query = query.trim().to_string();
        let hidden = thread_search_hidden_prefixes();
        let titles = self
            .history
            .search_thread_titles(&query, limit, &hidden)
            .await?;
        let content = self
            .history
            .search_thread_messages(&query, limit, &hidden)
            .await?;
        let config = self.config.read().await.clone();
        let (semantic, semantic_error, semantic_hits) = match self
            .semantic_message_ids(&query, THREAD_SEARCH_SEMANTIC_MESSAGES, &config)
            .await
        {
            Ok(None) => (false, None, Vec::new()),
            Ok(Some(message_ids)) => (
                true,
                None,
                self.history
                    .thread_search_candidates_for_messages(&message_ids, &hidden)
                    .await?,
            ),
            Err(error) => {
                tracing::warn!(%error, "thread search: semantic lookup failed; using keyword results only");
                (false, Some(error.to_string()), Vec::new())
            }
        };
        let results = fuse_thread_search_results(
            vec![
                ("title", titles),
                ("content", content),
                ("semantic", semantic_hits),
            ],
            limit,
        );
        Ok(ThreadSearchResponse {
            query,
            semantic,
            semantic_error,
            results,
        })
    }
}

#[cfg(test)]
#[path = "tests/thread_search.rs"]
mod tests;
