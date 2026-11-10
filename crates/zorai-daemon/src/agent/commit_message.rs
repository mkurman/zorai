use super::*;
use futures::StreamExt;

const COMMIT_DIFF_MAX_CHARS: usize = 60_000;
const COMMIT_RECENT_SUBJECTS_MAX: usize = 10;

const COMMIT_MESSAGE_SYSTEM_PROMPT: &str =
    "You write git commit messages from a diff. Reply with only the commit message: \
a subject line under 72 characters, then a blank line and a short body when the change needs it. \
Match the style of the recent commit subjects when they are given, including any type(scope): prefix. \
Describe what changed and why, not file-by-file. No markdown fences, no quotes, no explanation.";

pub(crate) fn commit_message_prompt(diff: &str, recent_subjects: &[String]) -> String {
    let mut prompt = String::new();
    let subjects: Vec<&str> = recent_subjects
        .iter()
        .map(|subject| subject.trim())
        .filter(|subject| !subject.is_empty())
        .take(COMMIT_RECENT_SUBJECTS_MAX)
        .collect();
    if !subjects.is_empty() {
        prompt.push_str("Recent commit subjects:\n");
        for subject in subjects {
            prompt.push_str("- ");
            prompt.push_str(subject);
            prompt.push('\n');
        }
        prompt.push('\n');
    }
    let trimmed = diff.trim();
    let mut end = trimmed.len().min(COMMIT_DIFF_MAX_CHARS);
    while !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    prompt.push_str("Diff:\n");
    prompt.push_str(&trimmed[..end]);
    if end < trimmed.len() {
        prompt.push_str("\n[diff truncated]");
    }
    prompt
}

pub(crate) fn sanitize_commit_message(raw: &str) -> Option<String> {
    let mut text = raw.trim();
    if let Some(rest) = text.strip_prefix("```") {
        text = rest.split_once('\n').map(|(_, body)| body).unwrap_or("");
        text = text.trim_end().strip_suffix("```").unwrap_or(text).trim();
    }
    let message = text.trim_matches('"').trim();
    (!message.is_empty()).then(|| message.to_string())
}

impl AgentEngine {
    pub(crate) async fn generate_commit_message(
        &self,
        diff: &str,
        recent_subjects: &[String],
    ) -> Result<String> {
        if diff.trim().is_empty() {
            anyhow::bail!("there are no changes to describe");
        }
        let config = self.config.read().await.clone();
        let resolved = crate::agent::concierge::resolve_concierge_provider(&config)?;
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let provider_config = crate::agent::concierge::fast_concierge_provider_config(&resolved);
        drop(config);

        self.check_circuit_breaker(&provider_id).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(commit_message_prompt(diff, recent_subjects)),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            COMMIT_MESSAGE_SYSTEM_PROMPT,
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
        sanitize_commit_message(&content)
            .ok_or_else(|| anyhow::anyhow!("Rarog returned an empty commit message"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_shows_recent_subjects_so_the_message_follows_the_repo_style() {
        let prompt = commit_message_prompt(
            "diff --git a/x b/x\n+new",
            &["feat(agent): add thing".to_string(), "  ".to_string()],
        );
        assert!(prompt.starts_with("Recent commit subjects:\n- feat(agent): add thing\n\nDiff:\n"));
        assert!(prompt.ends_with("+new"));
    }

    #[test]
    fn prompt_truncates_huge_diffs_on_a_char_boundary() {
        let diff = "é".repeat(COMMIT_DIFF_MAX_CHARS);
        let prompt = commit_message_prompt(&diff, &[]);
        assert!(prompt.ends_with("[diff truncated]"));
        assert!(prompt.len() < diff.len() + 64);
    }

    #[test]
    fn sanitize_strips_fences_and_quotes_but_keeps_the_body() {
        assert_eq!(
            sanitize_commit_message("```\nfix: a\n\nbody\n```").as_deref(),
            Some("fix: a\n\nbody")
        );
        assert_eq!(
            sanitize_commit_message("\"fix: a\"").as_deref(),
            Some("fix: a")
        );
        assert_eq!(sanitize_commit_message("  "), None);
    }
}
