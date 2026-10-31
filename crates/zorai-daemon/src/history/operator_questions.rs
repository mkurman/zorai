use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PersistedOperatorQuestion {
    pub(crate) question_id: String,
    pub(crate) thread_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) session_id: Option<String>,
    pub(crate) content: String,
    pub(crate) options: Vec<String>,
}

impl HistoryStore {
    pub(crate) async fn insert_pending_operator_question(
        &self,
        question: &PersistedOperatorQuestion,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO pending_operator_questions
                    (question_id, thread_id, tool_call_id, session_id, content, options_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    question.question_id.clone(),
                    question.thread_id.clone(),
                    question.tool_call_id.clone(),
                    question.session_id.clone(),
                    question.content.clone(),
                    serde_json::to_string(&question.options)?,
                    now_ts() as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn delete_pending_operator_question(&self, question_id: &str) -> Result<()> {
        self.conn_db
            .execute(
                "DELETE FROM pending_operator_questions WHERE question_id = ?1",
                db::db_params![question_id],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn list_pending_operator_questions(
        &self,
    ) -> Result<Vec<PersistedOperatorQuestion>> {
        let rows = self
            .interactive_read_db
            .query(
                "SELECT question_id, thread_id, tool_call_id, session_id, content, options_json
                 FROM pending_operator_questions
                 ORDER BY created_at ASC, question_id ASC",
                db::Params::None,
            )
            .await?;
        rows.iter()
            .map(|row| {
                Ok(PersistedOperatorQuestion {
                    question_id: row.get(0)?,
                    thread_id: row.get(1)?,
                    tool_call_id: row.get(2)?,
                    session_id: row.get(3)?,
                    content: row.get(4)?,
                    options: serde_json::from_str(&row.get::<String>(5)?)?,
                })
            })
            .collect()
    }
}
