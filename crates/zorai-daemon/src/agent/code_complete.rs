use super::*;
use futures::StreamExt;

const PREFIX_MAX_CHARS: usize = 6_000;
const SUFFIX_MAX_CHARS: usize = 3_000;
const COMPLETION_MAX_TOKENS: u32 = 256;
const CODE_COMPLETE_SYSTEM_PROMPT: &str = "You are an inline code completion engine inside an editor. \
You get the code before the cursor and the code after it. Reply with only the text to insert at the cursor: \
do not repeat code that is already before or after the cursor, no explanation, no markdown fences. \
Keep it short: finish the current line or statement, at most a few lines. If nothing sensible fits, reply with nothing.";
const NEXT_EDIT_SYSTEM_PROMPT: &str = "You predict a developer's next edit in a code editor. \
You get the edits they just made and the code around their cursor, with line numbers. \
If those edits clearly imply one more change elsewhere in the shown code, such as renaming the remaining uses of a renamed symbol \
or updating a call to match a changed signature, reply with `LINES <first>-<last>` naming the contiguous block to replace (at most 12 lines, \
use the same number twice for one line) followed by the complete new text of that block. \
Otherwise reply with exactly `NONE`. Never explain.";
const NEXT_EDIT_MAX_LINES: usize = 12;
const NEXT_EDIT_WINDOW_LINES: usize = 60;
const RECENT_EDITS_MAX: usize = 5;

static IN_FLIGHT: std::sync::Mutex<Option<tokio::task::AbortHandle>> = std::sync::Mutex::new(None);
static NEXT_EDIT_IN_FLIGHT: std::sync::Mutex<Option<tokio::task::AbortHandle>> =
    std::sync::Mutex::new(None);

pub(crate) fn supersede_in_flight(next_edit: bool, handle: tokio::task::AbortHandle) {
    let slot = if next_edit {
        &NEXT_EDIT_IN_FLIGHT
    } else {
        &IN_FLIGHT
    };
    if let Ok(mut slot) = slot.lock() {
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
    pub recent_edits: &'a [String],
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

fn strip_thinking(text: &str) -> String {
    let mut text = text.to_string();
    while let Some(start) = text.find("<think>") {
        match text[start..].find("</think>") {
            Some(end) => text.replace_range(start..start + end + "</think>".len(), ""),
            None => text.truncate(start),
        }
    }
    text
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
    let text = strip_thinking(raw);
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

pub(crate) fn next_edit_prompt(request: &CodeCompleteRequest<'_>) -> String {
    let cursor_line = request.prefix.matches('\n').count() + 1;
    let text = format!("{}{}", request.prefix, request.suffix);
    let lines: Vec<&str> = text.split('\n').collect();
    let first = cursor_line.saturating_sub(NEXT_EDIT_WINDOW_LINES).max(1);
    let last = (cursor_line + NEXT_EDIT_WINDOW_LINES).min(lines.len());
    let mut prompt = format!(
        "File: {}\nLanguage: {}\nCursor line: {cursor_line}\n\nRecent edits (oldest first):\n",
        request.path, request.language
    );
    let skip = request.recent_edits.len().saturating_sub(RECENT_EDITS_MAX);
    for edit in request.recent_edits.iter().skip(skip) {
        prompt.push_str("- ");
        prompt.push_str(edit.trim());
        prompt.push('\n');
    }
    prompt.push_str("\nCode:\n");
    for number in first..=last {
        prompt.push_str(&format!("{number:>5} | {}\n", lines[number - 1]));
    }
    prompt
}

pub(crate) fn parse_next_edit(raw: &str, line_count: usize) -> Option<(usize, usize, String)> {
    let cleaned = strip_thinking(raw).replace("```", "");
    let mut lines = cleaned.trim().lines();
    let header = lines.next()?.trim();
    let (first, last) = if let Some(range) = header.strip_prefix("LINES") {
        let (first, last) = range.trim().split_once('-')?;
        (first.trim().parse().ok()?, last.trim().parse().ok()?)
    } else {
        let number: usize = header.strip_prefix("LINE")?.trim().parse().ok()?;
        (number, number)
    };
    if first == 0 || last < first || last > line_count || last - first >= NEXT_EDIT_MAX_LINES {
        return None;
    }
    let body: Vec<&str> = lines.map(str::trim_end).collect();
    let keep = if last == first {
        body.len().min(1)
    } else {
        body.len().min(NEXT_EDIT_MAX_LINES * 2)
    };
    Some((first, last, body[..keep].join("\n")))
}

impl AgentEngine {
    pub(crate) async fn predict_next_edit(
        &self,
        request: &CodeCompleteRequest<'_>,
    ) -> Result<String> {
        if request.recent_edits.is_empty() {
            return Ok(String::new());
        }
        let raw = self
            .run_completion_prompt(request, NEXT_EDIT_SYSTEM_PROMPT, next_edit_prompt(request))
            .await?;
        let line_count = format!("{}{}", request.prefix, request.suffix)
            .split('\n')
            .count();
        Ok(parse_next_edit(&raw, line_count)
            .map(|(line, end_line, replacement)| {
                serde_json::json!({ "line": line, "end_line": end_line, "replacement": replacement })
                    .to_string()
            })
            .unwrap_or_default())
    }

    pub(crate) async fn generate_code_completion(
        &self,
        request: &CodeCompleteRequest<'_>,
    ) -> Result<String> {
        let raw = self
            .run_completion_prompt(
                request,
                CODE_COMPLETE_SYSTEM_PROMPT,
                code_complete_prompt(request),
            )
            .await?;
        Ok(sanitize_completion(&raw, request.prefix, request.suffix))
    }

    async fn run_completion_prompt(
        &self,
        request: &CodeCompleteRequest<'_>,
        system_prompt: &str,
        user_prompt: String,
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
            content: ApiContent::Text(user_prompt),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            system_prompt,
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
        Ok(content)
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
            recent_edits: &[],
        };
        let prompt = code_complete_prompt(&request);
        assert!(prompt.contains("NEAR</before_cursor><after_cursor>AFTER"));
        assert!(prompt.len() < PREFIX_MAX_CHARS + 200);
    }

    #[test]
    fn next_edit_only_accepts_lines_inside_the_file() {
        assert_eq!(
            parse_next_edit("LINE 3\n    let user_name = name;", 10),
            Some((3, 3, "    let user_name = name;".to_string()))
        );
        assert_eq!(
            parse_next_edit("<think>rename</think>```\nLINE 2\nfoo()\n```", 5),
            Some((2, 2, "foo()".to_string()))
        );
        assert_eq!(parse_next_edit("NONE", 10), None);
        assert_eq!(parse_next_edit("LINE 99\nx", 10), None);
        assert_eq!(parse_next_edit("Sure! Here is the edit", 10), None);
    }

    #[test]
    fn next_edit_prompt_numbers_lines_and_keeps_only_the_latest_edits() {
        let edits: Vec<String> = (1..=7).map(|n| format!("edit {n}")).collect();
        let request = CodeCompleteRequest {
            path: "src/lib.rs",
            language: "rust",
            prefix: "fn a() {\n    let user_name",
            suffix: " = 1;\n}\n",
            recent_edits: &edits,
        };
        let prompt = next_edit_prompt(&request);
        assert!(prompt.contains("Cursor line: 2"));
        assert!(prompt.contains("    2 |     let user_name = 1;"));
        assert!(!prompt.contains("edit 2\n"));
        assert!(prompt.contains("edit 3\n") && prompt.contains("edit 7\n"));
    }

    #[test]
    fn next_edit_replies_name_a_bounded_block_inside_the_shown_code() {
        assert_eq!(
            parse_next_edit("LINE 4\n    total += user_name;\nextra", 10),
            Some((4, 4, "    total += user_name;".to_string()))
        );
        assert_eq!(
            parse_next_edit("LINES 2-3\nfn b(name: &str) {\n    name.len()\n}", 10),
            Some((2, 3, "fn b(name: &str) {\n    name.len()\n}".to_string()))
        );
        assert_eq!(parse_next_edit("LINES 3-2\nx", 10), None);
        assert_eq!(parse_next_edit("LINES 9-11\nx", 10), None);
        assert_eq!(parse_next_edit("LINES 1-20\nx", 30), None);
        assert_eq!(parse_next_edit("NONE", 10), None);
    }
}
