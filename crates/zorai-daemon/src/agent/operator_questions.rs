#![allow(dead_code)]

use super::*;

#[derive(Debug)]
pub(super) struct PendingOperatorQuestionState {
    question: PendingOperatorQuestion,
    response_tx: Option<tokio::sync::oneshot::Sender<String>>,
}

#[derive(Debug, Clone)]
struct PendingOperatorQuestion {
    question_id: String,
    content: String,
    options: Vec<String>,
    session_id: Option<String>,
    thread_id: Option<String>,
}

fn validate_question_content(content: &str) -> Result<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        anyhow::bail!("content must not be empty");
    }
    Ok(trimmed.to_string())
}

fn validate_question_options(options: Vec<String>) -> Result<Vec<String>> {
    if options.len() < 2 {
        anyhow::bail!("options must contain at least two compact ordered tokens");
    }

    let mut normalized = Vec::with_capacity(options.len());
    let mut seen = HashSet::new();
    for option in options {
        let trimmed = option.trim();
        if trimmed.is_empty() {
            anyhow::bail!("options must not contain empty labels");
        }
        if trimmed.len() > 4
            || trimmed.chars().any(|ch| ch.is_whitespace())
            || !trimmed.chars().all(|ch| ch.is_ascii_alphanumeric())
        {
            anyhow::bail!(
                "options must use compact ordered tokens like A/B/C/D or 1/2/3/4; move full answer text into content"
            );
        }
        if !seen.insert(trimmed.to_ascii_uppercase()) {
            anyhow::bail!("options must be unique");
        }
        normalized.push(trimmed.to_string());
    }

    Ok(normalized)
}

impl AgentEngine {
    pub(super) async fn pending_operator_question_thread_ids(&self) -> HashSet<String> {
        self.pending_operator_questions
            .lock()
            .await
            .values()
            .filter_map(|state| state.question.thread_id.clone())
            .collect()
    }

    pub(crate) async fn pending_operator_question_events_for_thread(
        &self,
        thread_id: &str,
    ) -> Vec<AgentEvent> {
        self.pending_operator_questions
            .lock()
            .await
            .values()
            .filter(|state| state.question.thread_id.as_deref() == Some(thread_id))
            .map(|state| AgentEvent::OperatorQuestion {
                question_id: state.question.question_id.clone(),
                content: state.question.content.clone(),
                options: state.question.options.clone(),
                session_id: state.question.session_id.clone(),
                thread_id: state.question.thread_id.clone(),
            })
            .collect()
    }

    pub async fn ask_operator_question(
        &self,
        content: &str,
        options: Vec<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<(String, String)> {
        let content = validate_question_content(content)?;
        let options = validate_question_options(options)?;
        let question_id = format!("oq_{}", uuid::Uuid::new_v4());
        let question = PendingOperatorQuestion {
            question_id: question_id.clone(),
            content: content.clone(),
            options: options.clone(),
            session_id: session_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            thread_id: thread_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.pending_operator_questions.lock().await.insert(
            question_id.clone(),
            PendingOperatorQuestionState {
                question: question.clone(),
                response_tx: Some(response_tx),
            },
        );

        let _ = self.event_tx.send(AgentEvent::OperatorQuestion {
            question_id: question_id.clone(),
            content,
            options,
            session_id: question.session_id.clone(),
            thread_id: question.thread_id.clone(),
        });

        let answer = response_rx
            .await
            .context("operator question ended before an answer arrived")?;
        Ok((question_id, answer))
    }

    pub async fn answer_operator_question(&self, question_id: &str, answer: &str) -> Result<()> {
        let normalized_answer = answer.trim();
        if normalized_answer.is_empty() {
            anyhow::bail!("answer must not be empty");
        }

        let pending = {
            let mut pending_questions = self.pending_operator_questions.lock().await;
            let state = pending_questions
                .get(question_id)
                .ok_or_else(|| anyhow::anyhow!("unknown operator question: {question_id}"))?;
            if !state
                .question
                .options
                .iter()
                .any(|option| option == normalized_answer)
            {
                anyhow::bail!("answer must match one of the advertised compact option tokens");
            }
            pending_questions
                .remove(question_id)
                .expect("pending operator question should still exist")
        };

        if let Some(response_tx) = pending.response_tx {
            let _ = response_tx.send(normalized_answer.to_string());
        }

        let _ = self.event_tx.send(AgentEvent::OperatorQuestionResolved {
            question_id: question_id.to_string(),
            answer: normalized_answer.to_string(),
            thread_id: pending.question.thread_id,
        });
        Ok(())
    }
}
