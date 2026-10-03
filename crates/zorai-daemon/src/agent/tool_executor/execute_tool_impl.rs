use super::*;
async fn maybe_emit_cli_wrapper_synthesis_proposal_notice(
    agent: &AgentEngine,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    dedupe_hint: &str,
    message: String,
    details: serde_json::Value,
) {
    if thread_id.trim().is_empty() {
        return;
    }

    let tool_synthesis_enabled = agent.config.read().await.tool_synthesis.enabled;
    if !tool_synthesis_enabled {
        return;
    }

    let dedupe_key = format!("{thread_id}::{dedupe_hint}");
    {
        let mut notices = agent.tool_synthesis_gap_notices.write().await;
        if !notices.insert(dedupe_key) {
            return;
        }
    }

    let _ = event_tx.send(AgentEvent::WorkflowNotice {
        thread_id: thread_id.to_string(),
        kind: "tool-synthesis-proposal".to_string(),
        message,
        details: Some(details.to_string()),
    });
}

async fn maybe_emit_existing_tool_status_notice(
    agent: &AgentEngine,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    dedupe_hint: &str,
    proposal_tool_name: &str,
    proposal_target: &str,
    proposal_kind: &str,
    existing: &serde_json::Value,
    source_reason: &str,
) {
    let status = existing
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("active");
    let id = existing
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or(proposal_tool_name);
    let gap_label = match proposal_kind {
        "openapi" => "OpenAPI gap",
        _ => "CLI gap",
    };
    let (message, recommended_action) = match status {
        "new" => (
            format!(
                "Equivalent generated tool `{id}` already exists for this {gap_label}. Activate it instead of synthesizing a duplicate."
            ),
            tool_names::ACTIVATE_GENERATED_TOOL,
        ),
        "promotable" => (
            format!(
                "Equivalent generated tool `{id}` is already promotable. Prefer using or promoting it instead of synthesizing a duplicate."
            ),
            tool_names::PROMOTE_GENERATED_TOOL,
        ),
        "promoted" => (
            format!(
                "Equivalent generated tool `{id}` is already promoted. Reuse it instead of synthesizing a duplicate."
            ),
            "use_existing_generated_tool",
        ),
        _ => (
            format!(
                "Equivalent generated tool `{id}` is already active. Reuse it instead of synthesizing a duplicate."
            ),
            "use_existing_generated_tool",
        ),
    };
    let details = serde_json::json!({
        "reason": "existing_equivalent_generated_tool",
        "source_reason": source_reason,
        "recommended_action": recommended_action,
        "existing_tool": existing,
        "proposal_kind": proposal_kind,
        "target": proposal_target,
    });
    let status_notice_dedupe_hint = format!("{dedupe_hint}::{id}::{status}");
    maybe_emit_cli_wrapper_synthesis_proposal_notice(
        agent,
        event_tx,
        thread_id,
        &status_notice_dedupe_hint,
        message,
        details,
    )
    .await;
}

async fn strongest_repeated_shell_fallback_evidence(
    agent: &AgentEngine,
    tool_name: &str,
) -> Option<(String, u64)> {
    let model = agent.operator_model.read().await;
    model
        .implicit_feedback
        .fallback_histogram
        .iter()
        .filter_map(|(pair, count)| {
            if *count < 2 {
                return None;
            }
            let (_, to_tool) = pair.split_once("->")?;
            if to_tool.trim().eq_ignore_ascii_case(tool_name) {
                Some((pair.clone(), *count))
            } else {
                None
            }
        })
        .max_by_key(|(_, count)| *count)
}

async fn maybe_emit_unknown_tool_synthesis_proposal_notice(
    agent: &AgentEngine,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    missing_tool: &str,
) {
    let Some(proposal) = detect_cli_wrapper_synthesis_proposal(missing_tool) else {
        return;
    };
    if let Some(existing) =
        find_equivalent_generated_cli_tool(&agent.data_dir, &proposal).unwrap_or(None)
    {
        if existing
            .get("status")
            .and_then(|value| value.as_str())
            .is_none_or(|status| status != "new")
        {
            return;
        }
        maybe_emit_existing_tool_status_notice(
            agent,
            event_tx,
            thread_id,
            &format!("existing-unknown::{missing_tool}"),
            &proposal.tool_name,
            &proposal.target,
            "cli",
            &existing,
            "unknown_tool_safe_cli_gap",
        )
        .await;
        return;
    }
    let synthesize_args = serde_json::json!({
        "kind": "cli",
        "target": proposal.target,
        "name": proposal.tool_name,
        "activate": false,
    });
    let message = format!(
        "Missing capability around `{missing_tool}` looks like a conservative CLI-wrapper gap. Proposal ready via synthesize_tool."
    );
    let details = serde_json::json!({
        "reason": "unknown_tool_safe_cli_gap",
        "missing_tool": missing_tool,
        "proposal_kind": "cli",
        "synthesize_tool_args": synthesize_args,
    });
    maybe_emit_cli_wrapper_synthesis_proposal_notice(
        agent,
        event_tx,
        thread_id,
        &format!("unknown::{missing_tool}"),
        message,
        details,
    )
    .await;
}

async fn maybe_emit_successful_shell_synthesis_proposal_notice(
    agent: &AgentEngine,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) {
    let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
        return;
    };
    let Some(proposal) = detect_cli_wrapper_synthesis_proposal_from_command(command) else {
        return;
    };
    if let Some(existing) =
        find_equivalent_generated_cli_tool(&agent.data_dir, &proposal).unwrap_or(None)
    {
        if existing
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| status == "new")
        {
            return;
        }
        maybe_emit_existing_tool_status_notice(
            agent,
            event_tx,
            thread_id,
            &format!("existing-shell::{command}"),
            &proposal.tool_name,
            &proposal.target,
            "cli",
            &existing,
            "successful_safe_shell_cli_gap",
        )
        .await;
        return;
    }
    let synthesize_args = serde_json::json!({
        "kind": "cli",
        "target": proposal.target,
        "name": proposal.tool_name,
        "activate": false,
    });
    let repeated_fallback = strongest_repeated_shell_fallback_evidence(agent, tool_name).await;
    let (reason, dedupe_hint, message, details) = if let Some((pair, count)) = repeated_fallback {
        (
            "repeated_safe_shell_fallback_cli_gap",
            format!("repeated-shell::{command}"),
            format!(
                "Repeated successful fallback through `{tool_name}` suggests a conservative CLI-wrapper gap. Proposal ready via synthesize_tool."
            ),
            serde_json::json!({
                "reason": "repeated_safe_shell_fallback_cli_gap",
                "missing_tool": tool_name,
                "proposal_kind": "cli",
                "synthesize_tool_args": synthesize_args,
                "matched_fallback": pair,
                "fallback_count": count,
            }),
        )
    } else {
        (
            "successful_safe_shell_cli_gap",
            format!("shell::{command}"),
            format!(
                "Missing capability around `{tool_name}` looks like a conservative CLI-wrapper gap. Proposal ready via synthesize_tool."
            ),
            serde_json::json!({
                "reason": "successful_safe_shell_cli_gap",
                "missing_tool": tool_name,
                "proposal_kind": "cli",
                "synthesize_tool_args": synthesize_args,
            }),
        )
    };
    let _ = reason;
    maybe_emit_cli_wrapper_synthesis_proposal_notice(
        agent,
        event_tx,
        thread_id,
        &dedupe_hint,
        message,
        details,
    )
    .await;
}

async fn maybe_emit_openapi_synthesis_proposal_notice(
    agent: &AgentEngine,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    args: &serde_json::Value,
    content: &str,
) {
    if thread_id.trim().is_empty() {
        return;
    }
    if !agent.config.read().await.tool_synthesis.enabled {
        return;
    }
    let Some(url) = args
        .get("url")
        .and_then(|value| value.as_str())
        .map(str::trim)
    else {
        return;
    };
    if url.is_empty() {
        return;
    }

    let saved_to = content
        .lines()
        .find_map(|line| line.strip_prefix("- saved_to: "))
        .map(str::trim);
    let body = if let Some(saved_path) = saved_to {
        match tokio::fs::read_to_string(saved_path).await {
            Ok(saved) => saved.trim().to_string(),
            Err(_) => return,
        }
    } else if let Some((_, body)) = content.split_once("\n\n") {
        body.trim().to_string()
    } else if content.starts_with("HTTP ") {
        content
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        content.trim().to_string()
    };
    let Ok(spec) = serde_json::from_str::<serde_json::Value>(&body) else {
        return;
    };
    let Some(paths) = spec.get("paths").and_then(|value| value.as_object()) else {
        return;
    };

    let mut selected_path = None::<String>;
    let mut selected_operation_id = None::<String>;
    for (path, item) in paths {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        for (method, operation) in item_obj {
            if method.to_ascii_uppercase() != "GET" {
                continue;
            }
            selected_path = Some(path.clone());
            selected_operation_id = operation
                .get("operationId")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            break;
        }
        if selected_path.is_some() {
            break;
        }
    }
    let Some(selected_path) = selected_path else {
        return;
    };

    let tool_name = selected_operation_id
        .as_deref()
        .map(|value| {
            value
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .trim_matches('_')
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            selected_path
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
                .trim_matches('_')
                .to_string()
        });

    let dedupe_hint = format!("openapi::{url}");
    if let Some(existing) = find_equivalent_generated_openapi_tool(
        &agent.data_dir,
        url,
        selected_operation_id.as_deref(),
    )
    .unwrap_or(None)
    {
        maybe_emit_existing_tool_status_notice(
            agent,
            event_tx,
            thread_id,
            &format!("existing-openapi::{url}"),
            &tool_name,
            url,
            "openapi",
            &existing,
            "fetched_openapi_get_gap",
        )
        .await;
        return;
    }
    let synthesize_args = serde_json::json!({
        "kind": "openapi",
        "target": url,
        "name": tool_name,
        "operation_id": selected_operation_id,
        "activate": false,
    });
    let message = format!(
        "Fetched content from `{url}` looks like a conservative GET OpenAPI surface. Proposal ready via synthesize_tool."
    );
    let details = serde_json::json!({
        "reason": "fetched_openapi_get_gap",
        "proposal_kind": "openapi",
        "target": url,
        "selected_path": selected_path,
        "operation_id": selected_operation_id,
        "synthesize_tool_args": synthesize_args,
    });
    maybe_emit_cli_wrapper_synthesis_proposal_notice(
        agent,
        event_tx,
        thread_id,
        &dedupe_hint,
        message,
        details,
    )
    .await;
}

fn should_scrub_successful_tool_result(tool_name: &str) -> bool {
    !matches!(tool_name, tool_names::READ_OFFLOADED_PAYLOAD)
}

fn sanitize_broadcast_mentions(message: &str) -> Option<String> {
    let sanitized = message
        .replace("@everyone", "everyone")
        .replace("@here", "here");
    if sanitized == message {
        None
    } else {
        Some(sanitized)
    }
}

fn critique_requests_operator_window(critique_modifications: &[String]) -> bool {
    critique_modifications.iter().any(|modification| {
        let normalized = modification.trim().to_ascii_lowercase();
        normalized.contains("typical working window")
            || normalized.contains("typical active window")
            || normalized.contains("schedule this background task")
            || normalized.contains("schedule this task")
    })
}

fn critique_requests_sensitive_file_narrowing(critique_modifications: &[String]) -> bool {
    critique_modifications.iter().any(|modification| {
        let normalized = modification.trim().to_ascii_lowercase();
        normalized.contains("narrow the sensitive file path")
            || normalized.contains("narrow any sensitive file path")
            || normalized.contains("minimal basename")
            || normalized.contains("sensitive file path")
    })
}

fn next_operator_window_timestamp_ms(now_ms: u64, preferred_hour_utc: u8) -> u64 {
    const HOUR_MS: u64 = 3_600_000;
    const DAY_MS: u64 = 24 * HOUR_MS;

    let day_start = now_ms - (now_ms % DAY_MS);
    let candidate = day_start + u64::from(preferred_hour_utc.min(23)) * HOUR_MS;
    if candidate > now_ms {
        candidate
    } else {
        candidate + DAY_MS
    }
}

fn critique_requests_narrower_subagent_scope(critique_modifications: &[String]) -> bool {
    critique_modifications.iter().any(|modification| {
        let normalized = modification.trim().to_ascii_lowercase();
        normalized.contains("smaller tool-call budget")
            || normalized.contains("tool-call budget")
            || normalized.contains("wall-clock window")
            || normalized.contains("reduce permissions")
            || normalized.contains("narrow delegated scope")
    })
}

fn upsert_budget_limit(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    limit: u64,
) -> bool {
    let budget = map
        .entry("budget".to_string())
        .or_insert_with(|| serde_json::Value::Object(Default::default()));
    let Some(budget_map) = budget.as_object_mut() else {
        return false;
    };

    let should_write = budget_map
        .get(key)
        .and_then(|value| value.as_u64())
        .map(|current| current > limit)
        .unwrap_or(true);
    if should_write {
        budget_map.insert(
            key.to_string(),
            serde_json::Value::Number(serde_json::Number::from(limit)),
        );
    }
    should_write
}

fn has_directive(
    directives: &[crate::agent::critique::types::CritiqueDirective],
    needle: crate::agent::critique::types::CritiqueDirective,
) -> bool {
    directives.iter().any(|directive| *directive == needle)
}

fn maybe_rewrite_shell_tool_to_safer_file_mutation(
    tool_name: &str,
    args: &serde_json::Value,
    critique_modifications: &[String],
) -> Option<(String, serde_json::Value, Vec<String>)> {
    if !matches!(
        tool_names::canonical_tool_name(tool_name),
        tool_names::BASH_COMMAND
            | tool_names::RUN_TERMINAL_COMMAND
            | tool_names::EXECUTE_MANAGED_COMMAND
    ) {
        return None;
    }

    let prefers_apply_patch = critique_modifications.iter().any(|modification| {
        modification
            .trim()
            .to_ascii_lowercase()
            .contains("prefer apply_patch")
    });
    if prefers_apply_patch {
        let patch_text = args
            .get("input")
            .or_else(|| args.get("patch"))
            .and_then(|value| value.as_str())?;
        let mut rewritten = serde_json::Map::new();
        rewritten.insert(
            "input".to_string(),
            serde_json::Value::String(patch_text.to_string()),
        );
        return Some((
            tool_names::APPLY_PATCH.to_string(),
            serde_json::Value::Object(rewritten),
            vec!["fallback:rewrite_to_apply_patch".to_string()],
        ));
    }

    let prefers_replace_in_file = critique_modifications.iter().any(|modification| {
        modification
            .trim()
            .to_ascii_lowercase()
            .contains("prefer replace_in_file")
    });
    if !prefers_replace_in_file {
        return None;
    }

    let path = args.get("path").and_then(|value| value.as_str())?;
    let old_text = args.get("old_text").and_then(|value| value.as_str())?;
    let new_text = args.get("new_text").and_then(|value| value.as_str())?;

    let mut rewritten = serde_json::Map::new();
    rewritten.insert(
        "path".to_string(),
        serde_json::Value::String(path.to_string()),
    );
    rewritten.insert(
        "old_text".to_string(),
        serde_json::Value::String(old_text.to_string()),
    );
    rewritten.insert(
        "new_text".to_string(),
        serde_json::Value::String(new_text.to_string()),
    );
    if let Some(replace_all) = args.get("replace_all").and_then(|value| value.as_bool()) {
        rewritten.insert(
            "replace_all".to_string(),
            serde_json::Value::Bool(replace_all),
        );
    }

    Some((
        tool_names::REPLACE_IN_FILE.to_string(),
        serde_json::Value::Object(rewritten),
        vec!["fallback:rewrite_to_replace_in_file".to_string()],
    ))
}

pub(crate) fn apply_critique_modifications(
    tool_name: &str,
    args: &serde_json::Value,
    critique_decision: Option<&str>,
    critique_reasons: &[String],
    critique_modifications: &[String],
    critique_directives: &[crate::agent::critique::types::CritiqueDirective],
    preferred_start_hour_utc: Option<u8>,
) -> (serde_json::Value, Vec<String>) {
    if critique_decision != Some("proceed_with_modifications") {
        return (args.clone(), Vec::new());
    }

    let critique_bypass_confirmation = args
        .get("__critique_bypass_confirmation")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let mut adjusted = args.clone();
    let Some(map) = adjusted.as_object_mut() else {
        return (adjusted, Vec::new());
    };
    let mut adjustments = Vec::new();

    match tool_names::canonical_tool_name(tool_name) {
        tool_names::BASH_COMMAND
        | tool_names::RUN_TERMINAL_COMMAND
        | tool_names::EXECUTE_MANAGED_COMMAND => {
            if let Some(value) = map.remove("dangerous_flag") {
                map.insert("safe_flag".to_string(), value);
                adjustments.push("shell:rename_key:dangerous_flag->safe_flag".to_string());
            }
            if let Some(parsed) = map
                .get("max_tool_calls")
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse::<u64>().ok())
            {
                map.insert(
                    "max_tool_calls".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(parsed.min(8))),
                );
                adjustments.push("shell:coerce_max_tool_calls".to_string());
            }
            if map.get("allow_network").and_then(|value| value.as_bool()) != Some(false) {
                map.insert("allow_network".to_string(), serde_json::Value::Bool(false));
                adjustments.push("shell:disable_network".to_string());
            }
            if map.get("sandbox_enabled").and_then(|value| value.as_bool()) != Some(true) {
                map.insert("sandbox_enabled".to_string(), serde_json::Value::Bool(true));
                adjustments.push("shell:enable_sandbox".to_string());
            }
            if map.get("security_level").and_then(|value| value.as_str()) == Some("yolo") {
                map.insert(
                    "security_level".to_string(),
                    serde_json::Value::String("moderate".to_string()),
                );
                adjustments.push("shell:downgrade_security_level".to_string());
            } else if !map.contains_key("security_level")
                && has_directive(
                    critique_directives,
                    crate::agent::critique::types::CritiqueDirective::DowngradeSecurityLevel,
                )
            {
                map.insert(
                    "security_level".to_string(),
                    serde_json::Value::String("moderate".to_string()),
                );
                adjustments.push("shell:inject_security_level".to_string());
            }
        }
        tool_names::SEND_SLACK_MESSAGE => {
            if map.remove("channel").is_some() {
                adjustments.push("messaging:strip_explicit_channel".to_string());
            }
            if map.remove("thread_ts").is_some() {
                adjustments.push("messaging:strip_explicit_thread".to_string());
            }
        }
        tool_names::SEND_DISCORD_MESSAGE => {
            if map.remove("channel_id").is_some() {
                adjustments.push("messaging:strip_explicit_channel".to_string());
            }
            if map.remove("user_id").is_some() {
                adjustments.push("messaging:strip_explicit_user".to_string());
            }
            if map.remove("reply_to_message_id").is_some() {
                adjustments.push("messaging:strip_explicit_reply".to_string());
            }
        }
        tool_names::SEND_TELEGRAM_MESSAGE => {
            if map.remove("chat_id").is_some() {
                adjustments.push("messaging:strip_explicit_chat".to_string());
            }
            if map.remove("reply_to_message_id").is_some() {
                adjustments.push("messaging:strip_explicit_reply".to_string());
            }
        }
        tool_names::SEND_WHATSAPP_MESSAGE => {
            if map.remove("phone").is_some() {
                adjustments.push("messaging:strip_explicit_phone".to_string());
            }
            if map.remove("to").is_some() {
                adjustments.push("messaging:strip_explicit_phone".to_string());
            }
        }
        tool_names::SYNTHESIZE_TOOL => {
            let requires_confirmation = !critique_bypass_confirmation
                && critique_modifications.iter().any(|item| {
                    let normalized = item.trim().to_ascii_lowercase();
                    normalized.contains("require explicit operator confirmation")
                        || normalized.contains("runtime tool capability policy")
                        || normalized.contains("tool synthesis")
                });
            if requires_confirmation {
                map.insert(
                    "__critique_requires_operator_confirmation".to_string(),
                    serde_json::Value::Bool(true),
                );
                map.insert(
                    "__critique_confirmation_reason".to_string(),
                    serde_json::Value::String(tool_names::SYNTHESIZE_TOOL.to_string()),
                );
                adjustments.push("synthesize_tool:require_operator_confirmation".to_string());
            }
        }
        tool_names::SWITCH_MODEL => {
            let requires_confirmation = !critique_bypass_confirmation
                && (critique_modifications.iter().any(|item| {
                    let normalized = item.trim().to_ascii_lowercase();
                    normalized.contains("require explicit operator confirmation")
                }) || critique_reasons.iter().any(|reason| {
                    let normalized = reason.trim().to_ascii_lowercase();
                    normalized.contains("persisted agent execution policy")
                        || normalized.contains("provider or model reconfiguration")
                }));
            if requires_confirmation {
                map.insert(
                    "__critique_requires_operator_confirmation".to_string(),
                    serde_json::Value::Bool(true),
                );
                map.insert(
                    "__critique_confirmation_reason".to_string(),
                    serde_json::Value::String(tool_names::SWITCH_MODEL.to_string()),
                );
                adjustments.push("switch_model:require_operator_confirmation".to_string());
            }
        }
        tool_names::PLUGIN_API_CALL => {
            let requires_confirmation = !critique_bypass_confirmation
                && critique_modifications.iter().any(|item| {
                    let normalized = item.trim().to_ascii_lowercase();
                    normalized.contains("require explicit operator confirmation")
                        || normalized.contains("plugin execution policy")
                        || normalized.contains("plugin endpoint")
                });
            if requires_confirmation {
                map.insert(
                    "__critique_requires_operator_confirmation".to_string(),
                    serde_json::Value::Bool(true),
                );
                map.insert(
                    "__critique_confirmation_reason".to_string(),
                    serde_json::Value::String(tool_names::PLUGIN_API_CALL.to_string()),
                );
                adjustments.push("plugin_api_call:require_operator_confirmation".to_string());
            }
        }
        tool_names::WRITE_FILE
        | tool_names::CREATE_FILE
        | tool_names::APPEND_TO_FILE
        | tool_names::REPLACE_IN_FILE
        | tool_names::APPLY_FILE_PATCH => {
            let sensitive_path = has_directive(
                critique_directives,
                crate::agent::critique::types::CritiqueDirective::NarrowSensitiveFilePath,
            ) || critique_requests_sensitive_file_narrowing(
                critique_modifications,
            ) || critique_reasons
                .iter()
                .any(|reason| reason.contains("sensitive path"));
            if sensitive_path {
                for key in ["path", "file_path", "filepath", "filename", "file"] {
                    let Some(current) = map.get(key).and_then(|value| value.as_str()) else {
                        continue;
                    };
                    let narrowed = std::path::Path::new(current)
                        .file_name()
                        .map(|value| value.to_string_lossy().to_string())
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| current.to_string());
                    if narrowed != current {
                        map.insert(key.to_string(), serde_json::Value::String(narrowed));
                        adjustments.push(format!("file:narrow_path:{key}"));
                    }
                    break;
                }
                if map.remove("cwd").is_some() {
                    adjustments.push("file:drop_cwd_anchor".to_string());
                }
                if map.remove("session").is_some() {
                    adjustments.push("file:drop_session_anchor".to_string());
                }
            }
        }
        tool_names::APPLY_PATCH => {
            let sensitive_path = has_directive(
                critique_directives,
                crate::agent::critique::types::CritiqueDirective::NarrowSensitiveFilePath,
            ) || critique_requests_sensitive_file_narrowing(
                critique_modifications,
            ) || critique_reasons
                .iter()
                .any(|reason| reason.contains("sensitive path"));
            if sensitive_path {
                for key in ["input", "patch"] {
                    let Some(current) = map.get(key).and_then(|value| value.as_str()) else {
                        continue;
                    };
                    let rewritten = current
                        .lines()
                        .map(|line| {
                            for prefix in
                                ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                            {
                                if let Some(path) = line.strip_prefix(prefix) {
                                    let narrowed = std::path::Path::new(path.trim())
                                        .file_name()
                                        .map(|value| value.to_string_lossy().to_string())
                                        .filter(|value| !value.is_empty())
                                        .unwrap_or_else(|| path.trim().to_string());
                                    return format!("{prefix}{narrowed}");
                                }
                            }
                            line.to_string()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if rewritten != current {
                        map.insert(key.to_string(), serde_json::Value::String(rewritten));
                        adjustments.push(format!("file:narrow_path:{key}"));
                    }
                    break;
                }
            }
        }
        tool_names::ENQUEUE_TASK => {
            let has_explicit_schedule = map.get("scheduled_at").is_some()
                || map.get("schedule_at").is_some()
                || map.get("delay_seconds").is_some();
            if !has_explicit_schedule
                && (has_directive(
                    critique_directives,
                    crate::agent::critique::types::CritiqueDirective::ScheduleForOperatorWindow,
                ) || critique_requests_operator_window(critique_modifications))
                && preferred_start_hour_utc.is_some()
            {
                let scheduled_at = next_operator_window_timestamp_ms(
                    super::now_millis(),
                    preferred_start_hour_utc.unwrap_or_default(),
                );
                map.insert(
                    "scheduled_at".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(scheduled_at)),
                );
                adjustments.push("temporal:schedule_for_operator_window".to_string());
            }
        }
        tool_names::SPAWN_SUBAGENT => {
            let has_explicit_schedule = map.get("scheduled_at").is_some()
                || map.get("schedule_at").is_some()
                || map.get("delay_seconds").is_some();
            if !has_explicit_schedule
                && (has_directive(
                    critique_directives,
                    crate::agent::critique::types::CritiqueDirective::ScheduleForOperatorWindow,
                ) || critique_requests_operator_window(critique_modifications))
                && preferred_start_hour_utc.is_some()
            {
                let scheduled_at = next_operator_window_timestamp_ms(
                    super::now_millis(),
                    preferred_start_hour_utc.unwrap_or_default(),
                );
                map.insert(
                    "scheduled_at".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(scheduled_at)),
                );
                adjustments.push("temporal:schedule_for_operator_window".to_string());
            }
            let tighten_wall_time =
                has_directive(
                    critique_directives,
                    crate::agent::critique::types::CritiqueDirective::LimitSubagentWallTime,
                ) || critique_requests_narrower_subagent_scope(critique_modifications);
            if tighten_wall_time {
                if upsert_budget_limit(map, "max_wall_time_secs", 120) {
                    adjustments.push("subagent:limit_wall_time".to_string());
                }
            }
        }
        _ => {}
    }

    if let Some(message) = map.get("message").and_then(|value| value.as_str()) {
        if let Some(sanitized) = sanitize_broadcast_mentions(message) {
            map.insert("message".to_string(), serde_json::Value::String(sanitized));
            adjustments.push("messaging:strip_broadcast_mentions".to_string());
        }
    }

    (adjusted, adjustments)
}

fn injected_reserved_critique_marker(args: &serde_json::Value) -> Option<String> {
    let map = args.as_object()?;
    map.keys()
        .find(|key| key.starts_with("__critique_"))
        .cloned()
}

fn trusted_critique_confirmation_replay(tool_call: &ToolCall, args: &serde_json::Value) -> bool {
    let critique_bypass_confirmation = args
        .get("__critique_bypass_confirmation")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    critique_bypass_confirmation
        && tool_call.weles_review.as_ref().is_some_and(|review| {
            review
                .reasons
                .iter()
                .any(|reason| reason == "trusted_critique_confirmation_replay")
        })
}

pub(crate) fn annotate_review_with_critique(
    review: &mut crate::agent::types::WelesReviewMeta,
    critique_session_id: Option<&str>,
    critique_decision: Option<&str>,
    critique_adjustments: &[String],
    critique_report_summary: Option<&str>,
) {
    let Some(session_id) = critique_session_id else {
        return;
    };
    review.weles_reviewed = true;
    let critique_decision = critique_decision.unwrap_or("proceed");
    if !review
        .reasons
        .iter()
        .any(|reason| reason.contains("critique_preflight:"))
    {
        review.reasons.push(format!(
            "critique_preflight:{}:{}",
            session_id, critique_decision
        ));
    }
    for adjustment in critique_adjustments {
        let reason = format!("critique_applied:{adjustment}");
        if !review.reasons.iter().any(|existing| existing == &reason) {
            review.reasons.push(reason);
        }
    }
    if let Some(summary) = critique_report_summary {
        let reason = format!("critique_report:{summary}");
        if !review.reasons.iter().any(|existing| existing == &reason) {
            review.reasons.push(reason);
        }
    }
    if review.audit_id.is_none() {
        review.audit_id = Some(session_id.to_string());
    }
}

fn is_shell_execution_tool(tool_name: &str) -> bool {
    matches!(
        tool_names::canonical_tool_name(tool_name),
        tool_names::BASH_COMMAND
            | tool_names::RUN_TERMINAL_COMMAND
            | tool_names::EXECUTE_MANAGED_COMMAND
    )
}

fn pending_approval_for_weles_shell_block(
    tool_name: &str,
    args: &serde_json::Value,
    review: &crate::agent::types::WelesReviewMeta,
    security_level: SecurityLevel,
) -> crate::agent::types::ToolPendingApproval {
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(|value| crate::agent::summarize_text(value, 200))
        .unwrap_or_else(|| crate::agent::summarize_text(&args.to_string(), 200));
    let risk_level = match security_level {
        SecurityLevel::Highest => "high",
        SecurityLevel::Yolo => "yolo",
        SecurityLevel::Lowest => "low",
        SecurityLevel::Moderate => "medium",
    };
    let mut reasons = review.reasons.clone();
    if !reasons
        .iter()
        .any(|reason| reason == "WELES requested operator decision before shell execution")
    {
        reasons.push("WELES requested operator decision before shell execution".to_string());
    }

    crate::agent::types::ToolPendingApproval {
        approval_id: format!("weles-governance-{}", uuid::Uuid::new_v4()),
        execution_id: format!("weles-governance-exec-{}", uuid::Uuid::new_v4()),
        command: format!("{tool_name} {command}"),
        rationale: "WELES governance returned a block verdict for a shell command; operator approval is required before execution.".to_string(),
        risk_level: risk_level.to_string(),
        blast_radius: "shell command execution".to_string(),
        reasons,
        session_id: args
            .get("session")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
    }
}

struct PreparedToolExecution {
    pub(crate) tool_name: String,
    pub(crate) args: serde_json::Value,
    pub(crate) dispatch_tool_name: String,
    pub(crate) dispatch_args: serde_json::Value,
    pub(crate) timeout_adjustments: Vec<String>,
    pub(crate) governance_decision: crate::agent::weles_governance::WelesExecutionDecision,
    pub(crate) critique_session_id: Option<String>,
    pub(crate) critique_decision: Option<String>,
    pub(crate) critique_adjustments: Vec<String>,
    pub(crate) critique_report_summary: Option<String>,
}

async fn prepare_tool_execution(
    tool_call: &ToolCall,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<PreparedToolExecution, ToolResult> {
    let args = match parse_tool_args(
        tool_call.function.name.as_str(),
        &tool_call.function.arguments,
    ) {
        Ok(args) => args,
        Err(error) => {
            tracing::warn!(
                tool = %tool_call.function.name,
                error = %error,
                "agent tool argument parse failed"
            );
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: error,
                is_error: true,
                weles_review: tool_call.weles_review.clone(),
                pending_approval: None,
            });
        }
    };
    let critique_classification =
        crate::agent::weles_governance::classify_tool_call(tool_call.function.name.as_str(), &args);
    let current_task = if let Some(task_id) = task_id {
        task_by_id_for_tool_scope(agent, task_id).await
    } else {
        None
    };
    let trusted_weles_internal_task = if let Some(task) = current_task.as_ref() {
        task.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            && agent.trusted_weles_tasks.read().await.contains(&task.id)
    } else {
        false
    };
    let critique_bypass_confirmation = args
        .get("__critique_bypass_confirmation")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    if let Some(marker) = injected_reserved_critique_marker(&args) {
        let trusted_internal_marker = marker == "__critique_bypass_confirmation"
            && (trusted_weles_internal_task
                || trusted_critique_confirmation_replay(tool_call, &args));
        if !trusted_internal_marker {
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: format!(
                    "Rejected reserved internal critique marker `{marker}` in tool arguments."
                ),
                is_error: true,
                weles_review: tool_call.weles_review.clone(),
                pending_approval: None,
            });
        }
    }
    let critique_result = if critique_bypass_confirmation {
        None
    } else if agent
        .should_run_critique_preflight(tool_call.function.name.as_str(), &critique_classification)
        .await
    {
        let action_summary = crate::agent::summarize_text(&tool_call.function.arguments, 240);
        match agent
            .run_critique_preflight(
                &tool_call.id,
                tool_call.function.name.as_str(),
                &action_summary,
                &critique_classification.reasons,
                Some(thread_id),
                task_id,
            )
            .await
        {
            Ok(session) => {
                let report_summary = crate::agent::critique::operator_report_summary(&session);
                let decision = session
                    .resolution
                    .as_ref()
                    .map(|resolution| resolution.decision.as_str().to_string());
                let risk_tolerance = agent
                    .operator_model
                    .read()
                    .await
                    .risk_fingerprint
                    .risk_tolerance;
                if let Some(resolution) = session.resolution.as_ref() {
                    if agent.critique_requires_blocking_review(resolution, risk_tolerance) {
                        let decision = resolution.decision.as_str();
                        let mut review = crate::agent::types::WelesReviewMeta {
                            weles_reviewed: true,
                            verdict: crate::agent::types::WelesVerdict::Block,
                            reasons: Vec::new(),
                            audit_id: None,
                            security_override_mode: None,
                        };
                        annotate_review_with_critique(
                            &mut review,
                            Some(session.id.as_str()),
                            Some(decision),
                            &[],
                            Some(report_summary.as_str()),
                        );
                        return Err(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.function.name.clone(),
                            content: format!(
                                "Blocked by critique preflight ({decision}). critique_session_id={} :: {}",
                                session.id, resolution.synthesis
                            ),
                            is_error: true,
                            weles_review: Some(review),
                            pending_approval: None,
                        });
                    }
                }
                let modifications = session
                    .resolution
                    .as_ref()
                    .map(|resolution| resolution.modifications.clone())
                    .unwrap_or_default();
                let directives = session
                    .resolution
                    .as_ref()
                    .map(|resolution| resolution.directives.clone())
                    .unwrap_or_default();
                Some((
                    session.id,
                    decision,
                    modifications,
                    directives,
                    report_summary,
                ))
            }
            Err(error) => {
                tracing::warn!(tool = %tool_call.function.name, error = %error, "critique preflight failed; continuing without critique enforcement");
                None
            }
        }
    } else {
        None
    };
    let (
        critique_session_id,
        critique_decision,
        critique_modifications,
        critique_directives,
        critique_report_summary,
    ) = critique_result
        .map(
            |(session_id, decision, modifications, directives, report_summary)| {
                (
                    Some(session_id),
                    decision,
                    modifications,
                    directives,
                    Some(report_summary),
                )
            },
        )
        .unwrap_or((None, None, Vec::new(), Vec::new(), None));
    let preferred_start_hour_utc = agent
        .operator_model
        .read()
        .await
        .session_rhythm
        .typical_start_hour_utc;
    let (runtime_args, mut critique_adjustments) = apply_critique_modifications(
        tool_call.function.name.as_str(),
        &args,
        critique_decision.as_deref(),
        &critique_classification.reasons,
        &critique_modifications,
        &critique_directives,
        preferred_start_hour_utc,
    );
    if let Some(map) = runtime_args.as_object() {
        if !critique_bypass_confirmation
            && map
                .get("__critique_requires_operator_confirmation")
                .and_then(|value| value.as_bool())
                == Some(true)
        {
            let mut review = crate::agent::types::WelesReviewMeta {
                weles_reviewed: true,
                verdict: crate::agent::types::WelesVerdict::Block,
                reasons: Vec::new(),
                audit_id: None,
                security_override_mode: None,
            };
            annotate_review_with_critique(
                &mut review,
                critique_session_id.as_deref(),
                critique_decision.as_deref(),
                &critique_adjustments,
                critique_report_summary.as_deref(),
            );
            let confirmation_reason = map
                .get("__critique_confirmation_reason")
                .and_then(|value| value.as_str())
                .unwrap_or(tool_call.function.name.as_str());
            let pending_approval = crate::agent::types::ToolPendingApproval {
                approval_id: format!("critique-confirmation-{}", uuid::Uuid::new_v4()),
                execution_id: format!("critique-confirmation-exec-{}", uuid::Uuid::new_v4()),
                command: format!(
                    "{} {}",
                    tool_call.function.name,
                    crate::agent::summarize_text(&tool_call.function.arguments, 200)
                ),
                rationale: format!(
                    "Critique requires explicit operator confirmation before executing {confirmation_reason}."
                ),
                risk_level: "medium".to_string(),
                blast_radius: "agent execution policy".to_string(),
                reasons: vec![format!(
                    "critique confirmation required for {confirmation_reason}"
                )],
                session_id: None,
            };
            agent.critique_approval_continuations.lock().await.insert(
                pending_approval.approval_id.clone(),
                crate::agent::CritiqueApprovalContinuation {
                    tool_call: ToolCall {
                        id: tool_call.id.clone(),
                        function: crate::agent::types::ToolFunction {
                            name: tool_call.function.name.clone(),
                            arguments: {
                                let mut resumed_args = args.clone();
                                if let Some(map) = resumed_args.as_object_mut() {
                                    map.insert(
                                        "__critique_bypass_confirmation".to_string(),
                                        serde_json::Value::Bool(true),
                                    );
                                }
                                resumed_args.to_string()
                            },
                        },
                        weles_review: {
                            let mut review = tool_call.weles_review.clone().unwrap_or(
                                crate::agent::types::WelesReviewMeta {
                                    weles_reviewed: false,
                                    verdict: crate::agent::types::WelesVerdict::Allow,
                                    reasons: Vec::new(),
                                    audit_id: None,
                                    security_override_mode: None,
                                },
                            );
                            if !review
                                .reasons
                                .iter()
                                .any(|reason| reason == "trusted_critique_confirmation_replay")
                            {
                                review
                                    .reasons
                                    .push("trusted_critique_confirmation_replay".to_string());
                            }
                            Some(review)
                        },
                    },
                    thread_id: thread_id.to_string(),
                    agent_data_dir: agent.data_dir.clone(),
                },
            );
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: format!(
                    "Critique confirmation for {confirmation_reason} requires operator approval before execution. Approval ID: {}",
                    pending_approval.approval_id
                ),
                is_error: false,
                weles_review: Some(review),
                pending_approval: Some(pending_approval),
            });
        }
    }
    let (effective_tool_name, mut effective_args, rewrite_adjustments) =
        maybe_rewrite_shell_tool_to_safer_file_mutation(
            tool_call.function.name.as_str(),
            &runtime_args,
            &critique_modifications,
        )
        .unwrap_or_else(|| {
            (
                tool_call.function.name.clone(),
                runtime_args.clone(),
                Vec::new(),
            )
        });
    if !rewrite_adjustments.is_empty() {
        critique_adjustments.extend(rewrite_adjustments);
    }
    let security_level = {
        let config = agent.config.read().await;
        crate::agent::weles_governance::security_level_for_tool_call(
            &config,
            effective_tool_name.as_str(),
            &effective_args,
        )
    };
    crate::agent::weles_governance::stamp_shell_tool_security_level(
        effective_tool_name.as_str(),
        &mut effective_args,
        security_level,
    );
    let active_scope_id = crate::agent::agent_identity::current_agent_scope_id();
    let governance_classification = crate::agent::weles_governance::classify_tool_call(
        effective_tool_name.as_str(),
        &effective_args,
    );
    let governance_decision = if matches!(security_level, SecurityLevel::Yolo) {
        // YOLO is an operator-selected no-supervision mode. Do not spawn a
        // WELES review task, create an approval, or attach a synthetic
        // supervision element to a goal. The ordinary tool result/audit trail
        // remains the source of execution visibility.
        crate::agent::weles_governance::direct_allow_decision(governance_classification.class)
    } else if !crate::agent::weles_governance::should_guard_classification(
        &governance_classification,
    ) {
        crate::agent::weles_governance::direct_allow_decision(governance_classification.class)
    } else if critique_bypass_confirmation {
        crate::agent::weles_governance::reviewed_runtime_decision(
            &governance_classification,
            security_level,
            crate::agent::weles_governance::WelesRuntimeReviewPayload {
                verdict: crate::agent::types::WelesVerdict::Allow,
                reasons: vec!["operator approved critique confirmation replay".to_string()],
                audit_id: Some(format!("critique_confirmation_{}", tool_call.id)),
            },
        )
    } else if effective_tool_name.as_str() == tool_names::SWITCH_MODEL {
        crate::agent::weles_governance::direct_allow_decision(governance_classification.class)
    } else if crate::agent::agent_identity::is_weles_agent_scope(&active_scope_id) {
        crate::agent::weles_governance::internal_runtime_decision(
            &governance_classification,
            security_level,
        )
    } else if trusted_weles_internal_task {
        crate::agent::weles_governance::internal_runtime_decision(
            &governance_classification,
            security_level,
        )
    } else {
        let config = agent.config.read().await;
        if !crate::agent::weles_governance::review_available(&config) {
            crate::agent::weles_governance::guarded_fallback_decision(
                &governance_classification,
                security_level,
            )
        } else {
            drop(config);
            match spawn_weles_internal_subagent(
                agent,
                thread_id,
                task_id,
                crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
                effective_tool_name.as_str(),
                &effective_args,
                security_level,
                &governance_classification.reasons,
            )
            .await
            {
                Ok(weles_task) => {
                    let review_outcome = agent
                        .send_internal_task_message(
                            &active_scope_id,
                            crate::agent::agent_identity::WELES_AGENT_ID,
                            &weles_task.id,
                            None,
                            Some("daemon"),
                            &crate::agent::weles_governance::build_weles_runtime_review_message(
                                &governance_classification,
                                security_level,
                            ),
                        )
                        .await;
                    finalize_synchronous_weles_review_task(
                        agent,
                        &weles_task,
                        review_outcome.as_ref(),
                    )
                    .await;
                    match review_outcome {
                        Ok(outcome) => {
                            let response = agent
                                .latest_assistant_message_text(&outcome.thread_id)
                                .await
                                .unwrap_or_default();
                            if let Some(runtime_review) =
                                crate::agent::weles_governance::parse_weles_runtime_review_response(
                                    &response,
                                )
                            {
                                let runtime_review = crate::agent::weles_governance::normalize_runtime_verdict_for_classification(
                                    &governance_classification,
                                    security_level,
                                    runtime_review,
                                );
                                crate::agent::weles_governance::reviewed_runtime_decision(
                                    &governance_classification,
                                    security_level,
                                    runtime_review,
                                )
                            } else {
                                crate::agent::weles_governance::guarded_fallback_decision(
                                    &governance_classification,
                                    security_level,
                                )
                            }
                        }
                        Err(_) => crate::agent::weles_governance::guarded_fallback_decision(
                            &governance_classification,
                            security_level,
                        ),
                    }
                }
                Err(_) => crate::agent::weles_governance::guarded_fallback_decision(
                    &governance_classification,
                    security_level,
                ),
            }
        }
    };
    if !governance_decision.should_execute {
        let mut review = governance_decision.review.clone();
        annotate_review_with_critique(
            &mut review,
            critique_session_id.as_deref(),
            critique_decision.as_deref(),
            &critique_adjustments,
            critique_report_summary.as_deref(),
        );
        if is_shell_execution_tool(effective_tool_name.as_str()) {
            let pending_approval = pending_approval_for_weles_shell_block(
                effective_tool_name.as_str(),
                &effective_args,
                &review,
                security_level,
            );
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: format!(
                    "WELES governance requires operator approval before shell execution. Approval ID: {}",
                    pending_approval.approval_id
                ),
                is_error: false,
                weles_review: Some(review),
                pending_approval: Some(pending_approval),
            });
        }
        return Err(ToolResult {
            tool_call_id: tool_call.id.clone(),
            name: tool_call.function.name.clone(),
            content: governance_decision.block_message.unwrap_or_else(|| {
                "Blocked by WELES governance before tool execution.".to_string()
            }),
            is_error: true,
            weles_review: Some(review),
            pending_approval: None,
        });
    }
    if trusted_weles_internal_task
        && tool_names::canonical_tool_name(effective_tool_name.as_str()) == tool_names::BASH_COMMAND
    {
        if let serde_json::Value::Object(ref mut map) = effective_args {
            map.insert(
                "__weles_force_headless".to_string(),
                serde_json::Value::Bool(true),
            );
        }
    }
    let (dispatch_tool_name, mut dispatch_args) =
        normalize_tool_dispatch(effective_tool_name.as_str(), &effective_args);

    let timeout_adjustments = {
        let model = agent.operator_model.read().await;
        let adaptation_mode = crate::agent::operator_model::SatisfactionAdaptationMode::from_label(
            &model.operator_satisfaction.label,
        );
        let mut adjustments = Vec::new();
        if let Some(adapted_timeout_seconds) = adapted_timeout_override_for_mode(
            dispatch_tool_name.as_str(),
            &dispatch_args,
            adaptation_mode,
        ) {
            if let Some(map) = dispatch_args.as_object_mut() {
                map.insert(
                    "timeout_seconds".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(adapted_timeout_seconds)),
                );
                adjustments.push(format!(
                    "timeout:{}:{}",
                    dispatch_tool_name, adapted_timeout_seconds
                ));
            }
        }
        adjustments
    };

    Ok(PreparedToolExecution {
        tool_name: effective_tool_name,
        args: effective_args.clone(),
        dispatch_tool_name,
        dispatch_args,
        timeout_adjustments,
        governance_decision,
        critique_session_id,
        critique_decision,
        critique_adjustments,
        critique_report_summary,
    })
}

async fn dispatch_tool_execution(
    prepared: &PreparedToolExecution,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    agent_data_dir: &std::path::Path,
    http_client: &reqwest::Client,
    cancel_token: Option<CancellationToken>,
) -> (Result<String>, Option<ToolPendingApproval>) {
    let args = &prepared.args;
    let dispatch_args = &prepared.dispatch_args;
    let mut pending_approval = None;

    let result = match prepared.dispatch_tool_name.as_str() {
        tool_names::LIST_TERMINALS | tool_names::LIST_SESSIONS => {
            execute_list_sessions(session_manager).await
        }
        tool_names::READ_ACTIVE_TERMINAL_CONTENT => {
            execute_read_terminal(args, session_manager).await
        }
        tool_names::RUN_TERMINAL_COMMAND => {
            match execute_run_terminal_command(
                dispatch_args,
                agent,
                task_id,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::EXECUTE_MANAGED_COMMAND => {
            match execute_managed_command(
                dispatch_args,
                agent,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::GET_OPERATION_STATUS => {
            execute_get_operation_status(
                args,
                session_manager,
                Some(agent),
                cancel_token.clone(),
                Some(thread_id),
            )
            .await
        }
        tool_names::GET_BACKGROUND_TASK_STATUS => {
            execute_get_background_task_status(
                args,
                session_manager,
                Some(agent),
                cancel_token.clone(),
                Some(thread_id),
            )
            .await
        }
        tool_names::ALLOCATE_TERMINAL => {
            execute_allocate_terminal(
                args,
                agent,
                thread_id,
                task_id,
                session_manager,
                session_id,
                event_tx,
            )
            .await
        }
        tool_names::FETCH_AUTHENTICATED_PROVIDERS => {
            execute_fetch_authenticated_providers(agent).await
        }
        tool_names::LIST_PROVIDERS => execute_list_providers(agent).await,
        tool_names::FETCH_PROVIDER_MODELS => execute_fetch_provider_models(args, agent).await,
        tool_names::LIST_MODELS => execute_list_models(args, agent).await,
        tool_names::LIST_AGENTS => execute_list_agents(agent).await,
        tool_names::LIST_PARTICIPANTS => execute_list_participants(agent, thread_id).await,
        tool_names::SWITCH_MODEL => execute_switch_model(args, agent).await,
        tool_names::SPAWN_SUBAGENT => {
            execute_spawn_subagent(
                args,
                agent,
                thread_id,
                task_id,
                session_manager,
                session_id,
                event_tx,
            )
            .await
        }
        tool_names::HANDOFF_THREAD_AGENT => {
            match execute_handoff_thread_agent(args, agent, thread_id).await {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::LIST_SUBAGENTS => execute_list_subagents(args, agent, thread_id, task_id).await,
        tool_names::REPORT_SUBAGENT_OUTCOME => {
            execute_report_subagent_outcome(args, agent, thread_id, task_id).await
        }
        tool_names::EXTEND_SUBAGENT_BUDGET => {
            execute_extend_subagent_budget(args, agent, thread_id, task_id).await
        }
        tool_names::MESSAGE_AGENT => {
            Box::pin(execute_message_agent(
                args, agent, thread_id, task_id, session_id,
            ))
            .await
        }
        tool_names::ASK_PARENT => execute_ask_parent(args, agent, thread_id, task_id).await,
        tool_names::ANSWER_CHILD => execute_answer_child(args, agent, thread_id, task_id).await,
        tool_names::NOTE_TO_CHILD => execute_note_to_child(args, agent, thread_id, task_id).await,
        tool_names::ROUTE_TO_SPECIALIST => {
            execute_route_to_specialist(args, agent, thread_id, task_id).await
        }
        tool_names::RUN_DIVERGENT => execute_run_divergent(args, agent, thread_id, task_id).await,
        tool_names::GET_DIVERGENT_SESSION => execute_get_divergent_session(args, agent).await,
        tool_names::RUN_DEBATE => execute_run_debate(args, agent, thread_id, task_id).await,
        tool_names::GET_DEBATE_SESSION => execute_get_debate_session(args, agent).await,
        tool_names::GET_CRITIQUE_SESSION => execute_get_critique_session(args, agent).await,
        tool_names::LOOKUP_EMERGENT_PROTOCOL => {
            execute_lookup_emergent_protocol(args, agent, thread_id).await
        }
        tool_names::LIST_EMERGENT_PROTOCOL_PROPOSALS => {
            execute_list_emergent_protocol_proposals(args, agent, thread_id).await
        }
        tool_names::RESPOND_EMERGENT_PROTOCOL_PROPOSAL => {
            execute_respond_emergent_protocol_proposal(args, agent, thread_id).await
        }
        tool_names::RELOAD_EMERGENT_PROTOCOL_REGISTRY => {
            execute_reload_emergent_protocol_registry(args, agent, thread_id).await
        }
        tool_names::DECODE_EMERGENT_PROTOCOL => {
            execute_decode_emergent_protocol(args, agent, thread_id).await
        }
        tool_names::GET_EMERGENT_PROTOCOL_USAGE_LOG => {
            execute_get_emergent_protocol_usage_log(args, agent).await
        }
        tool_names::APPEND_DEBATE_ARGUMENT => execute_append_debate_argument(args, agent).await,
        tool_names::ADVANCE_DEBATE_ROUND => execute_advance_debate_round(args, agent).await,
        tool_names::COMPLETE_DEBATE_SESSION => execute_complete_debate_session(args, agent).await,
        tool_names::BROADCAST_CONTRIBUTION => {
            execute_broadcast_contribution(args, agent, thread_id, task_id).await
        }
        tool_names::READ_PEER_MEMORY => execute_read_peer_memory(args, agent, task_id).await,
        tool_names::VOTE_ON_DISAGREEMENT => {
            execute_vote_on_disagreement(args, agent, thread_id, task_id).await
        }
        tool_names::DISPATCH_VIA_BID_PROTOCOL => {
            execute_dispatch_via_bid_protocol(args, agent).await
        }
        tool_names::LIST_COLLABORATION_SESSIONS => {
            execute_list_collaboration_sessions(args, agent, task_id).await
        }
        tool_names::LIST_THREADS => execute_list_threads(args, agent).await,
        tool_names::GET_THREAD => execute_get_thread(args, agent).await,
        tool_names::READ_OFFLOADED_PAYLOAD => {
            execute_read_offloaded_payload(args, agent, thread_id).await
        }
        tool_names::ENQUEUE_TASK => execute_enqueue_task(args, agent).await,
        tool_names::LIST_TASKS => execute_list_tasks(args, agent).await,
        tool_names::SCHEDULE_WAKEUP => execute_schedule_wakeup(args, agent, thread_id).await,
        tool_names::CANCEL_WAKEUP => execute_cancel_wakeup(args, agent).await,
        tool_names::START_GOAL_RUN => {
            execute_start_goal_run(args, agent, thread_id, session_id).await
        }
        tool_names::LIST_GOAL_RUNS => execute_list_goal_runs(&prepared.args, agent).await,
        tool_names::REQUEST_GOAL_REVIEW => {
            execute_request_goal_review(args, agent, thread_id, task_id).await
        }
        tool_names::SUBMIT_GOAL_REVIEW => {
            execute_submit_goal_review(args, agent, thread_id, task_id).await
        }
        tool_names::CREATE_ROUTINE => execute_create_routine(args, agent).await,
        tool_names::LIST_ROUTINES => execute_list_routines(args, agent).await,
        tool_names::GET_ROUTINE => execute_get_routine(args, agent).await,
        tool_names::PREVIEW_ROUTINE => execute_preview_routine(args, agent).await,
        tool_names::UPDATE_ROUTINE => execute_update_routine(args, agent).await,
        tool_names::RUN_ROUTINE_NOW => execute_run_routine_now(args, agent).await,
        tool_names::LIST_ROUTINE_HISTORY => execute_list_routine_history(args, agent).await,
        tool_names::RERUN_ROUTINE => execute_rerun_routine(args, agent).await,
        tool_names::PAUSE_ROUTINE => execute_pause_routine(args, agent).await,
        tool_names::RESUME_ROUTINE => execute_resume_routine(args, agent).await,
        tool_names::DELETE_ROUTINE => execute_delete_routine(args, agent).await,
        tool_names::RUN_WORKFLOW_PACK => {
            match execute_run_workflow_pack(args, agent, thread_id, task_id).await {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::WHATSAPP_LINK_START => execute_whatsapp_link_start(args, agent).await,
        tool_names::WHATSAPP_LINK_STOP => execute_whatsapp_link_stop(args, agent).await,
        tool_names::WHATSAPP_LINK_RESET => execute_whatsapp_link_reset(args, agent).await,
        tool_names::WHATSAPP_LINK_STATUS => execute_whatsapp_link_status(args, agent).await,
        tool_names::LIST_TRIGGERS => execute_list_triggers(args, agent).await,
        tool_names::INGEST_WEBHOOK_EVENT => execute_ingest_webhook_event(args, agent).await,
        tool_names::ADD_TRIGGER => execute_add_trigger(args, agent).await,
        tool_names::LIST_TRIGGER_FIRE_HISTORY => {
            execute_list_trigger_fire_history(args, agent).await
        }
        tool_names::GET_COST_SUMMARY => execute_get_cost_summary(args, agent).await,
        tool_names::LIST_BROWSER_PROFILES => execute_list_browser_profiles(args, agent).await,
        tool_names::CREATE_BROWSER_PROFILE => execute_create_browser_profile(args, agent).await,
        tool_names::UPDATE_BROWSER_PROFILE_HEALTH => {
            execute_update_browser_profile_health(args, agent).await
        }
        tool_names::SHOW_DREAMS => execute_show_dreams(args, agent).await,
        tool_names::SHOW_HARNESS_STATE => {
            execute_show_harness_state(args, agent, thread_id, task_id).await
        }
        tool_names::IMPORT_EXTERNAL_RUNTIME => execute_import_external_runtime(args, agent).await,
        tool_names::SHOW_IMPORT_REPORT => execute_show_import_report(args, agent).await,
        tool_names::PREVIEW_SHADOW_RUN => execute_preview_shadow_run(args, agent).await,
        tool_names::GET_TODOS => execute_get_todos(args, agent, thread_id, task_id).await,
        tool_names::CANCEL_TASK => execute_cancel_task(args, agent, thread_id, task_id).await,
        tool_names::TYPE_IN_TERMINAL => execute_type_in_terminal(args, session_manager).await,
        tool_names::SEND_SLACK_MESSAGE
        | tool_names::SEND_DISCORD_MESSAGE
        | tool_names::SEND_TELEGRAM_MESSAGE
        | tool_names::SEND_WHATSAPP_MESSAGE => {
            execute_gateway_message(prepared.tool_name.as_str(), args, agent, http_client).await
        }
        tool_names::LIST_WORKSPACES
        | tool_names::CREATE_WORKSPACE
        | tool_names::SET_ACTIVE_WORKSPACE
        | tool_names::CREATE_SURFACE
        | tool_names::SET_ACTIVE_SURFACE
        | tool_names::SPLIT_PANE
        | tool_names::RENAME_PANE
        | tool_names::SET_LAYOUT_PRESET
        | tool_names::EQUALIZE_LAYOUT
        | tool_names::LIST_SNIPPETS
        | tool_names::CREATE_SNIPPET
        | tool_names::RUN_SNIPPET => {
            execute_workspace_tool(prepared.tool_name.as_str(), args, event_tx).await
        }
        tool_names::WORKSPACE_GET_SETTINGS
        | tool_names::WORKSPACE_LIST_TASKS
        | tool_names::WORKSPACE_GET_TASK
        | tool_names::WORKSPACE_LIST_NOTICES
        | tool_names::WORKSPACE_SET_OPERATOR
        | tool_names::WORKSPACE_CREATE_TASK
        | tool_names::WORKSPACE_UPDATE_TASK
        | tool_names::WORKSPACE_MOVE_TASK
        | tool_names::WORKSPACE_RUN_TASK
        | tool_names::WORKSPACE_PAUSE_TASK
        | tool_names::WORKSPACE_STOP_TASK
        | tool_names::WORKSPACE_DELETE_TASK
        | tool_names::WORKSPACE_SUBMIT_REVIEW
        | tool_names::WORKSPACE_SUBMIT_COMPLETION => {
            execute_workspace_task_tool(prepared.tool_name.as_str(), args, agent).await
        }
        tool_names::BASH_COMMAND => {
            match execute_bash_command(
                dispatch_args,
                agent,
                task_id,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::PYTHON_EXECUTE => {
            match execute_python_execute(
                dispatch_args,
                agent,
                task_id,
                thread_id,
                session_manager,
                session_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        tool_names::ANALYZE_IMAGE => execute_analyze_image(args, agent, http_client).await,
        tool_names::GENERATE_IMAGE => {
            execute_generate_image(args, agent, http_client, Some(thread_id)).await
        }
        tool_names::SPEECH_TO_TEXT => execute_speech_to_text(args, agent, http_client).await,
        tool_names::TEXT_TO_SPEECH => execute_text_to_speech(args, agent, http_client).await,
        tool_names::LIST_FILES => execute_list_files(args, session_manager, session_id).await,
        tool_names::READ_FILE => execute_read_file(args).await,
        tool_names::GET_GIT_LINE_STATUSES => execute_get_git_line_statuses(args).await,
        tool_names::WRITE_FILE => execute_write_file(args, session_manager, session_id).await,
        tool_names::CREATE_FILE => execute_create_file(args).await,
        tool_names::APPEND_TO_FILE => execute_append_to_file(args).await,
        tool_names::REPLACE_IN_FILE => execute_replace_in_file(args).await,
        tool_names::APPLY_FILE_PATCH => execute_apply_file_patch(args).await,
        tool_names::APPLY_PATCH => execute_apply_patch(args).await,
        tool_names::SEARCH_FILES => execute_search_files(args).await,
        tool_names::GET_SYSTEM_INFO => execute_system_info().await,
        tool_names::GET_CURRENT_DATETIME => execute_current_datetime().await,
        tool_names::LIST_PROCESSES => execute_list_processes(args).await,
        tool_names::SEARCH_HISTORY => execute_search_history(args, agent).await,
        tool_names::FETCH_GATEWAY_HISTORY => {
            execute_fetch_gateway_history(args, agent, thread_id).await
        }
        tool_names::SESSION_SEARCH => execute_session_search(args, session_manager).await,
        tool_names::AGENT_QUERY_MEMORY => execute_agent_query_memory(args, agent).await,
        tool_names::ONECONTEXT_SEARCH => execute_onecontext_search(args).await,
        tool_names::NOTIFY_USER => execute_notify(args, agent).await,
        tool_names::UPDATE_TODO => execute_update_todo(args, agent, thread_id, task_id).await,
        tool_names::UPDATE_MEMORY => {
            execute_update_memory(args, agent, thread_id, task_id, agent_data_dir).await
        }
        tool_names::READ_MEMORY => {
            execute_read_memory(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::READ_USER => {
            execute_read_user(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::READ_SOUL => {
            execute_read_soul(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::SEARCH_MEMORY => {
            execute_search_memory(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::SEARCH_USER => {
            execute_search_user(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::SEARCH_SOUL => {
            execute_search_soul(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        tool_names::LIST_TOOLS => {
            execute_list_tools(
                args,
                agent,
                session_manager,
                agent_data_dir,
                thread_id,
                task_id,
            )
            .await
        }
        tool_names::TOOL_SEARCH => {
            execute_tool_search(
                args,
                agent,
                session_manager,
                agent_data_dir,
                thread_id,
                task_id,
            )
            .await
        }
        tool_names::LIST_GUIDELINES => execute_list_guidelines(args, agent_data_dir).await,
        tool_names::DISCOVER_GUIDELINES => {
            execute_discover_guidelines(args, agent, session_id).await
        }
        tool_names::READ_GUIDELINE => {
            let result = execute_read_guideline(args, agent_data_dir).await;
            if let Ok(ref content) = result {
                if let Some(name) = args.get("guideline").and_then(|v| v.as_str()) {
                    let _ = agent
                        .history
                        .record_thread_skill_read(
                            thread_id,
                            "guideline",
                            name,
                            content,
                            crate::agent::now_millis() as i64,
                        )
                        .await;
                }
            }
            result
        }
        tool_names::LIST_SKILLS => execute_list_skills(args, agent_data_dir, &agent.history).await,
        tool_names::DISCOVER_SKILLS => execute_discover_skills(args, agent, session_id).await,
        tool_names::SEMANTIC_QUERY => {
            execute_semantic_query(
                dispatch_args,
                session_manager,
                session_id,
                &agent.history,
                agent_data_dir,
            )
            .await
        }
        tool_names::READ_SKILL => {
            let result = execute_read_skill(
                args,
                agent,
                agent_data_dir,
                &agent.history,
                session_manager,
                session_id,
                thread_id,
                task_id,
            )
            .await;
            if let Ok(ref content) = result {
                let name = args.get("skill").and_then(|v| v.as_str()).or_else(|| {
                    args.get("skills")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                });
                if let Some(name) = name {
                    let _ = agent
                        .history
                        .record_thread_skill_read(
                            thread_id,
                            "skill",
                            name,
                            content,
                            crate::agent::now_millis() as i64,
                        )
                        .await;
                }
            }
            result
        }
        tool_names::ASK_QUESTIONS => {
            let parsed = (|| -> Result<(String, Vec<String>, Option<String>, Option<String>)> {
                let content = dispatch_args
                    .get("content")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?
                    .to_string();
                let options = dispatch_args
                    .get("options")
                    .and_then(|value| value.as_array())
                    .ok_or_else(|| anyhow::anyhow!("missing 'options' argument"))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|option| !option.is_empty())
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "'options' must be an array of compact non-empty strings"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let session = dispatch_args
                    .get("session")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| session_id.map(|value| value.to_string()));
                let thread = (!thread_id.trim().is_empty()).then(|| thread_id.to_string());
                Ok((content, options, session, thread))
            })();

            match parsed {
                Ok((content, options, session, thread)) => agent
                    .ask_operator_question(&content, options, session, thread)
                    .await
                    .map(|(_, answer)| answer),
                Err(error) => Err(error),
            }
        }
        tool_names::JUSTIFY_SKILL_SKIP => execute_justify_skill_skip(args, agent, thread_id).await,
        tool_names::SYNTHESIZE_TOOL => {
            synthesize_tool(args, agent, agent_data_dir, http_client).await
        }
        tool_names::LIST_GENERATED_TOOLS => list_generated_tools(agent_data_dir),
        tool_names::PROMOTE_GENERATED_TOOL => args
            .get("tool")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
            .and_then(|tool| promote_generated_tool(agent_data_dir, tool)),
        tool_names::ACTIVATE_GENERATED_TOOL => args
            .get("tool")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
            .and_then(|tool| activate_generated_tool(agent_data_dir, tool)),
        tool_names::RESTORE_GENERATED_TOOL => args
            .get("tool")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
            .and_then(|tool| restore_generated_tool(agent_data_dir, tool)),
        tool_names::WEB_SEARCH => {
            let config = agent.config.read().await;
            let search_provider = config
                .extra
                .get("search_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string();
            let exa_api_key = config
                .extra
                .get("exa_api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tavily_api_key = config
                .extra
                .get("tavily_api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let duckduckgo_region = config
                .extra
                .get("duckduckgo_region")
                .and_then(|v| v.as_str())
                .unwrap_or("us-en")
                .to_string();
            let duckduckgo_safe_search = config
                .extra
                .get("duckduckgo_safe_search")
                .and_then(|v| v.as_str())
                .unwrap_or("moderate")
                .to_string();
            drop(config);
            execute_web_search(
                args,
                http_client,
                &search_provider,
                &exa_api_key,
                &tavily_api_key,
                &duckduckgo_region,
                &duckduckgo_safe_search,
            )
            .await
        }
        tool_names::FETCH_URL => {
            let config = agent.config.read().await;
            let browse_provider = config
                .extra
                .get("browse_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string();
            drop(config);
            execute_fetch_url(args, agent, http_client, &browse_provider, thread_id).await
        }
        tool_names::SETUP_WEB_BROWSING => execute_setup_web_browsing(args, agent).await,
        tool_names::PLUGIN_API_CALL => {
            let plugin_name = match get_string_arg(args, &["plugin_name"]) {
                Some(name) => name.to_string(),
                None => {
                    return (
                        Err(anyhow::anyhow!("Error: missing 'plugin_name' argument")),
                        pending_approval,
                    )
                }
            };
            let endpoint_name = match get_string_arg(args, &["endpoint_name"]) {
                Some(name) => name.to_string(),
                None => {
                    return (
                        Err(anyhow::anyhow!("Error: missing 'endpoint_name' argument")),
                        pending_approval,
                    )
                }
            };
            let params = args
                .get("params")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            match agent.plugin_manager.get() {
                Some(pm) => match pm.api_call(&plugin_name, &endpoint_name, params).await {
                    Ok(text) => Ok(text),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                },
                None => Err(anyhow::anyhow!("Plugin system not available")),
            }
        }
        other => match execute_generated_tool(
            other,
            args,
            agent,
            agent_data_dir,
            http_client,
            Some(thread_id),
        )
        .await
        {
            Ok(Some(content)) => Ok(content),
            Ok(None) => {
                maybe_emit_unknown_tool_synthesis_proposal_notice(
                    agent, event_tx, thread_id, other,
                )
                .await;
                Err(anyhow::anyhow!("Unknown tool: {other}"))
            }
            Err(error) => Err(error),
        },
    };

    (result, pending_approval)
}

pub fn execute_tool<'a>(
    tool_call: &'a ToolCall,
    agent: &'a AgentEngine,
    thread_id: &'a str,
    task_id: Option<&'a str>,
    session_manager: &'a Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &'a broadcast::Sender<AgentEvent>,
    agent_data_dir: &'a std::path::Path,
    http_client: &'a reqwest::Client,
    cancel_token: Option<CancellationToken>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
    Box::pin(async move {
        let redacted_arguments = scrub_sensitive(&tool_call.function.arguments);
        tracing::info!(
            tool = %tool_call.function.name,
            args = %redacted_arguments,
            "agent tool call"
        );

        let prepared =
            match Box::pin(prepare_tool_execution(tool_call, agent, thread_id, task_id)).await {
                Ok(prepared) => prepared,
                Err(result) => return result,
            };

        let tool_domain =
            crate::agent::uncertainty::domains::classify_domain(tool_call.function.name.as_str());
        if tool_domain == crate::agent::uncertainty::domains::DomainClassification::Safety {
            let evidence = format!(
                "Safety-domain tool '{}' with blast-radius uncertainty. Args: {}",
                tool_call.function.name,
                tool_call
                    .function
                    .arguments
                    .chars()
                    .take(200)
                    .collect::<String>()
            );
            let _ = event_tx.send(AgentEvent::ConfidenceWarning {
                thread_id: thread_id.to_string(),
                action_type: "tool_call".to_string(),
                band: "medium".to_string(),
                evidence,
                domain: "safety".to_string(),
                blocked: false,
            });
        }

        let (result, pending_approval) = Box::pin(dispatch_tool_execution(
            &prepared,
            agent,
            thread_id,
            task_id,
            session_manager,
            session_id,
            event_tx,
            agent_data_dir,
            http_client,
            cancel_token,
        ))
        .await;

        match result {
            Ok(content) => {
                let content =
                    if should_scrub_successful_tool_result(prepared.dispatch_tool_name.as_str()) {
                        scrub_sensitive(&content)
                    } else {
                        content
                    };
                let mut review = prepared.governance_decision.review.clone();
                annotate_review_with_critique(
                    &mut review,
                    prepared.critique_session_id.as_deref(),
                    prepared.critique_decision.as_deref(),
                    &prepared.critique_adjustments,
                    prepared.critique_report_summary.as_deref(),
                );
                for adjustment in &prepared.timeout_adjustments {
                    let reason = format!("adapted_runtime:{adjustment}");
                    if !review.reasons.iter().any(|existing| existing == &reason) {
                        review.reasons.push(reason);
                    }
                }
                emit_workflow_notice_for_tool(
                    event_tx,
                    thread_id,
                    prepared.dispatch_tool_name.as_str(),
                    &prepared.dispatch_args,
                );
                let notify_on_completion = prepared
                    .dispatch_args
                    .get("notify_on_completion")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                agent
                    .register_operation_wakeups_from_tool_result(
                        thread_id,
                        prepared.dispatch_tool_name.as_str(),
                        &content,
                        notify_on_completion,
                    )
                    .await;
                if prepared.dispatch_tool_name.as_str() == tool_names::FETCH_URL {
                    maybe_emit_openapi_synthesis_proposal_notice(
                        agent,
                        event_tx,
                        thread_id,
                        &prepared.dispatch_args,
                        &content,
                    )
                    .await;
                }
                if matches!(
                    prepared.dispatch_tool_name.as_str(),
                    tool_names::BASH_COMMAND
                        | tool_names::RUN_TERMINAL_COMMAND
                        | tool_names::EXECUTE_MANAGED_COMMAND
                ) {
                    maybe_emit_successful_shell_synthesis_proposal_notice(
                        agent,
                        event_tx,
                        thread_id,
                        prepared.dispatch_tool_name.as_str(),
                        &prepared.dispatch_args,
                    )
                    .await;
                }
                tracing::info!(tool = %prepared.tool_name, result_len = content.len(), "agent tool result: ok");
                ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    content,
                    is_error: false,
                    weles_review: Some(review),
                    pending_approval,
                }
            }
            Err(e) => {
                let content = scrub_sensitive(&format!("Error: {e}"));
                tracing::warn!(tool = %prepared.tool_name, error = %content, "agent tool result: error");
                let mut review = prepared.governance_decision.review.clone();
                annotate_review_with_critique(
                    &mut review,
                    prepared.critique_session_id.as_deref(),
                    prepared.critique_decision.as_deref(),
                    &prepared.critique_adjustments,
                    prepared.critique_report_summary.as_deref(),
                );
                for adjustment in &prepared.timeout_adjustments {
                    let reason = format!("adapted_runtime:{adjustment}");
                    if !review.reasons.iter().any(|existing| existing == &reason) {
                        review.reasons.push(reason);
                    }
                }
                ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    content,
                    is_error: true,
                    weles_review: Some(review),
                    pending_approval: None,
                }
            }
        }
    })
}
