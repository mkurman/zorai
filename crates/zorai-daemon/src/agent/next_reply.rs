use super::*;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;

const DEFAULT_CONTEXT_MESSAGES: usize = 1;
const MESSAGE_PROMPT_MAX_CHARS: usize = 4000;
const SUGGESTION_MAX_CHARS: usize = 600;

const NEXT_REPLY_INSTRUCTIONS: &str =
    "You suggest the operator's next message in this conversation. \
Write it exactly as the operator would type it: first person, short, usually one sentence. \
Reply with only that message. No quotes, no labels, no explanation. \
If no natural next message exists, reply with NONE.";

#[derive(Debug, Clone)]
pub(super) struct NextReplyJob {
    thread_id: String,
    last_message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NextReplySource {
    Custom {
        provider: String,
        model: String,
        reasoning_effort: Option<String>,
    },
    Agent(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NextReplySettings {
    source: NextReplySource,
    context_messages: usize,
}

fn next_reply_settings(config: &AgentConfig) -> Option<NextReplySettings> {
    let raw = config.extra.get("next_reply")?;
    if raw.get("enabled").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let text = |key: &str| {
        raw.get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let source = if raw.get("source").and_then(|value| value.as_str()) == Some("agent") {
        NextReplySource::Agent(text("agent_id").unwrap_or_else(|| MAIN_AGENT_ID.to_string()))
    } else {
        NextReplySource::Custom {
            provider: text("provider").unwrap_or_else(|| config.provider.clone()),
            model: text("model").unwrap_or_default(),
            reasoning_effort: text("reasoning_effort"),
        }
    };
    let context_messages = raw
        .get("context_messages")
        .and_then(|value| value.as_u64())
        .map(|value| value.max(1) as usize)
        .unwrap_or(DEFAULT_CONTEXT_MESSAGES);
    Some(NextReplySettings {
        source,
        context_messages,
    })
}

fn next_reply_context(messages: &[AgentMessage], count: usize) -> Vec<&AgentMessage> {
    let start = messages
        .iter()
        .rposition(crate::agent::message_is_compaction_summary)
        .unwrap_or(0);
    let eligible = messages[start..]
        .iter()
        .filter(|message| {
            crate::agent::message_is_compaction_summary(message)
                || (matches!(message.role, MessageRole::User | MessageRole::Assistant)
                    && message.tool_calls.as_ref().is_none_or(Vec::is_empty)
                    && !message.content.trim().is_empty())
        })
        .collect::<Vec<_>>();
    let skip = eligible.len().saturating_sub(count.max(1));
    eligible.into_iter().skip(skip).collect()
}

fn next_reply_prompt(context: &[&AgentMessage]) -> String {
    let transcript = context
        .iter()
        .map(|message| {
            let speaker = if crate::agent::message_is_compaction_summary(message) {
                "Earlier context"
            } else if message.role == MessageRole::User {
                "Operator"
            } else {
                "Assistant"
            };
            let content = message
                .content
                .trim()
                .chars()
                .take(MESSAGE_PROMPT_MAX_CHARS)
                .collect::<String>();
            format!("{speaker}:\n{content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Conversation, most recent last:\n\n{transcript}\n\nWrite the operator's next message.")
}

fn sanitize_next_reply(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(trimmed.chars().take(SUGGESTION_MAX_CHARS).collect())
}

pub(super) fn spawn_next_reply_worker(
    engine: Arc<AgentEngine>,
    mut jobs: mpsc::UnboundedReceiver<NextReplyJob>,
) {
    tokio::spawn(async move {
        while let Some(job) = jobs.recv().await {
            if let Err(error) = engine.run_next_reply_job(job).await {
                tracing::debug!(error = %error, "next reply suggestion skipped");
            }
        }
    });
}

impl AgentEngine {
    pub(super) async fn queue_next_reply_suggestion(&self, thread_id: &str) {
        if !super::thread_title::thread_eligible_for_auto_title(thread_id) {
            return;
        }
        if next_reply_settings(&*self.config.read().await).is_none() {
            return;
        }
        let Some(last_message_id) = self
            .threads
            .read()
            .await
            .get(thread_id)
            .and_then(|thread| thread.messages.last())
            .filter(|message| message.role == MessageRole::Assistant)
            .map(|message| message.id.clone())
        else {
            return;
        };
        let job = NextReplyJob {
            thread_id: thread_id.to_string(),
            last_message_id,
        };
        if self.next_reply_jobs.send(job).is_err() {
            tracing::debug!(thread_id, "next reply worker is not running");
        }
    }

    async fn run_next_reply_job(&self, job: NextReplyJob) -> Result<()> {
        let config = self.config.read().await.clone();
        let Some(settings) = next_reply_settings(&config) else {
            return Ok(());
        };
        let prompt = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(&job.thread_id) else {
                return Ok(());
            };
            let context = next_reply_context(&thread.messages, settings.context_messages);
            if context.is_empty() {
                return Ok(());
            }
            next_reply_prompt(&context)
        };
        let (provider_id, provider_config, system_prompt) = match &settings.source {
            NextReplySource::Custom {
                provider,
                model,
                reasoning_effort,
            } => {
                let model = Some(model.as_str()).filter(|value| !value.is_empty());
                let mut provider_config = resolve_provider_config_for(&config, provider, model)?;
                if let Some(effort) = reasoning_effort {
                    provider_config.reasoning_effort = effort.clone();
                }
                (
                    provider.clone(),
                    provider_config,
                    NEXT_REPLY_INSTRUCTIONS.to_string(),
                )
            }
            NextReplySource::Agent(agent_id) => {
                let responder = self.participant_observer_responder_config(agent_id).await?;
                let system_prompt = if responder.base_prompt.trim().is_empty() {
                    NEXT_REPLY_INSTRUCTIONS.to_string()
                } else {
                    format!("{}\n\n{NEXT_REPLY_INSTRUCTIONS}", responder.base_prompt)
                };
                (
                    responder.provider_id,
                    responder.provider_config,
                    system_prompt,
                )
            }
        };
        drop(config);

        self.check_circuit_breaker(&provider_id).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            &system_prompt,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );
        let mut content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content: delta, .. }) => content.push_str(&delta),
                Ok(CompletionChunk::Done {
                    content: done_content,
                    ..
                }) => {
                    self.record_llm_outcome(&provider_id, true).await;
                    if !done_content.trim().is_empty() {
                        content = done_content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("LLM error: {message}");
                }
                Ok(_) => {}
                Err(error) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    return Err(error);
                }
            }
        }
        let Some(suggestion) = sanitize_next_reply(&content) else {
            return Ok(());
        };
        let still_current = self
            .threads
            .read()
            .await
            .get(&job.thread_id)
            .and_then(|thread| thread.messages.last())
            .is_some_and(|message| message.id == job.last_message_id);
        if still_current {
            let _ = self.event_tx.send(AgentEvent::NextReplySuggestion {
                thread_id: job.thread_id,
                content: suggestion,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: MessageRole, content: &str) -> AgentMessage {
        let mut message = AgentMessage::user(content, 1);
        message.role = role;
        message
    }

    #[test]
    fn suggestions_stay_off_until_enabled_and_default_to_the_last_message() {
        let mut config = AgentConfig::default();
        assert!(next_reply_settings(&config).is_none());
        config.extra.insert(
            "next_reply".to_string(),
            serde_json::json!({ "enabled": true, "provider": "openai", "model": "gpt-5.4-mini" }),
        );
        let settings = next_reply_settings(&config).expect("enabled settings");
        assert_eq!(settings.context_messages, 1);
        assert_eq!(
            settings.source,
            NextReplySource::Custom {
                provider: "openai".to_string(),
                model: "gpt-5.4-mini".to_string(),
                reasoning_effort: None,
            }
        );
    }

    #[test]
    fn context_never_reaches_past_the_latest_compaction_and_skips_tool_chatter() {
        let mut tool_call = message(MessageRole::Assistant, "checking");
        tool_call.tool_calls = Some(vec![ToolCall {
            id: "c".to_string(),
            function: ToolFunction {
                name: "read_file".to_string(),
                arguments: "{}".to_string(),
            },
            weles_review: None,
        }]);
        let messages = vec![
            message(MessageRole::User, "old question"),
            message(
                MessageRole::Assistant,
                "[Compacted earlier context] summary",
            ),
            message(MessageRole::User, "new question"),
            tool_call,
            message(MessageRole::Tool, "file body"),
            message(MessageRole::Assistant, "final answer"),
        ];

        let last_one = next_reply_context(&messages, 1);
        assert_eq!(last_one.len(), 1);
        assert_eq!(last_one[0].content, "final answer");

        let everything = next_reply_context(&messages, 50)
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            everything,
            vec![
                "[Compacted earlier context] summary",
                "new question",
                "final answer"
            ]
        );
    }

    #[test]
    fn none_and_quoted_replies_are_cleaned_so_the_composer_never_shows_noise() {
        assert_eq!(sanitize_next_reply("  NONE "), None);
        assert_eq!(
            sanitize_next_reply("\"Ship it\""),
            Some("Ship it".to_string())
        );
    }
}
