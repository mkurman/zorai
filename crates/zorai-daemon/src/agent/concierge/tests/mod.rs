use super::*;
use crate::agent::engine::AgentEngine;
use crate::session_manager::SessionManager;
use std::collections::HashMap;
use tempfile::tempdir;

mod basic;
mod context;
mod runtime;

fn test_now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn concierge_thread(messages: Vec<AgentMessage>) -> AgentThread {
    AgentThread {
        id: CONCIERGE_THREAD_ID.to_string(),
        agent_name: None,
        title: "Concierge".to_string(),
        created_at: 1,
        updated_at: 1,
        messages,
        pinned: true,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

fn assistant_message(content: &str, timestamp: u64) -> AgentMessage {
    AgentMessage {
        reasoning_tokens: None,
        thinking_ms: None,
        id: format!("assistant-{timestamp}"),
        role: MessageRole::Assistant,
        content: content.to_string(),
        content_blocks: Vec::new(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_arguments: None,
        tool_status: None,
        weles_review: None,
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
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp,
        feedback: None,
    }
}

fn user_message(content: &str, timestamp: u64) -> AgentMessage {
    AgentMessage::user(content, timestamp)
}

fn thread_with_messages(
    id: &str,
    title: &str,
    updated_at: u64,
    messages: Vec<AgentMessage>,
) -> AgentThread {
    AgentThread {
        id: id.to_string(),
        agent_name: None,
        title: title.to_string(),
        created_at: 1,
        updated_at,
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
        messages,
    }
}
