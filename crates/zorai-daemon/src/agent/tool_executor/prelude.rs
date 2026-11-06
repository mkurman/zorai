use super::*;
pub(crate) use std::future::Future;
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::Stdio;
pub(crate) use std::sync::Arc;

pub(crate) use anyhow::{Context, Result};
pub(crate) use base64::Engine;
pub(crate) use tokio::io::AsyncReadExt;
pub(crate) use tokio::sync::broadcast;
pub(crate) use tokio_util::sync::CancellationToken;
pub(crate) use zorai_protocol::{
    tool_names, DaemonMessage, ManagedCommandRequest, ManagedCommandSource, SecurityLevel,
    SessionId,
};

pub(crate) use crate::history::{HistoryStore, SkillVariantRecord};
pub(crate) use crate::scrub::scrub_sensitive;
pub(crate) use crate::session_manager::SessionManager;

pub(crate) use super::super::agent_identity::{
    build_spawned_persona_prompt, canonical_agent_id, canonical_agent_name, current_agent_scope_id,
    extract_persona_name, sender_name_for_task, CONCIERGE_AGENT_ID, CONCIERGE_AGENT_NAME,
    MAIN_AGENT_ID, MAIN_AGENT_NAME,
};
pub(crate) use super::super::memory::{
    apply_memory_update, MemoryTarget, MemoryUpdateMode, MemoryWriteContext,
};
pub(crate) use super::super::semantic_env::{execute_semantic_query, infer_workspace_context_tags};
pub(crate) use super::super::session_recall::execute_session_search as run_session_search;
pub(crate) use super::super::tool_synthesis::{
    activate_generated_tool, detect_cli_wrapper_synthesis_proposal,
    detect_cli_wrapper_synthesis_proposal_from_command, execute_generated_tool,
    find_equivalent_generated_cli_tool, find_equivalent_generated_openapi_tool,
    generated_tool_definitions, list_generated_tools, promote_generated_tool,
    restore_generated_tool, synthesize_tool,
};

pub(crate) use super::super::types::{
    AgentConfig, AgentEvent, NotificationSeverity, TodoItem, TodoStatus, ToolCall, ToolDefinition,
    ToolFunctionDef, ToolPendingApproval, ToolResult,
};
pub(crate) use super::super::AgentEngine;

const ONECONTEXT_TOOL_QUERY_MAX_CHARS: usize = 300;
pub(crate) const ONECONTEXT_TOOL_OUTPUT_MAX_CHARS: usize = 12_000;
const DEFAULT_DAEMON_TOOL_TIMEOUT_SECS: u64 = 120;
const MAX_DAEMON_TOOL_TIMEOUT_SECS: u64 = 600;
pub(crate) const SESSION_SEARCH_OUTPUT_MAX_CHARS: usize = 12_000;
const SEARCH_FILES_MAX_RESULTS_CAP: u64 = 200;
pub(crate) const SEARCH_FILES_PROGRAM: &str = "rg";
pub(crate) const SEARCH_FILES_MAX_LINE_BYTES: usize = 64 * 1024;
pub(crate) const SEARCH_FILES_MAX_STDERR_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct OnecontextSearchRequest {
    pub(crate) bounded_query: String,
    pub(crate) scope: String,
    pub(crate) timeout_seconds: u64,
}

#[derive(Clone)]
pub(crate) struct SearchFilesRequest {
    pub(crate) pattern: String,
    pub(crate) path: String,
    pub(crate) file_pattern: Option<String>,
    pub(crate) regex: bool,
    pub(crate) max_results: u64,
    pub(crate) timeout_seconds: u64,
}

pub(crate) struct SearchFilesCommandOutput {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) truncated: bool,
}

#[derive(Clone)]
pub(crate) struct WebSearchRequest {
    pub(crate) query: String,
    pub(crate) max_results: u64,
    pub(crate) timeout_seconds: u64,
}

#[derive(Clone)]
pub(crate) struct FetchUrlRequest {
    pub(crate) url: String,
    pub(crate) max_length: usize,
    pub(crate) timeout_seconds: u64,
    pub(crate) profile_id: Option<String>,
}

pub(crate) fn default_timeout_seconds_for_tool(tool_name: &str) -> u64 {
    match tool_name {
        tool_names::ONECONTEXT_SEARCH | tool_names::FETCH_URL | tool_names::WEB_SEARCH => 300,
        tool_names::ANALYZE_IMAGE
        | tool_names::GENERATE_IMAGE
        | tool_names::GENERATE_VIDEO
        | tool_names::GENERATE_MUSIC
        | tool_names::SPEECH_TO_TEXT
        | tool_names::TEXT_TO_SPEECH => 600,
        _ => DEFAULT_DAEMON_TOOL_TIMEOUT_SECS,
    }
}

pub(crate) fn daemon_tool_timeout_seconds(tool_name: &str, args: &serde_json::Value) -> u64 {
    args.get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or_else(|| default_timeout_seconds_for_tool(tool_name))
        .min(MAX_DAEMON_TOOL_TIMEOUT_SECS)
}

pub(crate) fn adapted_timeout_override_for_mode(
    tool_name: &str,
    args: &serde_json::Value,
    mode: crate::agent::operator_model::SatisfactionAdaptationMode,
) -> Option<u64> {
    if args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .is_some()
    {
        return None;
    }

    let adapted = match mode {
        crate::agent::operator_model::SatisfactionAdaptationMode::Normal => return None,
        crate::agent::operator_model::SatisfactionAdaptationMode::Tightened => match tool_name {
            tool_names::SEARCH_FILES => 90,
            tool_names::ONECONTEXT_SEARCH | tool_names::FETCH_URL | tool_names::WEB_SEARCH => 240,
            _ => return None,
        },
        crate::agent::operator_model::SatisfactionAdaptationMode::Minimal => match tool_name {
            tool_names::SEARCH_FILES => 90,
            tool_names::ONECONTEXT_SEARCH | tool_names::FETCH_URL | tool_names::WEB_SEARCH => 180,
            _ => return None,
        },
    };

    let default_timeout = default_timeout_seconds_for_tool(tool_name);
    (adapted < default_timeout).then_some(
        adapted
            .min(default_timeout)
            .min(MAX_DAEMON_TOOL_TIMEOUT_SECS),
    )
}

fn normalize_onecontext_simple_query(query: &str) -> String {
    let mut normalized = String::with_capacity(query.len());
    let mut last_was_space = true;

    for ch in query.chars() {
        let mapped = if ch.is_alphanumeric() || ch == '_' {
            ch
        } else if ch.is_whitespace() {
            ' '
        } else {
            ' '
        };

        if mapped == ' ' {
            if !last_was_space {
                normalized.push(' ');
            }
            last_was_space = true;
        } else {
            normalized.push(mapped);
            last_was_space = false;
        }
    }

    normalized.trim().to_string()
}

fn plain_text_onecontext_regex(query: &str) -> String {
    normalize_onecontext_simple_query(query)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(".*")
}

pub(crate) fn prepare_onecontext_search_query(
    query: &str,
    no_regex: bool,
    max_chars: usize,
) -> Option<String> {
    let prepared = if no_regex {
        plain_text_onecontext_regex(query)
    } else {
        query.trim().to_string()
    };

    let bounded_query = prepared.chars().take(max_chars).collect::<String>();
    if bounded_query.trim().is_empty() {
        return None;
    }

    Some(bounded_query)
}

pub(crate) fn onecontext_search_request(
    args: &serde_json::Value,
) -> Result<OnecontextSearchRequest> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?
        .trim();

    if query.is_empty() {
        return Err(anyhow::anyhow!("'query' must not be empty"));
    }

    let scope = args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("session");
    if !matches!(scope, "session" | "event" | "turn") {
        return Err(anyhow::anyhow!(
            "invalid 'scope': {scope} (expected session, event, or turn)"
        ));
    }

    if args
        .get("timeout_seconds")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(anyhow::anyhow!(
            "'timeout_seconds' must be a non-negative integer"
        ));
    }

    let no_regex = args
        .get("no_regex")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let Some(bounded_query) =
        prepare_onecontext_search_query(query, no_regex, ONECONTEXT_TOOL_QUERY_MAX_CHARS)
    else {
        return Err(anyhow::anyhow!(
            "'query' must contain at least one searchable keyword"
        ));
    };

    Ok(OnecontextSearchRequest {
        bounded_query,
        scope: scope.to_string(),
        timeout_seconds: daemon_tool_timeout_seconds(tool_names::ONECONTEXT_SEARCH, args),
    })
}

pub(crate) fn search_files_request(args: &serde_json::Value) -> Result<SearchFilesRequest> {
    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'pattern' argument"))?
        .trim();

    if pattern.is_empty() {
        return Err(anyhow::anyhow!("'pattern' must not be empty"));
    }

    if args
        .get("timeout_seconds")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(anyhow::anyhow!(
            "'timeout_seconds' must be a non-negative integer"
        ));
    }

    if args
        .get("max_results")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(anyhow::anyhow!(
            "'max_results' must be a non-negative integer"
        ));
    }

    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    validate_read_path(&path)?;
    let path = resolve_search_files_path(&path)?;

    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50)
        .max(1)
        .min(SEARCH_FILES_MAX_RESULTS_CAP);

    Ok(SearchFilesRequest {
        pattern: pattern.to_string(),
        path,
        file_pattern: args
            .get("file_pattern")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        regex: args.get("regex").and_then(|v| v.as_bool()).unwrap_or(false),
        max_results,
        timeout_seconds: daemon_tool_timeout_seconds(tool_names::SEARCH_FILES, args),
    })
}

pub(crate) use super::super::llm_client::{ApiContent, ApiMessage, CompletionChunk, RetryStrategy};
pub(crate) use super::super::task_prompt::now_millis;
pub(crate) use super::super::task_scheduler::selection::make_task_log_entry;
pub(crate) use super::super::types::CompletionProviderFinalResult;
#[cfg(test)]
pub(crate) use super::super::types::TaskStatus;
pub(crate) use super::super::types::{
    AgentContentBlock, AgentMessage, AgentMessageKind, AgentTask, ApiTransport, ApiType,
    ExternalRuntimeConflictPolicy, MessageRole, TaskLogLevel, ThreadExecutionProfile,
};
