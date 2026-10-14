use super::*;
pub(crate) async fn persist_gateway_health_update(
    agent: &Arc<AgentEngine>,
    update: GatewayHealthState,
) -> Result<()> {
    let updated_at_ms = current_time_ms();
    agent
        .history
        .upsert_gateway_health_snapshot(&update, updated_at_ms)
        .await?;

    let platform_key = update.platform.to_ascii_lowercase();
    let platform_label = match platform_key.as_str() {
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        other => other,
    }
    .to_string();
    let new_status = match update.status {
        GatewayConnectionStatus::Connected => {
            crate::agent::RuntimeGatewayConnectionStatus::Connected
        }
        GatewayConnectionStatus::Disconnected => {
            crate::agent::RuntimeGatewayConnectionStatus::Disconnected
        }
        GatewayConnectionStatus::Error => crate::agent::RuntimeGatewayConnectionStatus::Error,
    };
    let mut previous_status = None;

    let mut gw_guard = agent.gateway_state.lock().await;
    if let Some(gateway_state) = gw_guard.as_mut() {
        match platform_key.as_str() {
            "slack" => {
                previous_status = Some(gateway_state.slack_health.status);
                gateway_state.slack_health.status = new_status;
                gateway_state.slack_health.last_success_at = update.last_success_at_ms;
                gateway_state.slack_health.last_error_at = update.last_error_at_ms;
                gateway_state.slack_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.slack_health.last_error = update.last_error.clone();
                gateway_state.slack_health.current_backoff_secs = update.current_backoff_secs;
            }
            "discord" => {
                previous_status = Some(gateway_state.discord_health.status);
                gateway_state.discord_health.status = new_status;
                gateway_state.discord_health.last_success_at = update.last_success_at_ms;
                gateway_state.discord_health.last_error_at = update.last_error_at_ms;
                gateway_state.discord_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.discord_health.last_error = update.last_error.clone();
                gateway_state.discord_health.current_backoff_secs = update.current_backoff_secs;
            }
            "telegram" => {
                previous_status = Some(gateway_state.telegram_health.status);
                gateway_state.telegram_health.status = new_status;
                gateway_state.telegram_health.last_success_at = update.last_success_at_ms;
                gateway_state.telegram_health.last_error_at = update.last_error_at_ms;
                gateway_state.telegram_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.telegram_health.last_error = update.last_error.clone();
                gateway_state.telegram_health.current_backoff_secs = update.current_backoff_secs;
            }
            _ => {}
        }
    }

    drop(gw_guard);

    if previous_status != Some(new_status) {
        let status = match update.status {
            GatewayConnectionStatus::Connected => "connected",
            GatewayConnectionStatus::Disconnected => "disconnected",
            GatewayConnectionStatus::Error => "error",
        }
        .to_string();
        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::GatewayStatus {
                platform: platform_label.clone(),
                status: status.clone(),
                last_error: update.last_error.clone(),
                consecutive_failures: Some(update.consecutive_failure_count),
            });

        let description = match (status.as_str(), update.last_error.as_deref()) {
            ("connected", _) => format!("{platform_label} reconnected"),
            ("error", Some(err)) => format!(
                "{platform_label} disconnected after {} failures: {err}",
                update.consecutive_failure_count
            ),
            ("error", None) => format!("{platform_label} disconnected"),
            ("disconnected", _) => format!("{platform_label} disconnected"),
            _ => format!("{platform_label} status: {status}"),
        };

        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::HeartbeatDigest {
                cycle_id: format!("gateway_health_{}", uuid::Uuid::new_v4()),
                actionable: status != "connected",
                digest: description.clone(),
                items: vec![crate::agent::types::HeartbeatDigestItem {
                    priority: if status == "connected" { 3 } else { 1 },
                    check_type: crate::agent::types::HeartbeatCheckType::UnrepliedGatewayMessages,
                    title: description.clone(),
                    suggestion: if status == "connected" {
                        format!("{platform_label} is back online")
                    } else {
                        format!("Check {platform_label} API credentials and connectivity")
                    },
                }],
                checked_at: updated_at_ms,
                explanation: None,
                confidence: None,
            });

        let audit_entry = crate::history::AuditEntryRow {
            id: format!("gw_health_{}", uuid::Uuid::new_v4()),
            timestamp: updated_at_ms as i64,
            action_type: "gateway_health_transition".to_string(),
            summary: format!("{platform_label} -> {status}"),
            explanation: update.last_error.clone(),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: Some(
                serde_json::json!({
                    "platform": platform_label,
                    "new_status": status,
                    "consecutive_failures": update.consecutive_failure_count,
                })
                .to_string(),
            ),
        };
        agent.history.insert_action_audit(&audit_entry).await?;
    }

    Ok(())
}

pub(crate) fn gateway_response_channel_key(platform: &str, channel_id: &str) -> Option<String> {
    Some(crate::agent::gateway::gateway_channel_key(
        platform, channel_id,
    ))
}

pub(crate) fn is_expected_disconnect_error(error: &anyhow::Error) -> bool {
    let expected_io_error = |kind: std::io::ErrorKind| {
        matches!(
            kind,
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
        )
    };
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| expected_io_error(io.kind()))
    }) {
        return true;
    }

    let message = error.to_string().to_ascii_lowercase();
    message.contains("unexpected end of file")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
}

fn normalize_session_tag(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            last_was_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '/' | '\\' | ' ' | ':' | '_' | '.' | '-') {
            if last_was_dash {
                None
            } else {
                last_was_dash = true;
                Some('-')
            }
        } else {
            None
        };
        if let Some(ch) = mapped {
            normalized.push(ch);
        }
    }

    let trimmed = normalized.trim_matches('-');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

use zorai_shared::text::{utf8_prefix, utf8_suffix};

fn cap_vec_prefix_for_ipc<T, F>(items: Vec<T>, fits: F) -> (Vec<T>, bool)
where
    T: Clone,
    F: Fn(&[T]) -> bool,
{
    if fits(&items) {
        return (items, false);
    }

    let mut low = 0usize;
    let mut high = items.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(&items[..mid]) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (items.into_iter().take(low).collect(), true)
}

fn cap_vec_suffix_for_ipc<T, F>(items: Vec<T>, fits: F) -> (Vec<T>, bool)
where
    T: Clone,
    F: Fn(&[T]) -> bool,
{
    if fits(&items) {
        return (items, false);
    }

    let mut low = 0usize;
    let mut high = items.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if fits(&items[mid..]) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (items.into_iter().skip(low).collect(), true)
}

fn cap_string_prefix_for_ipc<F>(value: String, fits: F) -> (String, bool)
where
    F: Fn(&str) -> bool,
{
    if fits(&value) {
        return (value, false);
    }

    let mut low = 0usize;
    let mut high = value.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(utf8_prefix(&value, mid)) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (utf8_prefix(&value, low).to_string(), true)
}

fn cap_string_suffix_for_ipc<F>(value: String, fits: F) -> (String, bool)
where
    F: Fn(&str) -> bool,
{
    if fits(&value) {
        return (value, false);
    }

    let mut low = 0usize;
    let mut high = value.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if fits(utf8_suffix(&value, mid)) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (utf8_suffix(&value, low).to_string(), true)
}

fn cap_bytes_suffix_for_ipc<F>(value: Vec<u8>, fits: F) -> (Vec<u8>, bool)
where
    F: Fn(&[u8]) -> bool,
{
    if fits(&value) {
        return (value, false);
    }

    let mut low = 0usize;
    let mut high = value.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if fits(&value[mid..]) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (value.into_iter().skip(low).collect(), true)
}

const MAX_THREAD_DETAIL_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) fn thread_detail_fits_single_ipc_frame(thread_json: &str) -> bool {
    zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::AgentThreadDetail {
        thread_json: thread_json.to_string(),
    })
}

pub(crate) fn thread_detail_chunks_for_ipc(thread_json: &str) -> impl Iterator<Item = &[u8]> {
    thread_json.as_bytes().chunks(MAX_THREAD_DETAIL_CHUNK_BYTES)
}

pub(crate) fn cap_agent_thread_list_for_ipc(
    threads: Vec<crate::agent::types::AgentThread>,
) -> (Vec<crate::agent::types::AgentThread>, bool) {
    if let Ok(threads_json) = serde_json::to_string(&threads) {
        if agent_event_payload_fits_ipc_fast(threads_json.len()) {
            return (threads, false);
        }
    }

    cap_vec_prefix_for_ipc(threads, |candidate| {
        let Ok(threads_json) = serde_json::to_string(candidate) else {
            return false;
        };
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::AgentThreadList { threads_json })
    })
}

pub(crate) fn cap_scrollback_for_ipc(
    id: zorai_protocol::SessionId,
    data: Vec<u8>,
) -> (Vec<u8>, bool) {
    if agent_event_payload_fits_ipc_fast(data.len()) {
        return (data, false);
    }
    cap_bytes_suffix_for_ipc(data, |candidate| {
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::Scrollback {
            id,
            data: candidate.to_vec(),
        })
    })
}

pub(crate) fn cap_analysis_result_for_ipc(
    id: zorai_protocol::SessionId,
    result: String,
) -> (String, bool) {
    if agent_event_payload_fits_ipc_fast(result.len()) {
        return (result, false);
    }
    cap_string_suffix_for_ipc(result, |candidate| {
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::AnalysisResult {
            id,
            result: candidate.to_string(),
        })
    })
}

pub(crate) fn cap_history_search_result_for_ipc(
    query: &str,
    summary: String,
    hits: Vec<zorai_protocol::HistorySearchHit>,
) -> (String, Vec<zorai_protocol::HistorySearchHit>, bool) {
    let raw_estimate = query
        .len()
        .saturating_add(summary.len())
        .saturating_add(hits.iter().map(estimate_hit_size).sum::<usize>());
    if agent_event_payload_fits_ipc_fast(raw_estimate) {
        return (summary, hits, false);
    }

    let query_owned = query.to_string();
    let fits = |summary: &str, hits: &[zorai_protocol::HistorySearchHit]| {
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::HistorySearchResult {
            query: query_owned.clone(),
            summary: summary.to_string(),
            hits: hits.to_vec(),
        })
    };

    if fits(&summary, &hits) {
        return (summary, hits, false);
    }

    let (hits, hits_truncated) =
        cap_vec_prefix_for_ipc(hits, |candidate| fits(&summary, candidate));
    if fits(&summary, &hits) {
        return (summary, hits, true);
    }

    let (summary, summary_truncated) =
        cap_string_prefix_for_ipc(summary, |candidate| fits(candidate, &hits));
    (summary, hits, hits_truncated || summary_truncated)
}

fn estimate_hit_size(hit: &zorai_protocol::HistorySearchHit) -> usize {
    64 + hit.id.len()
        + hit.kind.len()
        + hit.title.len()
        + hit.excerpt.len()
        + hit.path.as_deref().map(str::len).unwrap_or(0)
}

pub(crate) fn cap_agent_db_thread_detail_for_ipc(
    thread: Option<zorai_protocol::AgentDbThread>,
    messages: Vec<zorai_protocol::AgentDbMessage>,
) -> ((String, String), bool) {
    let Ok(thread_json) = serde_json::to_string(&thread) else {
        return ((String::new(), String::new()), true);
    };

    let messages_json = match serde_json::to_string(&messages) {
        Ok(json) => json,
        Err(_) => return ((thread_json, String::new()), true),
    };
    if agent_event_payload_fits_ipc_fast(thread_json.len() + messages_json.len()) {
        return ((thread_json, messages_json), false);
    }

    let per_message_json: Vec<String> = messages
        .iter()
        .map(|message| serde_json::to_string(message).unwrap_or_default())
        .collect();

    let array_overhead = |count: usize| -> usize {
        if count == 0 {
            2
        } else {
            2 + (count - 1)
        }
    };
    let budget = zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES.saturating_sub(thread_json.len() + 4096);

    let mut keep_count = 0usize;
    let mut total_len = 0usize;
    for json in per_message_json.iter().rev() {
        let next_len =
            total_len + json.len() + array_overhead(keep_count + 1) - array_overhead(keep_count);
        if next_len > budget {
            break;
        }
        total_len = next_len;
        keep_count += 1;
    }

    let truncated = keep_count < messages.len();
    let kept_messages: Vec<zorai_protocol::AgentDbMessage> = messages
        .into_iter()
        .skip(per_message_json.len() - keep_count)
        .collect();
    let messages_json = serde_json::to_string(&kept_messages).unwrap_or_default();
    ((thread_json, messages_json), truncated)
}

pub(crate) fn cap_agent_event_rows_for_ipc(
    events: Vec<zorai_protocol::AgentEventRow>,
) -> (String, bool) {
    if let Ok(events_json) = serde_json::to_string(&events) {
        if agent_event_payload_fits_ipc_fast(events_json.len()) {
            return (events_json, false);
        }
    }

    let fits = |candidate: &[zorai_protocol::AgentEventRow]| {
        let Ok(events_json) = serde_json::to_string(candidate) else {
            return false;
        };
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::AgentEventRows { events_json })
    };

    let (events, truncated) = cap_vec_suffix_for_ipc(events, fits);
    (
        serde_json::to_string(&events).unwrap_or_default(),
        truncated,
    )
}

pub(crate) fn cap_git_diff_for_ipc(
    repo_path: &str,
    file_path: Option<&str>,
    diff: String,
) -> (String, bool) {
    if agent_event_payload_fits_ipc_fast(
        diff.len() + repo_path.len() + file_path.map(str::len).unwrap_or(0),
    ) {
        return (diff, false);
    }
    let repo_path = repo_path.to_string();
    let file_path = file_path.map(ToOwned::to_owned);
    cap_string_prefix_for_ipc(diff, |candidate| {
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::GitDiff {
            repo_path: repo_path.clone(),
            file_path: file_path.clone(),
            diff: candidate.to_string(),
        })
    })
}

pub(crate) fn cap_plugin_api_call_result_for_ipc(
    operation_id: Option<&str>,
    plugin_name: &str,
    endpoint_name: &str,
    success: bool,
    result: String,
    error_type: Option<&str>,
) -> (String, bool) {
    let raw_estimate = result.len()
        + plugin_name.len()
        + endpoint_name.len()
        + operation_id.map(str::len).unwrap_or(0)
        + error_type.map(str::len).unwrap_or(0);
    if agent_event_payload_fits_ipc_fast(raw_estimate) {
        return (result, false);
    }
    let operation_id = operation_id.map(ToOwned::to_owned);
    let plugin_name = plugin_name.to_string();
    let endpoint_name = endpoint_name.to_string();
    let error_type = error_type.map(ToOwned::to_owned);
    cap_string_prefix_for_ipc(result, |candidate| {
        zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::PluginApiCallResult {
            operation_id: operation_id.clone(),
            plugin_name: plugin_name.clone(),
            endpoint_name: endpoint_name.clone(),
            success,
            result: candidate.to_string(),
            error_type: error_type.clone(),
        })
    })
}

fn agent_event_frame_fits_ipc(event_json: &str) -> bool {
    zorai_protocol::daemon_message_fits_ipc(&DaemonMessage::AgentEvent {
        event_json: event_json.to_string(),
    })
}

/// Cheap upper-bound check: if the raw payload plus a small wrapper budget
/// fits the IPC frame, sending is guaranteed to succeed.
///
/// The IPC codec uses bincode (see `zorai-protocol::codec::serialize_payload`),
/// so the wrapper around an already-serialized `String` field adds only a
/// length prefix (~8 bytes per field) plus the variant tag — tens of bytes
/// total. 4 KiB of slack is wildly more than that and lets us avoid the
/// ~O(payload_size) allocation that `daemon_message_fits_ipc` does just to
/// measure the encoded length.
fn agent_event_payload_fits_ipc_fast(payload_len: usize) -> bool {
    const WRAPPER_BUDGET: usize = 4096;
    payload_len.saturating_add(WRAPPER_BUDGET) <= zorai_protocol::MAX_IPC_FRAME_SIZE_BYTES
}

pub(crate) fn cap_agent_event_for_ipc(
    event: &crate::agent::types::AgentEvent,
) -> Option<(String, bool)> {
    let event_json = serde_json::to_string(event).ok()?;
    if agent_event_payload_fits_ipc_fast(event_json.len()) {
        return Some((event_json, false));
    }
    if agent_event_frame_fits_ipc(&event_json) {
        return Some((event_json, false));
    }

    if let crate::agent::types::AgentEvent::WorkflowNotice {
        thread_id,
        kind,
        message,
        ..
    } = event
    {
        let without_details = crate::agent::types::AgentEvent::WorkflowNotice {
            thread_id: thread_id.clone(),
            kind: kind.clone(),
            message: message.clone(),
            details: None,
        };
        let without_details_json = serde_json::to_string(&without_details).ok()?;
        if agent_event_frame_fits_ipc(&without_details_json) {
            return Some((without_details_json, true));
        }

        let (capped_message, _truncated) =
            cap_string_prefix_for_ipc(message.clone(), |candidate| {
                let candidate_event = crate::agent::types::AgentEvent::WorkflowNotice {
                    thread_id: thread_id.clone(),
                    kind: kind.clone(),
                    message: candidate.to_string(),
                    details: None,
                };
                serde_json::to_string(&candidate_event)
                    .map(|json| agent_event_frame_fits_ipc(&json))
                    .unwrap_or(false)
            });
        let candidate_event = crate::agent::types::AgentEvent::WorkflowNotice {
            thread_id: thread_id.clone(),
            kind: kind.clone(),
            message: capped_message,
            details: None,
        };
        let candidate_json = serde_json::to_string(&candidate_event).ok()?;
        if agent_event_frame_fits_ipc(&candidate_json) {
            return Some((candidate_json, true));
        }
    }

    if let Some(thread_id) = agent_event_thread_id(event) {
        let fallback = crate::agent::types::AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        };
        let fallback_json = serde_json::to_string(&fallback).ok()?;
        if agent_event_frame_fits_ipc(&fallback_json) {
            return Some((fallback_json, true));
        }
    }

    let fallback = crate::agent::types::AgentEvent::WorkflowNotice {
        thread_id: agent_event_thread_id(event).unwrap_or_default().to_string(),
        kind: "oversized-agent-event".to_string(),
        message: "Large live update was truncated to fit IPC".to_string(),
        details: None,
    };
    let fallback_json = serde_json::to_string(&fallback).ok()?;
    if agent_event_frame_fits_ipc(&fallback_json) {
        return Some((fallback_json, true));
    }

    None
}

fn summarize_session_output(recent_output: Option<&str>) -> Option<String> {
    let line = recent_output?
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let condensed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if condensed.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 120;
    if condensed.chars().count() <= MAX_CHARS {
        return Some(condensed);
    }
    Some(condensed.chars().take(MAX_CHARS - 3).collect::<String>() + "...")
}

pub(crate) fn build_session_end_episode_payload(
    session_id: &str,
    info: Option<&SessionInfo>,
    recent_output: Option<&str>,
) -> (String, Vec<String>) {
    let title = info
        .and_then(|value| value.title.as_deref())
        .filter(|value| !value.is_empty());
    let active_command = info
        .and_then(|value| value.active_command.as_deref())
        .filter(|value| !value.is_empty());
    let cwd = info
        .and_then(|value| value.cwd.as_deref())
        .filter(|value| !value.is_empty());
    let cwd_label = cwd
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str());
    let recent_output_summary = summarize_session_output(recent_output);

    let focus = title
        .map(ToOwned::to_owned)
        .or_else(|| active_command.map(ToOwned::to_owned))
        .or_else(|| cwd_label.map(|value| format!("workspace {value}")))
        .unwrap_or_else(|| format!("session {session_id}"));

    let mut summary = format!("{focus} ended");
    if let Some(cwd_label) = cwd_label {
        summary.push_str(&format!(" in {cwd_label}"));
    }
    if let Some(output) = recent_output_summary {
        summary.push_str(&format!(". Last output: {output}"));
    }

    let mut entities = vec![format!("session:{session_id}")];
    if let Some(workspace_id) = info.and_then(|value| value.workspace_id.as_deref()) {
        entities.push(format!("workspace:{workspace_id}"));
    }
    if let Some(tag) = cwd_label.and_then(normalize_session_tag) {
        entities.push(format!("cwd:{tag}"));
    }
    if let Some(command) = active_command
        .and_then(|value| value.split_whitespace().next())
        .and_then(normalize_session_tag)
    {
        entities.push(format!("command:{command}"));
    }
    if let Some(tag) = title.and_then(normalize_session_tag) {
        entities.push(format!("title:{tag}"));
    }
    entities.sort();
    entities.dedup();

    (summary, entities)
}

pub(crate) fn agent_event_thread_id(event: &crate::agent::types::AgentEvent) -> Option<&str> {
    use crate::agent::types::AgentEvent;

    match event {
        AgentEvent::Delta { thread_id, .. }
        | AgentEvent::Reasoning { thread_id, .. }
        | AgentEvent::ToolCall { thread_id, .. }
        | AgentEvent::ToolResult { thread_id, .. }
        | AgentEvent::Done { thread_id, .. }
        | AgentEvent::Error { thread_id, .. }
        | AgentEvent::TurnInterrupted { thread_id, .. }
        | AgentEvent::ThreadCreated { thread_id, .. }
        | AgentEvent::ThreadTitleUpdated { thread_id, .. }
        | AgentEvent::ThreadReloadRequired { thread_id, .. }
        | AgentEvent::ContextWindowUpdate { thread_id, .. }
        | AgentEvent::ParticipantSuggestion { thread_id, .. }
        | AgentEvent::TodoUpdate { thread_id, .. }
        | AgentEvent::WorkContextUpdate { thread_id, .. }
        | AgentEvent::RetryStatus { thread_id, .. }
        | AgentEvent::WorkflowNotice { thread_id, .. }
        | AgentEvent::ModeShift { thread_id, .. }
        | AgentEvent::ConfidenceWarning { thread_id, .. }
        | AgentEvent::CounterWhoAlert { thread_id, .. }
        | AgentEvent::MessageFeedbackUpdated { thread_id, .. }
        | AgentEvent::PromptQueueUpdate { thread_id, .. }
        | AgentEvent::QueuedPromptDelivered { thread_id, .. }
        | AgentEvent::ApprovalRequired { thread_id, .. } => Some(thread_id.as_str()),
        AgentEvent::OperatorQuestion { thread_id, .. }
        | AgentEvent::OperatorQuestionResolved { thread_id, .. } => thread_id.as_deref(),
        AgentEvent::TaskUpdate {
            task: Some(task), ..
        } => task
            .thread_id
            .as_deref()
            .or(task.parent_thread_id.as_deref()),
        AgentEvent::GoalRunUpdate {
            goal_run: Some(goal_run),
            ..
        } => goal_run.thread_id.as_deref(),
        _ => None,
    }
}

#[cfg(not(test))]
pub(crate) async fn start_whatsapp_link_backend(agent: Arc<AgentEngine>) -> Result<()> {
    crate::agent::start_whatsapp_link_native(agent).await
}

#[cfg(not(test))]
pub(crate) async fn maybe_autostart_whatsapp_link(agent: Arc<AgentEngine>) {
    let gateway = agent.config.read().await.gateway.clone();
    if !gateway.enabled {
        return;
    }

    let persisted_path = crate::agent::whatsapp_native_store_path(&agent.data_dir);
    if !persisted_path.exists() {
        tracing::info!(
            persisted_path = %persisted_path.display(),
            "whatsapp link autostart skipped (no persisted daemon state)"
        );
        return;
    }

    tracing::info!(
        persisted_path = %persisted_path.display(),
        "whatsapp link autostarting from persisted daemon state"
    );
    match agent.whatsapp_link.start_if_idle().await {
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                "whatsapp link autostart: failed to transition runtime to starting"
            );
            return;
        }
    }
    if let Err(error) = start_whatsapp_link_backend(agent.clone()).await {
        tracing::warn!(
            error = %error,
            "whatsapp link autostart: failed to start backend"
        );
        agent
            .whatsapp_link
            .broadcast_error(error.to_string(), false)
            .await;
    }
}
