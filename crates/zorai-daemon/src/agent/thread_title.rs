//! Background generation of conversation titles from the first operator message.

use super::*;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;

const PLACEHOLDER_TITLE_CHARS: usize = 50;
const GENERATED_TITLE_MAX_CHARS: usize = 80;
const FIRST_MESSAGE_PROMPT_MAX_CHARS: usize = 1500;

const TITLE_SYSTEM_PROMPT: &str =
    "You generate a short conversation title from the operator's first message. \
Reply with only the title. No quotes, no markdown, no trailing punctuation, no explanation. \
Use 3 to 8 words and stay under 60 characters. Capture the intent, not a verbatim excerpt.";

#[derive(Debug, Clone)]
pub(super) struct AutoThreadTitleJob {
    thread_id: String,
    first_user_message: String,
    placeholder_title: String,
    mode: AutoThreadTitleMode,
}

pub(super) fn placeholder_thread_title(content: &str) -> String {
    content.chars().take(PLACEHOLDER_TITLE_CHARS).collect()
}

pub(crate) fn sanitize_generated_thread_title(raw: &str) -> Option<String> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    let stripped = first_line
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '*'))
        .trim()
        .trim_matches(|ch| matches!(ch, '.' | '!' | '?' | ':' | ';'))
        .trim();
    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let lowered = collapsed.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "null" | "none" | "n/a" | "na" | "untitled" | "title" | "new conversation"
    ) {
        return None;
    }
    let truncated: String = collapsed.chars().take(GENERATED_TITLE_MAX_CHARS).collect();
    let trimmed = truncated.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn thread_eligible_for_auto_title(thread_id: &str) -> bool {
    let normalized = thread_id.trim();
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case(crate::agent::concierge::CONCIERGE_THREAD_ID)
    {
        return false;
    }
    if crate::agent::agent_identity::is_internal_dm_thread(thread_id) {
        return false;
    }
    if crate::agent::agent_identity::is_participant_playground_thread(thread_id) {
        return false;
    }
    if thread_id.starts_with(INTERNAL_HANDOFF_THREAD_PREFIX) {
        return false;
    }
    true
}

pub(super) fn spawn_auto_thread_title_worker(
    engine: Arc<AgentEngine>,
    mut jobs: mpsc::UnboundedReceiver<AutoThreadTitleJob>,
) {
    tokio::spawn(async move {
        while let Some(job) = jobs.recv().await {
            if let Err(error) = engine.run_auto_thread_title_job(job).await {
                tracing::debug!(error = %error, "auto thread title generation skipped");
            }
        }
    });
}

impl AgentEngine {
    async fn thread_identity_skips_auto_title(&self, thread_id: &str) -> bool {
        self.thread_identity_metadata
            .read()
            .await
            .get(thread_id)
            .is_some_and(ThreadIdentityMetadata::skips_auto_thread_title)
    }

    pub(super) async fn queue_auto_thread_title_if_enabled(&self, thread_id: &str, content: &str) {
        if !thread_eligible_for_auto_title(thread_id) {
            return;
        }
        if self.thread_identity_skips_auto_title(thread_id).await {
            return;
        }
        let first_user_message = content.trim();
        if first_user_message.is_empty() {
            return;
        }
        let mode = self.config.read().await.auto_thread_title;
        if !mode.is_enabled() {
            return;
        }
        let job = AutoThreadTitleJob {
            thread_id: thread_id.to_string(),
            first_user_message: first_user_message
                .chars()
                .take(FIRST_MESSAGE_PROMPT_MAX_CHARS)
                .collect(),
            placeholder_title: placeholder_thread_title(content),
            mode,
        };
        if self.auto_thread_title_jobs.send(job).is_err() {
            tracing::debug!(thread_id, "auto thread title worker is not running");
        }
    }

    async fn run_auto_thread_title_job(&self, job: AutoThreadTitleJob) -> Result<()> {
        let generated = self
            .generate_thread_title_with_mode(job.mode, &job.first_user_message)
            .await?;
        let Some(title) = sanitize_generated_thread_title(&generated) else {
            return Ok(());
        };
        self.apply_generated_thread_title(&job.thread_id, &job.placeholder_title, &title)
            .await;
        Ok(())
    }

    async fn generate_thread_title_with_mode(
        &self,
        mode: AutoThreadTitleMode,
        first_user_message: &str,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        let (provider_id, provider_config) = match mode {
            AutoThreadTitleMode::Off => anyhow::bail!("auto thread title is off"),
            AutoThreadTitleMode::Rarog => {
                let resolved = crate::agent::concierge::resolve_concierge_provider(&config)?;
                let provider_id = config
                    .concierge
                    .provider
                    .as_deref()
                    .unwrap_or(&config.provider)
                    .to_string();
                (
                    provider_id,
                    crate::agent::concierge::fast_concierge_provider_config(&resolved),
                )
            }
            AutoThreadTitleMode::Weles => {
                let weles = super::config::build_effective_weles_definition(&config);
                let mut resolved =
                    resolve_provider_config_for(&config, &weles.provider, Some(&weles.model))?;
                crate::agent::provider_resolution::apply_role_transport_override(
                    &weles.provider,
                    &mut resolved,
                    weles.api_transport,
                );
                (weles.provider, resolved)
            }
        };
        drop(config);

        self.check_circuit_breaker(&provider_id).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(format!(
                "First operator message:\n{}",
                first_user_message.trim()
            )),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            TITLE_SYSTEM_PROMPT,
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
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta { content: delta, .. } => content.push_str(&delta),
                CompletionChunk::Done {
                    content: done_content,
                    ..
                } => {
                    self.record_llm_outcome(&provider_id, true).await;
                    if !done_content.trim().is_empty() {
                        content = done_content;
                    }
                    break;
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("LLM error: {message}");
                }
                _ => {}
            }
        }
        if content.trim().is_empty() {
            anyhow::bail!("empty thread title response");
        }
        Ok(content)
    }

    pub(super) async fn apply_generated_thread_title(
        &self,
        thread_id: &str,
        placeholder_title: &str,
        title: &str,
    ) -> bool {
        if self.thread_identity_skips_auto_title(thread_id).await {
            return false;
        }
        let updated_at = now_millis();
        {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            if thread.title != placeholder_title && !thread.title.trim().is_empty() {
                return false;
            }
            thread.title = title.to_string();
            thread.updated_at = updated_at;
        }
        if let Err(error) = self
            .history
            .update_thread_title(thread_id, title, updated_at as i64)
            .await
        {
            tracing::warn!(thread_id, error = %error, "failed to persist generated thread title");
        }
        let _ = self.event_tx.send(AgentEvent::ThreadTitleUpdated {
            thread_id: thread_id.to_string(),
            title: title.to_string(),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[test]
    fn sanitize_generated_thread_title_strips_quotes_and_rejects_empty() {
        assert_eq!(
            sanitize_generated_thread_title("  \"Fix the login bug\"  \nmore"),
            Some("Fix the login bug".to_string())
        );
        assert_eq!(sanitize_generated_thread_title("null"), None);
        assert_eq!(sanitize_generated_thread_title("   "), None);
        assert_eq!(sanitize_generated_thread_title("Untitled"), None);
    }

    #[test]
    fn thread_eligibility_skips_internal_and_system_threads() {
        assert!(thread_eligible_for_auto_title("thread-user-1"));
        assert!(!thread_eligible_for_auto_title("concierge"));
        assert!(!thread_eligible_for_auto_title("dm:svarog:weles"));
        assert!(!thread_eligible_for_auto_title("playground:abc"));
        assert!(!thread_eligible_for_auto_title("handoff:abc"));
    }

    #[tokio::test]
    async fn apply_generated_thread_title_skips_task_owned_threads() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let (thread_id, _) = engine
            .get_or_create_thread_with_target(None, "DeepSeekorrr", None)
            .await;
        let placeholder = placeholder_thread_title("DeepSeekorrr");
        engine
            .set_thread_identity_metadata(
                &thread_id,
                ThreadIdentityMetadata {
                    thread_id: thread_id.clone(),
                    goal_run_id: None,
                    goal_id: None,
                    task_id: Some("task-subagent".to_string()),
                    parent_task_id: Some("task-parent".to_string()),
                    parent_thread_id: Some("thread-parent".to_string()),
                    source: Some("subagent".to_string()),
                    reserved_at: Some(now_millis()),
                },
            )
            .await;

        assert!(
            !engine
                .apply_generated_thread_title(
                    &thread_id,
                    &placeholder,
                    "Greeting from DeepSeekorrr"
                )
                .await
        );
        let threads = engine.threads.read().await;
        assert_eq!(threads.get(&thread_id).unwrap().title, placeholder);
    }

    #[tokio::test]
    async fn apply_generated_thread_title_replaces_placeholder_and_skips_renamed() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let content = "Please review the billing invoice parser";
        let (thread_id, created) = engine.get_or_create_thread(None, content).await;
        assert!(created);
        let placeholder = placeholder_thread_title(content);
        {
            let threads = engine.threads.read().await;
            assert_eq!(threads.get(&thread_id).unwrap().title, placeholder);
        }

        assert!(
            engine
                .apply_generated_thread_title(&thread_id, &placeholder, "Billing invoice parser")
                .await
        );
        {
            let threads = engine.threads.read().await;
            assert_eq!(
                threads.get(&thread_id).unwrap().title,
                "Billing invoice parser"
            );
        }

        assert!(
            !engine
                .apply_generated_thread_title(&thread_id, &placeholder, "Should not replace")
                .await
        );
        {
            let threads = engine.threads.read().await;
            assert_eq!(
                threads.get(&thread_id).unwrap().title,
                "Billing invoice parser"
            );
        }
    }

    #[tokio::test]
    async fn rename_thread_persists_title_so_reopening_does_not_revert_it() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let content = "Please review the billing invoice parser";
        let (thread_id, _) = engine.get_or_create_thread(None, content).await;
        engine.persist_thread_by_id(&thread_id).await;
        let placeholder = placeholder_thread_title(content);

        assert!(engine.rename_thread(&thread_id, "  My renamed thread ").await);
        assert!(!engine.rename_thread(&thread_id, "   ").await);

        let persisted = engine
            .history
            .get_thread(&thread_id)
            .await
            .expect("load thread")
            .expect("thread row");
        assert_eq!(persisted.title, "My renamed thread");
        assert!(
            !engine
                .apply_generated_thread_title(&thread_id, &placeholder, "Auto title")
                .await
        );
        let threads = engine.threads.read().await;
        assert_eq!(threads.get(&thread_id).unwrap().title, "My renamed thread");
    }

    #[test]
    fn auto_thread_title_mode_parses_known_values() {
        assert_eq!(
            AutoThreadTitleMode::parse("rarog"),
            AutoThreadTitleMode::Rarog
        );
        assert_eq!(
            AutoThreadTitleMode::parse("WELES"),
            AutoThreadTitleMode::Weles
        );
        assert_eq!(AutoThreadTitleMode::parse("off"), AutoThreadTitleMode::Off);
        assert_eq!(AutoThreadTitleMode::parse("nope"), AutoThreadTitleMode::Off);
        assert!(!AutoThreadTitleMode::Off.is_enabled());
        assert!(AutoThreadTitleMode::Rarog.is_enabled());
        let config: AgentConfig = serde_json::from_value(serde_json::json!({
            "auto_thread_title": "mystery"
        }))
        .expect("partial agent config should deserialize");
        assert_eq!(config.auto_thread_title, AutoThreadTitleMode::Off);
    }
}
