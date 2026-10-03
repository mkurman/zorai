use super::*;
use crate::agent::thread_crud::ThreadListFilter;

const DEFAULT_GET_THREAD_LIMIT: usize = 5;

fn validate_offloaded_payload_id(payload_id: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::path::{Component, Path};

    let mut components = Path::new(payload_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(component)), None)
            if component == OsStr::new(payload_id) && !payload_id.contains('\\') =>
        {
            Ok(())
        }
        _ => anyhow::bail!("offloaded payload not found"),
    }
}

async fn resolve_canonical_offloaded_payload_read_path(
    agent: &AgentEngine,
    thread_id: &str,
    payload_id: &str,
) -> Result<std::path::PathBuf> {
    let root = agent.history.offloaded_payloads_dir();
    let thread_root = root.join(thread_id);
    let derived_path = agent.history.offloaded_payload_path(thread_id, payload_id);
    let canonical_root = tokio::fs::canonicalize(&root).await.with_context(|| {
        format!(
            "failed to resolve offloaded payload root {}",
            root.display()
        )
    })?;
    let canonical_thread_root = tokio::fs::canonicalize(&thread_root)
        .await
        .with_context(|| {
            format!(
                "failed to resolve offloaded payload thread root {}",
                thread_root.display()
            )
        })?;
    let canonical_path = tokio::fs::canonicalize(&derived_path)
        .await
        .with_context(|| {
            format!(
                "failed to resolve offloaded payload {}",
                derived_path.display()
            )
        })?;

    if !canonical_thread_root.starts_with(&canonical_root) {
        anyhow::bail!(
            "offloaded payload thread root {} resolves outside daemon-owned root {}",
            thread_id,
            canonical_root.display()
        );
    }

    if !canonical_path.starts_with(&canonical_root) {
        anyhow::bail!(
            "offloaded payload {} resolves outside daemon-owned root {}",
            payload_id,
            canonical_root.display()
        );
    }

    if !canonical_path.starts_with(&canonical_thread_root) {
        anyhow::bail!("offloaded payload not found");
    }

    Ok(canonical_path)
}

fn parse_non_negative_u64_arg(args: &serde_json::Value, field: &str) -> Result<Option<u64>> {
    if args
        .get(field)
        .is_some_and(|value| value.as_u64().is_none())
    {
        anyhow::bail!("'{field}' must be a non-negative integer");
    }

    Ok(args.get(field).and_then(|value| value.as_u64()))
}

fn parse_non_negative_usize_arg(args: &serde_json::Value, field: &str) -> Result<Option<usize>> {
    let value = parse_non_negative_u64_arg(args, field)?;
    value
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| anyhow::anyhow!("'{field}' is too large for this platform"))
        })
        .transpose()
}

fn parse_optional_bool_arg(args: &serde_json::Value, field: &str) -> Result<Option<bool>> {
    if args.get(field).is_some_and(|value| !value.is_boolean()) {
        anyhow::bail!("'{field}' must be a boolean");
    }

    Ok(args.get(field).and_then(|value| value.as_bool()))
}

fn parse_message_window_bound(
    args: &serde_json::Value,
    primary_field: &str,
    alias_field: &str,
) -> Result<Option<usize>> {
    Ok(parse_non_negative_usize_arg(args, primary_field)?
        .or(parse_non_negative_usize_arg(args, alias_field)?))
}

fn compact_tool_call_name(tool_call: &serde_json::Value) -> Option<String> {
    tool_call
        .get("function")
        .and_then(|function| function.get("name"))
        .or_else(|| tool_call.get("name"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn compact_message_tool_status(message: &serde_json::Value) -> Option<String> {
    let status = message
        .get("tool_status")
        .or_else(|| message.get("status"))
        .and_then(|value| value.as_str())
        .map(str::trim)?;
    let normalized = status.to_ascii_lowercase();
    match normalized.as_str() {
        "done" | "success" | "succeeded" => Some("done".to_string()),
        "error" | "failure" | "failed" => Some("error".to_string()),
        _ => None,
    }
}

fn compact_offloaded_thread_message(message: &serde_json::Value) -> Option<serde_json::Value> {
    let role = message
        .get("role")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let is_tool_message = role == Some("tool")
        || message.get("tool_name").is_some()
        || message.get("tool_call_id").is_some();
    let tool_status = if is_tool_message {
        Some(compact_message_tool_status(message)?)
    } else {
        None
    };

    let mut compact = serde_json::Map::new();
    if let Some(role) = role {
        compact.insert(
            "role".to_string(),
            serde_json::Value::String(role.to_string()),
        );
    }
    if let Some(content) = message.get("content").and_then(|value| value.as_str()) {
        compact.insert(
            "content".to_string(),
            serde_json::Value::String(content.to_string()),
        );
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(|value| value.as_array()) {
        let names = tool_calls
            .iter()
            .filter_map(compact_tool_call_name)
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        if !names.is_empty() {
            compact.insert("tool_calls".to_string(), serde_json::Value::Array(names));
        }
    }
    if let Some(status) = tool_status {
        if let Some(tool_name) = message
            .get("tool_name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            compact.insert(
                "tool_name".to_string(),
                serde_json::Value::String(tool_name.to_string()),
            );
        }
        compact.insert("tool_status".to_string(), serde_json::Value::String(status));
    }

    if compact.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(compact))
    }
}

fn compact_offloaded_thread_payload(
    raw_payload: &str,
    message_start: Option<usize>,
    message_end: Option<usize>,
) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw_payload).ok()?;
    let messages = value
        .get("messages")
        .and_then(|value| value.as_array())
        .or_else(|| {
            value
                .get("thread")
                .and_then(|thread| thread.get("messages"))
                .and_then(|value| value.as_array())
        })?;
    let loaded_message_start = value
        .get("loaded_message_start")
        .or_else(|| {
            value
                .get("thread")
                .and_then(|thread| thread.get("loaded_message_start"))
        })
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let total_message_count = value
        .get("total_message_count")
        .or_else(|| {
            value
                .get("thread")
                .and_then(|thread| thread.get("total_message_count"))
        })
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_else(|| loaded_message_start.saturating_add(messages.len()));
    let available_start = loaded_message_start;
    let available_end = loaded_message_start.saturating_add(messages.len());
    let effective_start = message_start
        .unwrap_or(available_start)
        .max(available_start);
    let effective_end = message_end.unwrap_or(available_end).min(available_end);

    let compact_messages = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            let absolute_index = loaded_message_start.saturating_add(index);
            if absolute_index < effective_start || absolute_index >= effective_end {
                return None;
            }
            compact_offloaded_thread_message(message)
        })
        .collect::<Vec<_>>();

    serde_json::to_string_pretty(&serde_json::json!({
        "total_message_count": total_message_count,
        "loaded_message_start": available_start,
        "loaded_message_end": available_end,
        "message_start": effective_start,
        "message_end": effective_end,
        "returned": compact_messages.len(),
        "messages": compact_messages,
    }))
    .ok()
}

fn compact_offloaded_json_array(
    items: &[serde_json::Value],
    offset: usize,
    limit: usize,
) -> Option<String> {
    let total = items.len();
    let page = items
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let returned = page.len();
    let next_offset = offset.saturating_add(returned);
    let has_more = next_offset < total;

    serde_json::to_string_pretty(&serde_json::json!({
        "total": total,
        "limit": limit,
        "offset": offset,
        "returned": returned,
        "has_more": has_more,
        "next_offset": if has_more { Some(next_offset) } else { None },
        "items": page,
    }))
    .ok()
}

fn compact_offloaded_generic_payload(raw_payload: &str, offset: usize, limit: usize) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw_payload) {
        if let Some(items) = value.as_array() {
            if let Some(compact) = compact_offloaded_json_array(items, offset, limit) {
                return compact;
            }
        }
        if let Some(items) = value.get("items").and_then(|items| items.as_array()) {
            if let Some(compact) = compact_offloaded_json_array(items, offset, limit) {
                return compact;
            }
        }
        let object_keys = value
            .as_object()
            .map(|object| object.keys().take(50).cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        return serde_json::to_string_pretty(&serde_json::json!({
            "content_type": "json",
            "byte_size": raw_payload.len(),
            "object_keys": object_keys,
            "notice": "Raw JSON object omitted from default read; use full=true only when exact content is required.",
        }))
        .unwrap_or_else(|_| "{}".to_string());
    }

    let lines = raw_payload.lines().collect::<Vec<_>>();
    let total_lines = lines.len();
    let page = lines
        .iter()
        .skip(offset)
        .take(limit)
        .map(|line| line.chars().take(1_000).collect::<String>())
        .collect::<Vec<_>>();
    let returned = page.len();
    let next_offset = offset.saturating_add(returned);
    let has_more = next_offset < total_lines;
    serde_json::to_string_pretty(&serde_json::json!({
        "content_type": "text",
        "byte_size": raw_payload.len(),
        "total_lines": total_lines,
        "limit": limit,
        "offset": offset,
        "returned": returned,
        "has_more": has_more,
        "next_offset": if has_more { Some(next_offset) } else { None },
        "lines": page,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub(crate) async fn execute_list_threads(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let filter = ThreadListFilter {
        created_after: parse_non_negative_u64_arg(args, "created_after")?,
        created_before: parse_non_negative_u64_arg(args, "created_before")?,
        updated_after: parse_non_negative_u64_arg(args, "updated_after")?,
        updated_before: parse_non_negative_u64_arg(args, "updated_before")?,
        agent_name: args
            .get("agent_name")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        title_query: args
            .get("title_query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        pinned: args.get("pinned").and_then(|value| value.as_bool()),
        include_internal: args
            .get("include_internal")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        limit: parse_non_negative_usize_arg(args, "limit")?,
        offset: parse_non_negative_usize_arg(args, "offset")?.unwrap_or(0),
    };

    let threads = agent.list_threads_filtered(&filter).await;
    Ok(serde_json::to_string_pretty(&threads).unwrap_or_else(|_| "[]".to_string()))
}

pub(crate) async fn execute_get_thread(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let thread_id = match args.get("thread_id") {
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("'thread_id' must be a non-empty string"))?,
        None => anyhow::bail!("missing 'thread_id' argument"),
    };
    let message_limit = parse_non_negative_usize_arg(args, "limit")?
        .or(parse_non_negative_usize_arg(args, "message_limit")?)
        .unwrap_or(DEFAULT_GET_THREAD_LIMIT);
    let message_offset = parse_non_negative_usize_arg(args, "offset")?.unwrap_or(0);
    let include_internal = args
        .get("include_internal")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let detail = agent
        .get_thread_filtered(
            thread_id,
            include_internal,
            Some(message_limit),
            message_offset,
        )
        .await;

    let Some(detail) = detail else {
        if !include_internal
            && agent
                .get_thread_filtered(thread_id, true, Some(1), 0)
                .await
                .is_some()
        {
            tracing::info!(
                tool = tool_names::GET_THREAD,
                thread_id = %thread_id,
                "masked hidden internal thread as not found"
            );
        }
        anyhow::bail!("thread not found");
    };

    Ok(serde_json::to_string_pretty(&detail).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn execute_read_offloaded_payload(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let requested_payload_id = match args.get("payload_id") {
        Some(value) => value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("'payload_id' must be a non-empty string"))?,
        None => anyhow::bail!("missing 'payload_id' argument"),
    };
    let resolved_payload_id = resolve_payload_id(agent, thread_id, requested_payload_id).await?;
    let payload_id = resolved_payload_id.as_str();
    validate_offloaded_payload_id(payload_id)?;
    let full = parse_optional_bool_arg(args, "full")?.unwrap_or(false);
    let message_start = parse_message_window_bound(args, "message_start", "start")?;
    let message_end = parse_message_window_bound(args, "message_end", "end")?;
    let limit = parse_non_negative_usize_arg(args, "limit")?
        .unwrap_or(20)
        .clamp(1, 100);
    let offset = parse_non_negative_usize_arg(args, "offset")?.unwrap_or(0);
    if matches!(
        (message_start, message_end),
        (Some(message_start), Some(message_end)) if message_end < message_start
    ) {
        anyhow::bail!("'message_end' must be greater than or equal to 'message_start'");
    }

    let metadata = agent
        .history
        .get_offloaded_payload_metadata(payload_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("offloaded payload not found"))?;

    if metadata.thread_id != thread_id {
        anyhow::bail!("offloaded payload not found");
    }

    let derived_path = agent
        .history
        .offloaded_payload_path(&metadata.thread_id, payload_id);
    let derived_storage_path = derived_path.to_string_lossy().into_owned();
    if metadata.storage_path != derived_storage_path {
        tracing::warn!(
            payload_id = %payload_id,
            thread_id = %metadata.thread_id,
            stored_path = %metadata.storage_path,
            canonical_path = %derived_storage_path,
            "offloaded payload metadata path mismatch; reading from canonical daemon path"
        );
    }

    let canonical_path =
        resolve_canonical_offloaded_payload_read_path(agent, &metadata.thread_id, payload_id)
            .await?;

    let raw_payload = tokio::fs::read_to_string(&canonical_path)
        .await
        .with_context(|| {
            format!(
                "failed to read offloaded payload {} from {}",
                payload_id,
                canonical_path.display()
            )
        })?;

    if full {
        return Ok(raw_payload);
    }

    Ok(
        compact_offloaded_thread_payload(&raw_payload, message_start, message_end)
            .unwrap_or_else(|| compact_offloaded_generic_payload(&raw_payload, offset, limit)),
    )
}
