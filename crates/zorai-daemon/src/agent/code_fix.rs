use super::*;
use futures::StreamExt;

const CODE_FIX_CONTEXT_MAX_CHARS: usize = 12_000;
const CODE_FIX_TARGET_MAX_CHARS: usize = 20_000;
const CODE_FIX_DIAGNOSTICS_MAX: usize = 20;
const CODE_FIX_ANSWER_TOKENS: u32 = 4096;

const CODE_FIX_SYSTEM_PROMPT: &str =
    "You fix code inside an editor. You get the code before the target, the TARGET block, \
the code after it, any compiler or linter diagnostics, and possibly the developer's instruction. Reply with only the corrected TARGET block: \
keep its indentation and line structure, change nothing outside it, and do not repeat the surrounding code. \
If an instruction is given, do exactly what it asks within the target. \
Otherwise, if diagnostics are given, fix them; otherwise fix the most likely bug or clear problem in the target. \
If the target is empty, write new code for that spot from the instruction. If nothing needs to change, return the target unchanged. No markdown fences, no explanation.";

pub(crate) struct CodeFixRequest<'a> {
    pub path: &'a str,
    pub language: &'a str,
    pub before: &'a str,
    pub target: &'a str,
    pub after: &'a str,
    pub diagnostics: &'a [String],
    pub instruction: &'a str,
    pub reasoning: Option<bool>,
    pub images: &'a [String],
}

const CODE_FIX_MAX_IMAGES: usize = 4;

pub(crate) fn code_fix_content(request: &CodeFixRequest<'_>) -> ApiContent {
    let images: Vec<&String> = request.images.iter().filter(|image| image.starts_with("data:image/")).take(CODE_FIX_MAX_IMAGES).collect();
    if images.is_empty() {
        return ApiContent::Text(code_fix_prompt(request));
    }
    let mut blocks = vec![serde_json::json!({
        "type": "input_text",
        "text": format!("{}\n\nThe attached image(s) show the UI or a sketch of the change the developer wants; use them to decide what the target should become.", code_fix_prompt(request)),
    })];
    blocks.extend(images.into_iter().map(|image| serde_json::json!({ "type": "input_image", "image_url": image })));
    ApiContent::Blocks(blocks)
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

pub(crate) fn code_fix_prompt(request: &CodeFixRequest<'_>) -> String {
    let mut prompt = format!("File: {}\nLanguage: {}\n\n", request.path, request.language);
    let instruction = request.instruction.trim();
    if !instruction.is_empty() {
        prompt.push_str("Instruction: ");
        prompt.push_str(instruction);
        prompt.push_str("\n\n");
    }
    let diagnostics: Vec<&str> = request
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.trim())
        .filter(|diagnostic| !diagnostic.is_empty())
        .take(CODE_FIX_DIAGNOSTICS_MAX)
        .collect();
    if !diagnostics.is_empty() {
        prompt.push_str("Diagnostics:\n");
        for diagnostic in diagnostics {
            prompt.push_str("- ");
            prompt.push_str(diagnostic);
            prompt.push('\n');
        }
        prompt.push('\n');
    }
    prompt.push_str("<before>\n");
    prompt.push_str(tail_chars(request.before, CODE_FIX_CONTEXT_MAX_CHARS));
    prompt.push_str("\n</before>\n<target>\n");
    prompt.push_str(request.target);
    prompt.push_str("\n</target>\n<after>\n");
    prompt.push_str(head_chars(request.after, CODE_FIX_CONTEXT_MAX_CHARS));
    prompt.push_str("\n</after>");
    prompt
}

pub(crate) fn sanitize_code_fix(raw: &str) -> Option<String> {
    let without_thinking = strip_thinking(raw);
    let mut text = without_thinking.trim_matches(|c| c == '\n' || c == '\r');
    if text.trim_start().starts_with("```") {
        let body = text.trim_start();
        text = body.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        let trimmed = text.trim_end();
        text = trimmed.strip_suffix("```").unwrap_or(trimmed);
        text = text.trim_matches(|c| c == '\n' || c == '\r');
    }
    for tag in ["<target>", "</target>"] {
        text = text.trim_start_matches(tag).trim_end_matches(tag);
    }
    let text = text.trim_matches(|c| c == '\n' || c == '\r');
    (!text.trim().is_empty()).then(|| text.to_string())
}

fn strip_thinking(raw: &str) -> String {
    let mut text = raw.to_string();
    while let Some(start) = text.find("<think>") {
        match text[start..].find("</think>") {
            Some(end) => text.replace_range(start..start + end + "</think>".len(), ""),
            None => text.truncate(start),
        }
    }
    text
}

impl AgentEngine {
    pub(crate) async fn generate_code_fix(
        &self,
        request: &CodeFixRequest<'_>,
        on_delta: Option<tokio::sync::mpsc::UnboundedSender<(String, bool)>>,
    ) -> Result<String> {
        if request.target.trim().is_empty() && request.instruction.trim().is_empty() {
            anyhow::bail!("there is no code to fix; describe what to write");
        }
        if request.target.len() > CODE_FIX_TARGET_MAX_CHARS {
            anyhow::bail!("the selection is too large for a quick fix; select a smaller block");
        }
        let config = self.config.read().await.clone();
        let (provider_id, mut provider_config) =
            crate::agent::coder::resolve_coder_provider(&config)?;
        match request.reasoning {
            Some(false) => provider_config.reasoning_effort = "off".to_string(),
            Some(true)
                if crate::agent::llm_client::anthropic_thinking_budget(
                    &provider_config.reasoning_effort,
                )
                .is_none() =>
            {
                provider_config.reasoning_effort = "medium".to_string();
            }
            _ => {}
        }
        crate::agent::coder::ensure_answer_room(&mut provider_config, CODE_FIX_ANSWER_TOKENS);
        let system_prompt =
            crate::agent::coder::coder_system_prompt(&config, CODE_FIX_SYSTEM_PROMPT);
        drop(config);

        self.check_circuit_breaker(&provider_id).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: code_fix_content(request),
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
        let mut reasoned = false;
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
                CompletionChunk::Delta {
                    content: delta,
                    reasoning,
                } => {
                    if let Some(thought) = reasoning.filter(|text| !text.is_empty()) {
                        reasoned = true;
                        if let Some(sender) = &on_delta {
                            let _ = sender.send((thought, true));
                        }
                    }
                    if let Some(sender) = on_delta.as_ref().filter(|_| !delta.is_empty()) {
                        let _ = sender.send((delta.clone(), false));
                    }
                    content.push_str(&delta);
                }
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
        sanitize_code_fix(&content).ok_or_else(|| {
            if reasoned {
                anyhow::anyhow!("Leszy used its whole output on reasoning and wrote no code; retry, or lower Leszy's reasoning level")
            } else {
                anyhow::anyhow!("Leszy returned an empty fix")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(
        before: &'a str,
        target: &'a str,
        after: &'a str,
        diagnostics: &'a [String],
    ) -> CodeFixRequest<'a> {
        CodeFixRequest {
            path: "src/main.rs",
            language: "rust",
            before,
            target,
            after,
            diagnostics,
            instruction: "",
            reasoning: None,
            images: &[],
        }
    }

    #[test]
    fn screenshots_and_sketches_travel_as_image_blocks_and_junk_is_dropped() {
        let images = vec!["data:image/png;base64,iVBORw0K".to_string(), "https://evil.example/x.png".to_string()];
        match code_fix_content(&CodeFixRequest { images: &images, ..request("", "<div />", "", &[]) }) {
            ApiContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert_eq!(blocks[1]["type"], "input_image");
                assert!(blocks[0]["text"].as_str().unwrap().contains("attached image"));
            }
            ApiContent::Text(_) => panic!("expected image blocks"),
        }
        assert!(matches!(code_fix_content(&request("", "x", "", &[])), ApiContent::Text(_)));
    }

    #[test]
    fn prompt_leads_with_the_developers_instruction_so_it_overrides_the_default_fix() {
        let prompt = code_fix_prompt(&CodeFixRequest {
            instruction: "  rename x to total  ",
            ..request("", "let x = 1;", "", &[])
        });
        assert!(prompt.starts_with(
            "File: src/main.rs\nLanguage: rust\n\nInstruction: rename x to total\n\n<before>"
        ));
    }

    #[test]
    fn prompt_puts_diagnostics_first_and_fences_the_target_so_the_model_knows_what_to_return() {
        let diagnostics = vec!["E0308: mismatched types".to_string(), " ".to_string()];
        let prompt = code_fix_prompt(&request(
            "fn a() {",
            "    let x: i32 = \"1\";",
            "}",
            &diagnostics,
        ));
        assert!(prompt.starts_with("File: src/main.rs\nLanguage: rust\n\nDiagnostics:\n- E0308: mismatched types\n\n<before>\n"));
        assert!(prompt.contains("<target>\n    let x: i32 = \"1\";\n</target>"));
        assert!(prompt.ends_with("<after>\n}\n</after>"));
    }

    #[test]
    fn prompt_keeps_the_context_nearest_the_target_when_files_are_huge() {
        let before = format!("{}NEAR_BEFORE", "x".repeat(CODE_FIX_CONTEXT_MAX_CHARS));
        let after = format!("NEAR_AFTER{}", "é".repeat(CODE_FIX_CONTEXT_MAX_CHARS));
        let prompt = code_fix_prompt(&request(&before, "t", &after, &[]));
        assert!(prompt.contains("NEAR_BEFORE\n</before>"));
        assert!(prompt.contains("<after>\nNEAR_AFTER"));
        assert!(prompt.len() < before.len() + after.len());
    }

    #[test]
    fn sanitize_keeps_leading_indentation_because_the_edit_replaces_whole_lines() {
        assert_eq!(
            sanitize_code_fix("```rust\n    let x = 1;\n    y();\n```").as_deref(),
            Some("    let x = 1;\n    y();")
        );
        assert_eq!(
            sanitize_code_fix("\n    a();\n").as_deref(),
            Some("    a();")
        );
        assert_eq!(
            sanitize_code_fix("<target>\n  b();\n</target>").as_deref(),
            Some("  b();")
        );
        assert_eq!(sanitize_code_fix(" \n "), None);
    }

    #[test]
    fn sanitize_drops_inline_thinking_so_only_the_code_is_applied() {
        assert_eq!(
            sanitize_code_fix("<think>The user wants x renamed.</think>\n    let total = 1;")
                .as_deref(),
            Some("    let total = 1;")
        );
        assert_eq!(sanitize_code_fix("<think>still thinking"), None);
    }
}
