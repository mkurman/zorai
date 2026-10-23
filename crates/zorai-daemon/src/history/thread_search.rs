use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadSearchCandidate {
    pub thread_id: String,
    pub title: String,
    pub updated_at: i64,
    pub snippet: Option<String>,
}

pub(crate) fn thread_search_fts_query(query: &str) -> Option<String> {
    let terms = query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\"*"))
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn hidden_prefix_clause(hidden_id_prefixes: &[String], values: &mut Vec<db::Value>) -> String {
    let mut sql = String::new();
    for prefix in hidden_id_prefixes {
        sql.push_str(" AND t.id NOT LIKE ?");
        values.push(db::Value::Text(format!("{prefix}%")));
    }
    sql
}

fn candidate_from_row(row: &db::Row) -> Option<ThreadSearchCandidate> {
    Some(ThreadSearchCandidate {
        thread_id: row.get(0).ok()?,
        title: row.get(1).ok()?,
        updated_at: row.get(2).ok()?,
        snippet: row.get::<Option<String>>(3).ok()?,
    })
}

impl HistoryStore {
    pub(crate) async fn search_thread_titles(
        &self,
        query: &str,
        limit: usize,
        hidden_id_prefixes: &[String],
    ) -> Result<Vec<ThreadSearchCandidate>> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut values = vec![db::Value::Text(query.to_string())];
        let mut sql = "SELECT t.id, t.title, t.updated_at, NULL FROM agent_threads t \
             WHERE t.deleted_at IS NULL AND instr(lower(t.title), lower(?)) > 0"
            .to_string();
        sql.push_str(&hidden_prefix_clause(hidden_id_prefixes, &mut values));
        sql.push_str(" ORDER BY t.updated_at DESC LIMIT ?");
        values.push(db::Value::Integer(limit as i64));
        let rows = self
            .interactive_read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        Ok(rows.iter().filter_map(candidate_from_row).collect())
    }

    pub(crate) async fn search_thread_messages(
        &self,
        query: &str,
        limit: usize,
        hidden_id_prefixes: &[String],
    ) -> Result<Vec<ThreadSearchCandidate>> {
        let Some(fts_query) = thread_search_fts_query(query) else {
            return Ok(Vec::new());
        };
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut values = vec![db::Value::Text(fts_query)];
        let mut sql = "SELECT f.thread_id, t.title, t.updated_at, \
             snippet(agent_message_fts, 1, '', '', '…', 24) \
             FROM agent_message_fts f JOIN agent_threads t ON t.id = f.thread_id \
             WHERE agent_message_fts MATCH ? AND t.deleted_at IS NULL"
            .to_string();
        sql.push_str(&hidden_prefix_clause(hidden_id_prefixes, &mut values));
        sql.push_str(" ORDER BY bm25(agent_message_fts) LIMIT ?");
        values.push(db::Value::Integer((limit * 8) as i64));
        let rows = self
            .interactive_read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        let mut candidates: Vec<ThreadSearchCandidate> = Vec::new();
        for candidate in rows.iter().filter_map(candidate_from_row) {
            if candidates.len() >= limit {
                break;
            }
            if !candidates
                .iter()
                .any(|existing| existing.thread_id == candidate.thread_id)
            {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }

    pub(crate) async fn thread_search_candidates_for_messages(
        &self,
        message_ids: &[String],
        hidden_id_prefixes: &[String],
    ) -> Result<Vec<ThreadSearchCandidate>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; message_ids.len()].join(", ");
        let mut values = message_ids
            .iter()
            .map(|id| db::Value::Text(id.clone()))
            .collect::<Vec<_>>();
        let mut sql = format!(
            "SELECT m.id, t.id, t.title, t.updated_at, substr(m.content, 1, 240) \
             FROM agent_messages m JOIN agent_threads t ON t.id = m.thread_id \
             WHERE m.id IN ({placeholders}) AND m.deleted_at IS NULL \
             AND m.role IN ('user', 'assistant') AND t.deleted_at IS NULL"
        );
        sql.push_str(&hidden_prefix_clause(hidden_id_prefixes, &mut values));
        let rows = self
            .interactive_read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        let mut by_message = std::collections::HashMap::new();
        for row in &rows {
            let (Ok(message_id), Ok(thread_id), Ok(title), Ok(updated_at), Ok(snippet)) = (
                row.get::<String>(0),
                row.get::<String>(1),
                row.get::<String>(2),
                row.get::<i64>(3),
                row.get::<String>(4),
            ) else {
                continue;
            };
            by_message.insert(
                message_id,
                ThreadSearchCandidate {
                    thread_id,
                    title,
                    updated_at,
                    snippet: Some(snippet),
                },
            );
        }
        let mut candidates: Vec<ThreadSearchCandidate> = Vec::new();
        for message_id in message_ids {
            let Some(candidate) = by_message.remove(message_id) else {
                continue;
            };
            if !candidates
                .iter()
                .any(|existing| existing.thread_id == candidate.thread_id)
            {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }
}
