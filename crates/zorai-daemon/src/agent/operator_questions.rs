#![allow(dead_code)]

use super::*;

#[derive(Debug)]
pub(super) struct PendingOperatorQuestionState {
    question: PendingOperatorQuestion,
    response_tx: Option<tokio::sync::oneshot::Sender<String>>,
    tool_call_id: Option<String>,
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

    pub(super) async fn restore_pending_operator_questions(&self) {
        let persisted = match self.history.list_pending_operator_questions().await {
            Ok(persisted) => persisted,
            Err(error) => {
                tracing::warn!("failed to load pending operator questions from sqlite: {error}");
                return;
            }
        };
        let mut pending_questions = self.pending_operator_questions.lock().await;
        for persisted in persisted {
            pending_questions.insert(
                persisted.question_id.clone(),
                PendingOperatorQuestionState {
                    question: PendingOperatorQuestion {
                        question_id: persisted.question_id,
                        content: persisted.content,
                        options: persisted.options,
                        session_id: persisted.session_id,
                        thread_id: Some(persisted.thread_id),
                    },
                    response_tx: None,
                    tool_call_id: Some(persisted.tool_call_id),
                },
            );
        }
    }

    pub async fn ask_operator_question(
        &self,
        content: &str,
        options: Vec<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
    ) -> Result<(String, String)> {
        self.ask_operator_question_for_tool_call(content, options, session_id, thread_id, None)
            .await
    }

    pub(crate) async fn ask_operator_question_for_tool_call(
        &self,
        content: &str,
        options: Vec<String>,
        session_id: Option<String>,
        thread_id: Option<String>,
        tool_call_id: Option<String>,
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
                tool_call_id: tool_call_id.clone(),
            },
        );
        if let (Some(thread_id), Some(tool_call_id)) = (question.thread_id.clone(), tool_call_id) {
            if let Err(error) = self
                .history
                .insert_pending_operator_question(
                    &crate::history::operator_questions::PersistedOperatorQuestion {
                        question_id: question_id.clone(),
                        thread_id,
                        tool_call_id,
                        session_id: question.session_id.clone(),
                        content: content.clone(),
                        options: options.clone(),
                    },
                )
                .await
            {
                tracing::warn!(question_id = %question_id, "failed to persist operator question: {error}");
            }
        }

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

    pub async fn answer_operator_question(
        self: &Arc<Self>,
        question_id: &str,
        answer: &str,
    ) -> Result<()> {
        let normalized_answer = answer.trim();
        if normalized_answer.is_empty() {
            anyhow::bail!("answer must not be empty");
        }

        let pending = {
            let mut pending_questions = self.pending_operator_questions.lock().await;
            pending_questions
                .remove(question_id)
                .ok_or_else(|| anyhow::anyhow!("unknown operator question: {question_id}"))?
        };

        if let Err(error) = self
            .history
            .delete_pending_operator_question(question_id)
            .await
        {
            tracing::warn!(
                question_id,
                "failed to delete persisted operator question: {error}"
            );
        }

        let tool_answer = if pending
            .question
            .options
            .iter()
            .any(|option| option == normalized_answer)
        {
            normalized_answer.to_string()
        } else {
            format!(
                "The operator chose none of the options and answered in their own words: {normalized_answer}"
            )
        };
        let undelivered = match pending.response_tx {
            Some(response_tx) => response_tx.send(tool_answer).err(),
            None => Some(tool_answer),
        };
        let resumed = match (
            undelivered,
            pending.question.thread_id.as_deref(),
            pending.tool_call_id.as_deref(),
        ) {
            (Some(tool_answer), Some(thread_id), Some(tool_call_id)) => {
                self.resume_turn_with_operator_answer(thread_id, tool_call_id, tool_answer)
                    .await
            }
            _ => Ok(()),
        };

        let _ = self.event_tx.send(AgentEvent::OperatorQuestionResolved {
            question_id: question_id.to_string(),
            answer: normalized_answer.to_string(),
            thread_id: pending.question.thread_id,
        });
        resumed
    }

    async fn resume_turn_with_operator_answer(
        self: &Arc<Self>,
        thread_id: &str,
        tool_call_id: &str,
        tool_answer: String,
    ) -> Result<()> {
        self.ensure_thread_messages_loaded(thread_id).await;
        {
            let mut threads = self.threads.write().await;
            let awaiting_call = threads
                .get(thread_id)
                .and_then(|thread| thread.messages.last())
                .and_then(|message| message.tool_calls.as_ref())
                .and_then(|calls| calls.iter().find(|call| call.id == tool_call_id))
                .cloned();
            let (Some(thread), Some(call)) = (threads.get_mut(thread_id), awaiting_call) else {
                anyhow::bail!(
                    "this question expired: the conversation moved on after it was asked"
                );
            };
            let mut message = AgentMessage::user(tool_answer, now_millis());
            message.role = MessageRole::Tool;
            message.tool_call_id = Some(call.id);
            message.tool_name = Some(call.function.name);
            message.tool_arguments = Some(call.function.arguments);
            message.tool_status = Some("done".to_string());
            thread.messages.push(message);
            thread.updated_at = now_millis();
        }
        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        let engine = self.clone();
        let thread_id = thread_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = engine
                .continue_existing_user_message_without_queue_drain(&thread_id, "")
                .await
            {
                tracing::warn!(thread_id = %thread_id, "failed to resume turn after operator answer: {error}");
            }
        });
        Ok(())
    }
}
