//! Episodic recall over threads, transcripts, telemetry logs, and agent events.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::*;

const MAX_THREAD_SCAN: usize = 40;
const MAX_SNIPPETS_PER_GROUP: usize = 3;
const MAX_TELEMETRY_SCAN_LINES: usize = 400;
const MAX_EVENT_SCAN: usize = 200;

#[derive(Debug, Clone)]
struct RecallGroup {
    source: &'static str,
    id: String,
    title: String,
    timestamp: u64,
    score: usize,
    lineage: Option<String>,
    summary: String,
    snippets: Vec<String>,
}

#[derive(Debug, Clone)]
struct ThreadRecallAccumulator {
    title: String,
    timestamp: u64,
    message_count: u32,
    lineage: Option<String>,
    score: usize,
    role_hits: Vec<String>,
    snippets: Vec<String>,
}

pub(crate) async fn execute_session_search(
    session_manager: &Arc<SessionManager>,
    query: &str,
    limit: usize,
) -> Result<String> {
    let tokens = query_tokens(query);
    if tokens.is_empty() {
        anyhow::bail!("query must include at least one searchable token");
    }

    let mut groups = Vec::new();
    groups.extend(recall_from_threads(session_manager, &tokens).await?);
    groups.extend(recall_from_transcripts(session_manager, &tokens).await?);
    groups.extend(recall_from_agent_events(session_manager, &tokens).await?);
    groups.extend(recall_from_telemetry("cognitive", &tokens)?);
    groups.extend(recall_from_telemetry("operational", &tokens)?);

    groups.sort_by_key(|group| (Reverse(group.score), Reverse(group.timestamp)));
    groups.truncate(limit.max(1));

    if groups.is_empty() {
        return Ok(format!(
            "No matching prior sessions, transcripts, cognitive traces, or operational logs for \"{query}\"."
        ));
    }

    let mut lines = Vec::new();
    lines.push(format!("Session search for \"{query}\""));
    lines.push(format!(
        "Found {} grouped recall result(s) across threads, transcripts, telemetry, and behavioral events.",
        groups.len()
    ));
    lines.push(String::new());
    for (index, group) in groups.iter().enumerate() {
        lines.push(format!(
            "{}. [{}] {} | id={} | ts={} | score={}",
            index + 1,
            group.source,
            group.title,
            group.id,
            group.timestamp,
            group.score
        ));
        if let Some(lineage) = group.lineage.as_deref().filter(|value| !value.is_empty()) {
            lines.push(format!("   Lineage: {lineage}"));
        }
        lines.push(format!("   Summary: {}", group.summary));
        for snippet in &group.snippets {
            lines.push(format!("   - {snippet}"));
        }
    }

    Ok(lines.join("\n"))
}

async fn recall_from_threads(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let rows = session_manager
        .thread_recall_match_rows(tokens, MAX_THREAD_SCAN)
        .await?;
    let mut order = Vec::new();
    let mut accumulators = BTreeMap::<String, ThreadRecallAccumulator>::new();

    for row in rows {
        if !accumulators.contains_key(&row.thread_id) {
            order.push(row.thread_id.clone());
        }
        let accumulator = accumulators
            .entry(row.thread_id.clone())
            .or_insert_with(|| ThreadRecallAccumulator {
                title: row.title.clone(),
                timestamp: row.updated_at,
                message_count: row.message_count,
                lineage: parse_thread_metadata(row.metadata_json.as_deref()).upstream_thread_id,
                score: match_score(&row.title, tokens),
                role_hits: Vec::new(),
                snippets: Vec::new(),
            });
        let Some(content) = row.message_content.as_deref() else {
            continue;
        };
        let content_score = match_score(content, tokens);
        if content_score == 0 {
            continue;
        }
        accumulator.score += content_score;
        if let Some(role) = row.message_role {
            accumulator.role_hits.push(role);
        }
        if accumulator.snippets.len() < MAX_SNIPPETS_PER_GROUP {
            accumulator.snippets.push(build_snippet(content, tokens));
        }
    }

    let mut groups = Vec::new();
    for thread_id in order {
        let Some(accumulator) = accumulators.remove(&thread_id) else {
            continue;
        };
        if accumulator.score == 0 {
            continue;
        }
        groups.push(RecallGroup {
            source: "thread",
            id: thread_id,
            title: accumulator.title,
            timestamp: accumulator.timestamp,
            score: accumulator.score,
            lineage: accumulator.lineage,
            summary: format!(
                "Thread with {} message(s); matched {} message(s) across roles: {}.",
                accumulator.message_count,
                accumulator.role_hits.len(),
                summarize_roles(&accumulator.role_hits)
            ),
            snippets: accumulator.snippets,
        });
    }
    Ok(groups)
}

async fn recall_from_transcripts(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let entries = session_manager
        .list_transcript_index_limited(None, Some(MAX_THREAD_SCAN))
        .await?;
    let mut groups = Vec::new();
    for entry in entries {
        let mut combined = entry.preview.clone().unwrap_or_default();
        let transcript_path = PathBuf::from(&entry.filename);
        if transcript_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&transcript_path) {
                if combined.is_empty() || match_score(&combined, tokens) == 0 {
                    combined = content;
                }
            }
        }
        let score = match_score(&combined, tokens);
        if score == 0 {
            continue;
        }
        groups.push(RecallGroup {
            source: "transcript",
            id: entry.id,
            title: entry
                .reason
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| entry.filename.clone()),
            timestamp: entry.captured_at as u64,
            score,
            lineage: None,
            summary: format!(
                "Transcript snapshot from {}{}.",
                entry.filename,
                entry
                    .pane_id
                    .as_deref()
                    .map(|pane| format!(" (pane {pane})"))
                    .unwrap_or_default()
            ),
            snippets: vec![build_snippet(&combined, tokens)],
        });
    }
    Ok(groups)
}

async fn recall_from_agent_events(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let events = session_manager
        .agent_event_recall_matches("behavioral", tokens, MAX_EVENT_SCAN)
        .await?;
    let mut groups = Vec::new();
    for event in events {
        let score = match_score(&event.payload_json, tokens) + match_score(&event.kind, tokens);
        let payload = serde_json::from_str::<Value>(&event.payload_json).unwrap_or(Value::Null);
        groups.push(RecallGroup {
            source: "behavioral",
            id: event.id,
            title: event.kind,
            timestamp: event.timestamp as u64,
            score,
            lineage: payload
                .get("correlation_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            summary: "Behavioral event captured by the capability event bus.".to_string(),
            snippets: vec![build_snippet(&event.payload_json, tokens)],
        });
    }
    Ok(groups)
}

fn recall_from_telemetry(kind: &'static str, tokens: &[String]) -> Result<Vec<RecallGroup>> {
    let path = zorai_protocol::ensure_zorai_data_dir()?
        .join("semantic-logs")
        .join(format!("{kind}.jsonl"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let mut groups = Vec::new();
    for (index, line) in raw.lines().rev().take(MAX_TELEMETRY_SCAN_LINES).enumerate() {
        let score = match_score(line, tokens);
        if score == 0 {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).unwrap_or(Value::Null);
        let timestamp = value.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
        let title = value
            .get("execution_id")
            .and_then(Value::as_str)
            .map(|execution_id| format!("{kind}:{execution_id}"))
            .unwrap_or_else(|| format!("{kind}:{index}"));
        groups.push(RecallGroup {
            source: kind,
            id: title.clone(),
            title,
            timestamp,
            score,
            lineage: value
                .get("session_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            summary: telemetry_summary(kind, &value),
            snippets: vec![build_snippet(line, tokens)],
        });
    }
    Ok(groups)
}

fn telemetry_summary(kind: &str, value: &Value) -> String {
    match kind {
        "operational" => {
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let exit_code = value
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| code.to_string())
                .unwrap_or_else(|| "?".to_string());
            format!("Operational trace for `{command}` (exit {exit_code}).")
        }
        "cognitive" => {
            let rationale = value
                .get("rationale")
                .and_then(Value::as_str)
                .unwrap_or("no rationale recorded");
            format!(
                "Cognitive trace captured rationale: {}.",
                truncate_words(rationale, 12)
            )
        }
        other => format!("{other} telemetry entry."),
    }
}

fn summarize_roles(roles: &[String]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for role in roles {
        *counts.entry(role.clone()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|(role, count)| format!("{role} x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_snippet(text: &str, tokens: &[String]) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return String::new();
    }
    let lowered = cleaned.to_ascii_lowercase();
    let start_byte = tokens
        .iter()
        .filter_map(|token| lowered.find(token))
        .min()
        .unwrap_or(0);
    let start_char = lowered[..start_byte].chars().count().saturating_sub(48);
    cleaned
        .chars()
        .skip(start_char)
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

fn truncate_words(text: &str, max_words: usize) -> String {
    let words = text.split_whitespace().take(max_words).collect::<Vec<_>>();
    words.join(" ")
}

fn match_score(text: &str, tokens: &[String]) -> usize {
    if text.trim().is_empty() {
        return 0;
    }
    let lowered = text.to_ascii_lowercase();
    tokens
        .iter()
        .filter(|token| lowered.contains(token.as_str()))
        .count()
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            if token.len() >= 2 {
                Some(token)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorai_protocol::{AgentDbMessage, AgentDbThread, AgentEventRow};

    #[test]
    fn query_tokens_drop_short_noise() {
        assert_eq!(
            query_tokens("did we fix rg in ui?"),
            vec!["did", "we", "fix", "rg", "in", "ui"]
        );
    }

    #[test]
    fn build_snippet_centers_match() {
        let snippet = build_snippet(
            "prefix words alpha beta gamma important-token delta",
            &[String::from("important-token")],
        );
        assert!(snippet.contains("important-token"));
    }

    #[tokio::test]
    async fn session_search_recalls_thread_matches_without_full_message_hydration() -> Result<()> {
        let root = tempfile::tempdir()?;
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        manager
            .create_agent_thread(&AgentDbThread {
                id: "recall-thread-1".to_string(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("codex".to_string()),
                title: "Unrelated thread title".to_string(),
                created_at: 1000,
                updated_at: 2000,
                message_count: 1,
                total_tokens: 0,
                last_preview: "preview".to_string(),
                metadata_json: None,
            })
            .await?;
        manager
            .add_agent_message(&AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "recall-message-1".to_string(),
                thread_id: "recall-thread-1".to_string(),
                created_at: 1500,
                role: "assistant".to_string(),
                content: "Needle-phrase lives only in this persisted message.".to_string(),
                provider: None,
                model: None,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: Some("{\"ok\":true}".to_string()),
            })
            .await?;
        manager
            .execute_database_sql(
                "UPDATE agent_messages SET metadata_json = x'ff' WHERE id = 'recall-message-1'",
            )
            .await?;

        let output = execute_session_search(&manager, "needle-phrase", 5).await?;

        assert!(output.contains("[thread] Unrelated thread title"));
        assert!(output.contains("matched 1 message(s)"));
        assert!(output.contains("Needle-phrase"));
        Ok(())
    }

    #[tokio::test]
    async fn session_search_filters_behavioral_events_in_sql_before_limit() -> Result<()> {
        let root = tempfile::tempdir()?;
        let manager = crate::session_manager::SessionManager::new_test(root.path()).await;
        for index in 0..MAX_EVENT_SCAN {
            manager
                .upsert_agent_event(&AgentEventRow {
                    id: format!("recent-event-{index}"),
                    category: "behavioral".to_string(),
                    kind: "routine_tick".to_string(),
                    pane_id: None,
                    workspace_id: None,
                    surface_id: None,
                    session_id: None,
                    payload_json: serde_json::json!({
                        "detail": format!("nonmatching recent event {index}")
                    })
                    .to_string(),
                    timestamp: 10_000 + index as i64,
                })
                .await?;
        }
        manager
            .upsert_agent_event(&AgentEventRow {
                id: "older-matching-event".to_string(),
                category: "behavioral".to_string(),
                kind: "capability_decision".to_string(),
                pane_id: None,
                workspace_id: None,
                surface_id: None,
                session_id: None,
                payload_json: serde_json::json!({
                    "correlation_id": "older-correlation",
                    "decision": "approve the rare-signal path"
                })
                .to_string(),
                timestamp: 1,
            })
            .await?;

        let output = execute_session_search(&manager, "rare-signal", 5).await?;

        assert!(output.contains("[behavioral] capability_decision"));
        assert!(output.contains("older-matching-event"));
        assert!(output.contains("older-correlation"));
        Ok(())
    }
}
