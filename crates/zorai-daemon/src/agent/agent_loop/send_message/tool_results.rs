use super::*;

const OFFLOAD_SUMMARY_KEY_FINDING_LINES: usize = 3;
const OFFLOAD_SUMMARY_LINE_CHAR_LIMIT: usize = 160;
const OFFLOAD_JSON_ARRAY_FINDING_LIMIT: usize = 40;

/// Clip an oversized tool result to a bounded head+tail window on char
/// boundaries. Used only when persisting the payload (offload or preview file)
/// failed: keeping the full payload inline would re-inflate the context window
/// on every subsequent turn until compaction. The head preserves the primary
/// output and the tail preserves trailing errors/summaries, with a marker
/// noting the dropped bytes. Takes ownership so the no-clip path is zero-copy.
fn clip_inline_tool_result(raw_payload: String, max_bytes: usize) -> String {
    use zorai_shared::text::{ceil_char_boundary, floor_char_boundary};

    let head_budget = max_bytes.saturating_mul(3) / 4;
    let tail_budget = max_bytes.saturating_sub(head_budget);

    let head_end = floor_char_boundary(&raw_payload, head_budget);
    let tail_start =
        ceil_char_boundary(&raw_payload, raw_payload.len().saturating_sub(tail_budget));
    if tail_start <= head_end {
        return raw_payload;
    }
    let dropped = tail_start - head_end;

    let clipped = format!(
        "{}\n\n...[tool result clipped: {} bytes dropped; persisting to disk failed]...\n\n{}",
        &raw_payload[..head_end],
        dropped,
        &raw_payload[tail_start..]
    );

    // For pathologically small budgets the marker can exceed the savings; never
    // emit something larger than what we started with.
    if clipped.len() >= raw_payload.len() {
        raw_payload
    } else {
        clipped
    }
}
const TOOL_OUTPUT_PREVIEW_ALLOWLIST: &[&str] = &[
    zorai_protocol::tool_names::BASH_COMMAND,
    zorai_protocol::tool_names::PYTHON_EXECUTE,
    zorai_protocol::tool_names::RUN_TERMINAL_COMMAND,
    zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND,
    zorai_protocol::tool_names::WEB_SEARCH,
    zorai_protocol::tool_names::FETCH_URL,
];

fn summarize_offloaded_tool_result(
    result: &ToolResult,
    byte_size: usize,
    payload_id: &str,
) -> String {
    let status = if result.is_error { "error" } else { "done" };
    let key_findings = extract_offload_key_findings(&result.content);
    let findings_block = key_findings
        .into_iter()
        .map(|line| format!("  - {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Tool result offloaded\n- tool: {}\n- status: {}\n- bytes: {}\n- payload_id: {}\n- key findings:\n{}",
        result.name, status, byte_size, payload_id, findings_block
    )
}

fn summarize_preview_backed_tool_result(
    result: &ToolResult,
    byte_size: usize,
    preview_path: &str,
) -> String {
    let status = if result.is_error { "error" } else { "done" };
    let key_findings = extract_offload_key_findings(&result.content);
    let findings_block = key_findings
        .into_iter()
        .map(|line| format!("  - {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Tool result saved to preview file\n- tool: {}\n- status: {}\n- bytes: {}\n- path: {}\n- key findings:\n{}",
        result.name, status, byte_size, preview_path, findings_block
    )
}

fn should_persist_tool_output_preview(tool_name: &str) -> bool {
    TOOL_OUTPUT_PREVIEW_ALLOWLIST.contains(&tool_name)
}

fn should_include_argument_in_tool_output_preview(tool_name: &str) -> bool {
    matches!(
        tool_name,
        zorai_protocol::tool_names::BASH_COMMAND
            | zorai_protocol::tool_names::PYTHON_EXECUTE
            | zorai_protocol::tool_names::RUN_TERMINAL_COMMAND
            | zorai_protocol::tool_names::EXECUTE_MANAGED_COMMAND
    )
}

fn format_tool_output_preview_argument(raw_arguments: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw_arguments)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| raw_arguments.trim().to_string())
}

fn format_tool_output_preview_body(result: &ToolResult, tool_arguments: Option<&str>) -> String {
    if !should_include_argument_in_tool_output_preview(&result.name) {
        return result.content.clone();
    }

    let Some(tool_arguments) = tool_arguments
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return result.content.clone();
    };

    let argument = format_tool_output_preview_argument(tool_arguments);
    format!("argument:\n{argument}\n\nresult:\n{}", result.content)
}

fn extract_offload_key_findings(raw_payload: &str) -> Vec<String> {
    if let Some(findings) = json_array_key_findings(raw_payload) {
        return findings;
    }

    let findings = raw_payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(truncate_finding_line)
        .take(OFFLOAD_SUMMARY_KEY_FINDING_LINES)
        .collect::<Vec<_>>();

    if findings.is_empty() {
        vec!["(no non-empty lines)".to_string()]
    } else {
        findings
    }
}

fn json_array_key_findings(raw_payload: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(raw_payload).ok()?;
    let items = value.as_array()?;
    if items.is_empty() || !items.iter().all(serde_json::Value::is_object) {
        return None;
    }

    let mut findings = items
        .iter()
        .take(OFFLOAD_JSON_ARRAY_FINDING_LIMIT)
        .map(summarize_json_record)
        .collect::<Vec<_>>();
    if items.len() > OFFLOAD_JSON_ARRAY_FINDING_LIMIT {
        findings.push(format!(
            "+{} more",
            items.len() - OFFLOAD_JSON_ARRAY_FINDING_LIMIT
        ));
    }
    Some(findings)
}

fn summarize_json_record(value: &serde_json::Value) -> String {
    let Some(object) = value.as_object() else {
        return truncate_finding_line(&value.to_string());
    };
    let label = ["title", "name", "path", "id"]
        .into_iter()
        .find_map(|key| json_display(object.get(key)))
        .unwrap_or_else(|| "item".to_string());
    let mut parts = vec![label.clone()];
    for key in [
        "id",
        "agent_name",
        "agent",
        "provider",
        "model",
        "status",
        "kind",
        "role",
    ] {
        let Some(text) = json_display(object.get(key)) else {
            continue;
        };
        if text == label {
            continue;
        }
        parts.push(text);
    }
    if object.get("pinned").and_then(serde_json::Value::as_bool) == Some(true) {
        parts.push("pinned".to_string());
    }
    truncate_finding_line(&parts.join(" · "))
}

fn json_display(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value?;
    let text = match value {
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn truncate_finding_line(line: &str) -> String {
    let truncated: String = line.chars().take(OFFLOAD_SUMMARY_LINE_CHAR_LIMIT).collect();
    if line.chars().count() > OFFLOAD_SUMMARY_LINE_CHAR_LIMIT {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn build_handoff_restart_user_message(
    previous_agent_name: &str,
    next_agent_name: &str,
    original_user_message: &str,
    tool_arguments: &str,
) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(tool_arguments).ok()?;
    let reason = args
        .get("reason")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let summary = args
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let action = args
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or("push_handoff");

    let intro = if action == "return_handoff" {
        format!(
            "User requested you while talking to {previous_agent_name}. Control has returned to {next_agent_name}."
        )
    } else {
        format!(
            "User requested you while talking to {previous_agent_name}. You are now the active responder for this thread as {next_agent_name}."
        )
    };

    Some(format!(
        "{intro}\nHandoff reason: {reason}\nConversation summary from the previous responder: {summary}\nLatest operator request to answer now: {original_user_message}"
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedToolResultThreadMessage {
    pub content: String,
    pub offloaded_payload_id: Option<String>,
    pub tool_output_preview_path: Option<String>,
}

pub(crate) async fn prepare_tool_result_thread_message(
    engine: &AgentEngine,
    thread_id: &str,
    goal_run_id: Option<&str>,
    result: &ToolResult,
    created_at: u64,
) -> PreparedToolResultThreadMessage {
    prepare_tool_result_thread_message_with_arguments(
        engine,
        thread_id,
        goal_run_id,
        result,
        None,
        created_at,
    )
    .await
}

pub(crate) async fn prepare_tool_result_thread_message_with_arguments(
    engine: &AgentEngine,
    thread_id: &str,
    goal_run_id: Option<&str>,
    result: &ToolResult,
    tool_arguments: Option<&str>,
    created_at: u64,
) -> PreparedToolResultThreadMessage {
    let threshold_bytes = {
        engine
            .config
            .read()
            .await
            .offload_tool_result_threshold_bytes
    };
    let raw_payload = result.content.clone();
    let byte_size = raw_payload.as_bytes().len();

    if thread_id.trim().is_empty() {
        return PreparedToolResultThreadMessage {
            content: raw_payload,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
        };
    }

    if should_persist_tool_output_preview(&result.name) {
        let preview_path = engine.history.tool_output_preview_path(
            thread_id,
            goal_run_id,
            &result.name,
            created_at,
        );
        let persist_result: Result<()> = async {
            let parent = preview_path
                .parent()
                .context("tool output preview path missing parent directory")?;
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("create tool output preview directory {}", parent.display())
            })?;
            let preview_body = format_tool_output_preview_body(result, tool_arguments);
            tokio::fs::write(&preview_path, preview_body.as_bytes())
                .await
                .with_context(|| {
                    format!("write tool output preview file {}", preview_path.display())
                })?;
            Ok(())
        }
        .await;

        if let Err(error) = persist_result {
            tracing::warn!(
                thread_id = %thread_id,
                tool_name = %result.name,
                %error,
                "failed to persist tool output preview file; keeping inline content (clipped if over budget)"
            );
            let content = if threshold_bytes != 0 && byte_size > threshold_bytes {
                clip_inline_tool_result(raw_payload, threshold_bytes)
            } else {
                raw_payload
            };
            return PreparedToolResultThreadMessage {
                content,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
            };
        }

        let preview_path = preview_path.to_string_lossy().into_owned();
        let should_collapse = threshold_bytes != 0 && byte_size > threshold_bytes;
        return PreparedToolResultThreadMessage {
            content: if should_collapse {
                summarize_preview_backed_tool_result(result, byte_size, &preview_path)
            } else {
                raw_payload
            },
            offloaded_payload_id: None,
            tool_output_preview_path: Some(preview_path),
        };
    }

    if threshold_bytes == 0 || byte_size <= threshold_bytes {
        return PreparedToolResultThreadMessage {
            content: raw_payload,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
        };
    }

    let payload_id = uuid::Uuid::new_v4().to_string();
    let summary = summarize_offloaded_tool_result(result, byte_size, &payload_id);
    let payload_path = engine
        .history
        .offloaded_payload_path(thread_id, &payload_id);
    let persist_result: Result<()> = async {
        let parent = payload_path
            .parent()
            .context("offloaded payload path missing parent directory")?;
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create offloaded payload directory {}", parent.display()))?;
        tokio::fs::write(&payload_path, raw_payload.as_bytes())
            .await
            .with_context(|| format!("write offloaded payload file {}", payload_path.display()))?;
        if let Err(error) = engine
            .history
            .upsert_offloaded_payload_metadata(
                &payload_id,
                thread_id,
                &result.name,
                Some(result.tool_call_id.as_str()),
                "text/plain",
                byte_size as u64,
                &summary,
                created_at,
            )
            .await
        {
            let _ = tokio::fs::remove_file(&payload_path).await;
            return Err(error).context("persist offloaded payload metadata");
        }
        Ok(())
    }
    .await;

    if let Err(error) = persist_result {
        tracing::warn!(
            thread_id = %thread_id,
            tool_name = %result.name,
            %error,
            "failed to offload tool result payload; clipping inline content to configured budget"
        );
        return PreparedToolResultThreadMessage {
            content: clip_inline_tool_result(raw_payload, threshold_bytes),
            offloaded_payload_id: None,
            tool_output_preview_path: None,
        };
    }

    PreparedToolResultThreadMessage {
        content: summary,
        offloaded_payload_id: Some(payload_id),
        tool_output_preview_path: None,
    }
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn finalize_tool_call_result(
        &mut self,
        tc: &ToolCall,
        result: &ToolResult,
        args_summary: &str,
        args_hash: &str,
        now_epoch_secs: u64,
        decision_reasoning: Option<&str>,
    ) -> Result<ToolCallDisposition> {
        self.engine
            .persist_tool_selection_causal_trace(
                &self.tid,
                self.current_task_snapshot
                    .as_ref()
                    .and_then(|task| task.goal_run_id.as_deref()),
                self.task_id,
                tc,
                decision_reasoning,
                result,
                &self.trace_collector,
                &self.config,
                &self.provider_config,
            )
            .await;
        self.engine
            .record_provenance_event(
                "tool_call",
                "agent executed tool call",
                serde_json::json!({
                    "tool": tc.function.name.as_str(),
                    "arguments": tc.function.arguments.as_str(),
                    "is_error": result.is_error,
                }),
                self.current_task_snapshot
                    .as_ref()
                    .and_then(|task| task.goal_run_id.as_deref()),
                self.task_id,
                Some(self.tid.as_str()),
                None,
                None,
            )
            .await;

        self.engine
            .update_counter_who_on_tool_result(
                &self.tid,
                &tc.function.name,
                args_summary,
                !result.is_error,
            )
            .await;

        if result.is_error {
            self.engine
                .record_memory_graph_from_tool_failure(
                    &self.tid,
                    &tc.function.name,
                    &tc.function.arguments,
                    &result.content,
                )
                .await;
            let scope_hint = self
                .current_task_snapshot
                .as_ref()
                .map(|task| task.title.as_str())
                .or_else(|| {
                    self.current_task_snapshot
                        .as_ref()
                        .and_then(|task| task.goal_run_title.as_deref())
                });
            if let Err(error) = self
                .engine
                .record_negative_knowledge_from_tool_failure(
                    scope_hint,
                    &tc.function.name,
                    args_summary,
                    &result.content,
                )
                .await
            {
                tracing::warn!(
                    thread_id = %self.tid,
                    tool_name = %tc.function.name,
                    error = %error,
                    "failed to record immediate negative knowledge from tool failure"
                );
            }
        }

        let prepared_tool_result = prepare_tool_result_thread_message_with_arguments(
            self.engine,
            &self.tid,
            self.current_task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_id.as_deref()),
            result,
            Some(&tc.function.arguments),
            now_epoch_secs,
        )
        .await;
        let structural_refs = if result.is_error {
            Vec::new()
        } else {
            self.engine
                .enrich_thread_structural_memory_from_tool_result(
                    &self.tid,
                    &tc.function.name,
                    &tc.function.arguments,
                    Some(result.content.as_str()),
                )
                .await
        };
        let tool_result_content = prepared_tool_result.content.clone();
        let tool_result_name = result.name.clone();
        let tool_result_id = result.tool_call_id.clone();
        let tool_status = if result.is_error { "error" } else { "done" };
        let persisted_message_id = generate_message_id();

        let _ = self.engine.event_tx.send(AgentEvent::ToolResult {
            thread_id: self.tid.clone(),
            call_id: tool_result_id.clone(),
            name: tool_result_name.clone(),
            content: tool_result_content.clone(),
            is_error: result.is_error,
            weles_review: result.weles_review.clone(),
            message_id: Some(persisted_message_id.clone()),
        });

        {
            let mut threads = self.engine.threads.write().await;
            if let Some(thread) = threads.get_mut(&self.tid) {
                self.tool_side_effect_committed = true;
                thread.messages.push(AgentMessage {
                    id: persisted_message_id,
                    role: MessageRole::Tool,
                    content: tool_result_content,
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some(tool_result_id),
                    tool_name: Some(tool_result_name),
                    tool_arguments: Some(tc.function.arguments.clone()),
                    tool_status: Some(tool_status.to_string()),
                    weles_review: result.weles_review.clone(),
                    input_tokens: 0,
                    output_tokens: 0,
                    cost: None,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    author_agent_id: None,
                    author_agent_name: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: prepared_tool_result.offloaded_payload_id.clone(),
                    tool_output_preview_path: prepared_tool_result.tool_output_preview_path.clone(),
                    structural_refs,
                    pinned_for_compaction: false,
                    timestamp: now_millis(),
                    feedback: None,
                });
            }
        }
        self.emit_context_window_update_for_current_thread().await;

        let current_tokens = {
            let threads = self.engine.threads.read().await;
            threads
                .get(&self.tid)
                .map(|thread| {
                    crate::agent::subagent::context_budget::visible_output_budget_tokens(
                        &thread.messages,
                    )
                })
                .unwrap_or(0) as u32
        };
        if let Some(task) = self.current_task_snapshot.as_ref() {
            self.engine
                .record_subagent_tool_result(
                    task,
                    &self.tid,
                    &tc.function.name,
                    result.is_error,
                    current_tokens,
                )
                .await;
            self.engine.persist_subagent_runtime_metrics(&task.id).await;
        }

        let is_progress = !result.is_error && result.content.len() > 50;
        self.engine
            .record_awareness_outcome(
                &self.tid,
                "thread",
                &tc.function.name,
                args_hash,
                !result.is_error,
                is_progress,
            )
            .await;
        self.engine
            .check_awareness_mode_shift(&self.tid, &self.tid)
            .await;

        if let Some(task_id) = self.task_id {
            let task_snapshot = self.engine.task_by_id_for_turn_scope(task_id).await;
            if let Some(task_snapshot) = task_snapshot {
                let scope = policy_scope_for_task(&self.tid, &task_snapshot);
                if result.is_error
                    && self
                        .engine
                        .is_retry_guard_active(&scope, args_hash, now_epoch_secs)
                        .await
                {
                    if let super::orchestrator_policy::PolicyLoopAction::AbortRetry = self
                        .engine
                        .enforce_orchestrator_retry_guard(
                            &self.tid,
                            Some(task_id),
                            &scope,
                            args_hash,
                            now_epoch_secs,
                        )
                        .await?
                    {
                        self.policy_aborted_retry = true;
                        return Ok(ToolCallDisposition::BreakLoop);
                    }
                }

                if let Some(action) = apply_post_tool_policy_checkpoint(
                    self.engine,
                    &self.tid,
                    task_id,
                    &task_snapshot,
                    args_hash,
                    self.recent_policy_tool_outcomes.make_contiguous(),
                    now_epoch_secs,
                )
                .await?
                {
                    match action {
                        super::orchestrator_policy::PolicyLoopAction::Continue => {}
                        super::orchestrator_policy::PolicyLoopAction::RestartLoop => {
                            return Ok(ToolCallDisposition::RestartLoop);
                        }
                        super::orchestrator_policy::PolicyLoopAction::InterruptForApproval => {
                            self.interrupted_for_approval = true;
                            return Ok(ToolCallDisposition::BreakLoop);
                        }
                        super::orchestrator_policy::PolicyLoopAction::AbortRetry => {
                            self.policy_aborted_retry = true;
                            return Ok(ToolCallDisposition::BreakLoop);
                        }
                    }
                }
            }
        }

        if let Some(pending_approval) = result.pending_approval.as_ref() {
            self.engine
                .remember_pending_approval_command(pending_approval)
                .await;
            let _ = self
                .engine
                .record_operator_approval_requested(pending_approval)
                .await;
            self.engine
                .record_provenance_event(
                    "approval_requested",
                    "tool execution requested operator approval",
                    serde_json::json!({
                        "approval_id": pending_approval.approval_id,
                        "command": pending_approval.command,
                        "risk_level": pending_approval.risk_level,
                        "blast_radius": pending_approval.blast_radius,
                    }),
                    self.current_task_snapshot
                        .as_ref()
                        .and_then(|task| task.goal_run_id.as_deref()),
                    self.task_id,
                    Some(self.tid.as_str()),
                    Some(pending_approval.approval_id.as_str()),
                    None,
                )
                .await;
            self.interrupted_for_approval = true;
            if let Some(task_id) = self.task_id {
                if self
                    .engine
                    .auto_approve_task_if_rule_matches(task_id, &self.tid, pending_approval)
                    .await
                {
                    return Ok(ToolCallDisposition::BreakLoop);
                }
                self.engine
                    .mark_task_awaiting_approval(task_id, &self.tid, pending_approval)
                    .await;
            }
            let _ = self.engine.event_tx.send(AgentEvent::ApprovalRequired {
                thread_id: self.tid.clone(),
                approval_id: pending_approval.approval_id.clone(),
                command: pending_approval.command.clone(),
                rationale: Some(pending_approval.rationale.clone()),
                reasons: pending_approval.reasons.clone(),
                risk_level: pending_approval.risk_level.clone(),
                blast_radius: pending_approval.blast_radius.clone(),
            });
            return Ok(ToolCallDisposition::BreakLoop);
        }

        if !result.is_error && tc.function.name == zorai_protocol::tool_names::ASK_PARENT {
            self.interrupted_for_approval = true;
            return Ok(ToolCallDisposition::BreakLoop);
        }

        if self.stream_cancel_token.is_cancelled() {
            self.was_cancelled = true;
            return Ok(ToolCallDisposition::BreakLoop);
        }

        if !result.is_error && tc.function.name == zorai_protocol::tool_names::HANDOFF_THREAD_AGENT
        {
            let next_agent_name = self
                .engine
                .active_agent_id_for_thread(&self.tid)
                .await
                .map(|agent_id| canonical_agent_name(&agent_id).to_string())
                .unwrap_or_else(|| self.runtime_agent_name.clone());
            if let Some(llm_user_content) = build_handoff_restart_user_message(
                &self.runtime_agent_name,
                &next_agent_name,
                self.stored_user_content,
                &tc.function.arguments,
            ) {
                self.handoff_restart = Some(HandoffRestartRequest { llm_user_content });
            }
            let _ = self.engine.event_tx.send(AgentEvent::Done {
                thread_id: self.tid.clone(),
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: None,
                model: None,
                tps: None,
                generation_ms: None,
                reasoning: None,
                upstream_message: None,
                provider_final_result: None,
                message_id: None,
            });
            return Ok(ToolCallDisposition::BreakLoop);
        }

        Ok(ToolCallDisposition::ContinueTools)
    }
}

#[cfg(test)]
mod clip_tests {
    use super::*;

    #[test]
    fn clip_inline_tool_result_bounds_size_and_keeps_head_and_tail() {
        let payload = "HEAD-START".to_string() + &"x".repeat(5000) + "TAIL-END";
        let byte_size = payload.len();

        let clipped = clip_inline_tool_result(payload, 1024);

        assert!(clipped.len() < byte_size, "clipped output must be smaller");
        assert!(clipped.starts_with("HEAD-START"), "head is preserved");
        assert!(clipped.ends_with("TAIL-END"), "tail is preserved");
        assert!(clipped.contains("bytes dropped"), "drop marker present");
    }

    #[test]
    fn offload_key_findings_summarize_thread_records_instead_of_json_preamble() {
        let payload = serde_json::to_string_pretty(&serde_json::json!([
            {
                "id": "thread_a",
                "title": "Heartbeat",
                "agent_name": "Svarog",
                "pinned": true
            },
            {
                "id": "thread_b",
                "title": "SEPIQ",
                "agent_name": "Rarog",
                "pinned": false
            }
        ]))
        .expect("thread list json");

        assert_eq!(
            extract_offload_key_findings(&payload),
            vec![
                "Heartbeat · thread_a · Svarog · pinned".to_string(),
                "SEPIQ · thread_b · Rarog".to_string(),
            ]
        );
    }

    #[test]
    fn offload_key_findings_cap_long_json_arrays_and_keep_plain_text_short() {
        let items = (0..45)
            .map(|index| serde_json::json!({ "title": format!("T{index}"), "id": format!("id-{index}") }))
            .collect::<Vec<_>>();
        let findings =
            extract_offload_key_findings(&serde_json::to_string(&items).expect("json array"));
        assert_eq!(findings.len(), OFFLOAD_JSON_ARRAY_FINDING_LIMIT + 1);
        assert_eq!(findings[0], "T0 · id-0");
        assert_eq!(findings.last().map(String::as_str), Some("+5 more"));

        assert_eq!(
            extract_offload_key_findings("one\ntwo\nthree\nfour\n"),
            vec!["one".to_string(), "two".to_string(), "three".to_string()]
        );
    }

    #[test]
    fn clip_inline_tool_result_respects_utf8_char_boundaries() {
        // Multi-byte chars at both clip edges must not be split.
        let payload = "é".repeat(4000);

        let clipped = clip_inline_tool_result(payload, 500);

        // Round-tripping through chars proves no byte split occurred.
        assert_eq!(
            clipped,
            String::from_utf8(clipped.clone().into_bytes()).unwrap()
        );
        assert!(clipped.contains("bytes dropped"));
    }
}
