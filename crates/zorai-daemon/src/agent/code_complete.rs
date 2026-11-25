use super::*;
use futures::StreamExt;

const PREFIX_MAX_CHARS: usize = 6_000;
const SUFFIX_MAX_CHARS: usize = 3_000;
const COMPLETION_MAX_TOKENS: u32 = 256;
const CODE_COMPLETE_SYSTEM_PROMPT: &str = "You are an inline code completion engine inside an editor. \
You get the code before the cursor and the code after it. Reply with only the text to insert at the cursor: \
do not repeat code that is already before or after the cursor, no explanation, no markdown fences. \
Keep it short: finish the current line or statement, at most a few lines. If nothing sensible fits, reply with nothing.";

static IN_FLIGHT: std::sync::Mutex<Option<tokio::task::AbortHandle>> = std::sync::Mutex::new(None);

pub(crate) fn supersede_in_flight(handle: tokio::task::AbortHandle) {
    if let Ok(mut slot) = IN_FLIGHT.lock() {
        if let Some(previous) = slot.replace(handle) {
            previous.abort();
        }
    }
}

pub(crate) struct CodeCompleteRequest<'a> {
    pub path: &'a str,
    pub language: &'a str,
    pub prefix: &'a str,
    pub suffix: &'a str,
}

fn glob_matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            name.len() >= head.len() + tail.len()
                && name.starts_with(head)
                && glob_matches_rest(tail, &name[head.len()..])
        }
    }
}

fn glob_matches_rest(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return name.ends_with(pattern);
    }
    let (head, tail) = pattern.split_once('*').unwrap_or((pattern, ""));
    (0..=name.len())
        .filter(|start| name.is_char_boundary(*start))
        .any(|start| {
            name[start..].starts_with(head) && glob_matches_rest(tail, &name[start + head.len()..])
        })
}

pub(crate) fn is_ignored_path(path: &str, globs: &[String]) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    globs
        .iter()
        .map(|glob| glob.trim())
        .filter(|glob| !glob.is_empty())
        .any(|glob| glob_matches(glob, name) || glob_matches(glob, path))
}

fn tail_chars(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

fn head_chars(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub(crate) fn code_complete_prompt(request: &CodeCompleteRequest<'_>) -> String {
    format!(
        "File: {}\nLanguage: {}\n\n<before_cursor>\n{}</before_cursor><after_cursor>{}\n</after_cursor>",
        request.path,
        request.language,
        tail_chars(request.prefix, PREFIX_MAX_CHARS),
        head_chars(request.suffix, SUFFIX_MAX_CHARS),
    )
}

pub(crate) fn sanitize_completion(raw: &str, prefix: &str, suffix: &str) -> String {
    let mut text = raw.to_string();
    while let Some(start) = text.find("<think>") {
        match text[start..].find("</think>") {
            Some(end) => text.replace_range(start..start + end + "</think>".len(), ""),
            None => text.truncate(start),
        }
    }
    let trimmed = text.trim_start_matches('\n');
    let mut text = if trimmed.trim_start().starts_with("```") {
        let body = trimmed
            .trim_start()
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or("");
        body.rsplit_once("```")
            .map(|(inner, _)| inner)
            .unwrap_or(body)
            .trim_end_matches('\n')
            .to_string()
    } else {
        text.trim_end().to_string()
    };
    let current_line = prefix.rsplit('\n').next().unwrap_or("");
    if !current_line.trim().is_empty() && text.starts_with(current_line) {
        text = text[current_line.len()..].to_string();
    }
    let next_text = suffix.split('\n').next().unwrap_or("").trim();
    if !next_text.is_empty() && text.trim_end().ends_with(next_text) {
        let cut = text.trim_end().len() - next_text.len();
        text.truncate(cut);
    }
    text
}

impl AgentEngine {
    pub(crate) async fn generate_code_completion(
        &self,
        request: &CodeCompleteRequest<'_>,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        let settings = &config.code_completion;
        if !settings.enabled {
            anyhow::bail!("Tab completion is turned off");
        }
        if is_ignored_path(request.path, &settings.ignore_globs) {
            return Ok(String::new());
        }
        let model_override = settings
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let (provider_id, mut provider_config) = match settings
            .provider
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(provider_id) => (
                provider_id.to_string(),
                resolve_provider_config_for(&config, provider_id, model_override)?,
            ),
            None => {
                let (provider_id, mut resolved) =
                    crate::agent::coder::resolve_coder_provider(&config)?;
                if let Some(model) = model_override {
                    resolved.model = model.to_string();
                }
                (provider_id, resolved)
            }
        };
        provider_config.reasoning_effort = settings
            .reasoning_effort
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "off".to_string());
        provider_config.max_tokens = Some(COMPLETION_MAX_TOKENS);
        crate::agent::coder::ensure_answer_room(&mut provider_config, COMPLETION_MAX_TOKENS);
        drop(config);

        self.check_circuit_breaker(&provider_id).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(code_complete_prompt(request)),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            CODE_COMPLETE_SYSTEM_PROMPT,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: 0,
            },
        );
        let mut content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk? {
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
        Ok(sanitize_completion(
            &content,
            request.prefix,
            request.suffix,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_files_are_never_sent_for_completion() {
        let globs = crate::agent::types::default_code_completion_ignore_globs();
        assert!(is_ignored_path(".env", &globs));
        assert!(is_ignored_path("config/.env.production", &globs));
        assert!(is_ignored_path("certs/server.pem", &globs));
        assert!(is_ignored_path("/home/u/.ssh/id_rsa.pub", &globs));
        assert!(!is_ignored_path("src/environment.ts", &globs));
        assert!(!is_ignored_path("src/main.rs", &globs));
    }

    #[test]
    fn completion_never_duplicates_text_already_around_the_cursor() {
        assert_eq!(
            sanitize_completion(
                "    let total = sum(items);",
                "fn f() {\n    let total = ",
                "\n}"
            ),
            "sum(items);"
        );
        assert_eq!(
            sanitize_completion("items.len())", "let n = (", ")\nnext();"),
            "items.len()"
        );
    }

    #[test]
    fn completion_drops_fences_and_reasoning_from_chatty_models() {
        assert_eq!(
            sanitize_completion("<think>easy</think>```rust\nx + 1\n```", "let y = ", ""),
            "x + 1"
        );
        assert_eq!(sanitize_completion("<think>still thinking", "a", ""), "");
    }

    #[test]
    fn prompt_marks_the_cursor_and_keeps_text_nearest_to_it() {
        let prefix = "a".repeat(PREFIX_MAX_CHARS + 50) + "NEAR";
        let request = CodeCompleteRequest {
            path: "src/lib.rs",
            language: "rust",
            prefix: &prefix,
            suffix: "AFTER",
        };
        let prompt = code_complete_prompt(&request);
        assert!(prompt.contains("NEAR</before_cursor><after_cursor>AFTER"));
        assert!(prompt.len() < PREFIX_MAX_CHARS + 200);
    }
}
