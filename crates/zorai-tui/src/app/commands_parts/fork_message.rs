use super::*;
use crate::state::chat;
use serde_json::json;
use zorai_protocol::{AgentDbMessage, AgentDbThread};

impl TuiModel {
    pub(crate) fn fork_message(&mut self, index: usize) {
        let Some(parent) = self.chat.active_thread().cloned() else {
            return;
        };
        if index >= parent.messages.len() {
            return;
        }
        let Some(forked_parent_message_id) = parent.messages[index]
            .id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .map(str::to_string)
        else {
            self.status_line = "Cannot fork message before it is saved".to_string();
            return;
        };
        self.chat
            .mark_message_forking(index, self.tick_counter.saturating_add(20));

        let now = current_time_millis();
        let fork_thread_id = format!("fork-{}-{now}", parent.id);
        let forked_messages = forked_message_prefix(&parent.messages, index, &fork_thread_id);
        let title = fork_thread_title(&parent, index);
        let absolute_message_index = parent.loaded_message_start.saturating_add(index);
        let effective_profile = effective_thread_profile(&parent);
        let total_input_tokens = forked_messages
            .iter()
            .map(|message| message.input_tokens)
            .sum::<u64>();
        let total_output_tokens = forked_messages
            .iter()
            .map(|message| message.output_tokens)
            .sum::<u64>();
        let selected_preview = forked_messages
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();

        let fork_thread = chat::AgentThread {
            id: fork_thread_id.clone(),
            agent_name: parent.agent_name.clone(),
            profile_provider: effective_profile.provider.clone(),
            profile_model: effective_profile.model.clone(),
            profile_reasoning_effort: effective_profile.reasoning_effort.clone(),
            profile_context_window_tokens: effective_profile.context_window_tokens,
            title: title.clone(),
            created_at: now,
            updated_at: now,
            messages: forked_messages.clone(),
            total_message_count: forked_messages.len(),
            loaded_message_start: 0,
            loaded_message_end: forked_messages.len(),
            total_input_tokens,
            total_output_tokens,
            runtime_provider: parent.runtime_provider.clone(),
            runtime_model: parent.runtime_model.clone(),
            runtime_reasoning_effort: parent.runtime_reasoning_effort.clone(),
            ..Default::default()
        };

        let thread_json = match serde_json::to_string(&AgentDbThread {
            id: fork_thread_id.clone(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: parent.agent_name.clone(),
            title,
            created_at: now as i64,
            updated_at: now as i64,
            message_count: forked_messages.len() as i64,
            total_tokens: total_input_tokens.saturating_add(total_output_tokens) as i64,
            last_preview: truncate_preview(&selected_preview, 240),
            metadata_json: fork_thread_metadata_json(
                &parent,
                &forked_messages,
                &fork_thread_id,
                absolute_message_index,
                &forked_parent_message_id,
                now,
                &effective_profile,
            ),
        }) {
            Ok(json) => json,
            Err(err) => {
                self.status_line = format!("Failed to build fork thread payload: {err}");
                return;
            }
        };

        let messages_json = match forked_messages
            .iter()
            .enumerate()
            .map(|(message_index, message)| {
                serde_json::to_string(&agent_db_message_for_fork(
                    message,
                    &fork_thread_id,
                    message_index,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(messages) => messages,
            Err(err) => {
                self.status_line = format!("Failed to build fork message payload: {err}");
                return;
            }
        };

        self.chat
            .reduce(chat::ChatAction::ThreadDetailReceived(fork_thread));
        self.chat
            .reduce(chat::ChatAction::SelectThread(fork_thread_id.clone()));
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.status_line = format!("Forked thread at message {}", index + 1);
        self.send_daemon_command(DaemonCommand::ForkThread {
            thread_id: fork_thread_id,
            thread_json,
            messages_json,
            refresh_message_limit: self
                .chat_history_delete_backfill_target_size()
                .max(forked_messages.len()),
        });
    }
}

fn forked_message_prefix(
    messages: &[chat::AgentMessage],
    end_index: usize,
    fork_thread_id: &str,
) -> Vec<chat::AgentMessage> {
    messages
        .iter()
        .take(end_index.saturating_add(1))
        .enumerate()
        .map(|(index, message)| {
            let mut forked = message.clone();
            forked.id = Some(fork_message_id(fork_thread_id, index));
            forked.is_streaming = false;
            forked
        })
        .collect()
}

fn agent_db_message_for_fork(
    message: &chat::AgentMessage,
    fork_thread_id: &str,
    index: usize,
) -> AgentDbMessage {
    let input_tokens = nonzero_i64(message.input_tokens);
    let output_tokens = nonzero_i64(message.output_tokens);
    AgentDbMessage {
        id: fork_message_id(fork_thread_id, index),
        thread_id: fork_thread_id.to_string(),
        created_at: message.timestamp as i64,
        role: role_to_db(&message.role).to_string(),
        content: message.content.clone(),
        provider: None,
        model: None,
        input_tokens,
        output_tokens,
        total_tokens: nonzero_i64(message.input_tokens.saturating_add(message.output_tokens)),
        cost_usd: message.cost,
        reasoning: message.reasoning.clone(),
        tool_calls_json: None,
        metadata_json: Some(message_metadata_json(message)),
    }
}

fn message_metadata_json(message: &chat::AgentMessage) -> String {
    json!({
        "tool_call_id": message.tool_call_id,
        "toolCallId": message.tool_call_id,
        "tool_name": message.tool_name,
        "toolName": message.tool_name,
        "tool_arguments": message.tool_arguments,
        "toolArguments": message.tool_arguments,
        "tool_status": message.tool_status,
        "toolStatus": message.tool_status,
        "content_blocks": content_blocks_json(&message.content_blocks),
        "contentBlocks": content_blocks_json(&message.content_blocks),
        "weles_review": message.weles_review.as_ref().map(weles_review_json),
        "provider_final_result": message.provider_final_result_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok()),
        "author_agent_id": message.author_agent_id,
        "authorAgentId": message.author_agent_id,
        "author_agent_name": message.author_agent_name,
        "authorAgentName": message.author_agent_name,
        "is_operator_question": message.is_operator_question,
        "operator_question_id": message.operator_question_id,
        "operator_question_answer": message.operator_question_answer,
        "message_kind": message.message_kind,
        "compaction_strategy": message.compaction_strategy,
        "compaction_payload": message.compaction_payload,
        "tool_output_preview_path": message.tool_output_preview_path,
        "toolOutputPreviewPath": message.tool_output_preview_path,
        "pinned_for_compaction": message.pinned_for_compaction,
        "pinnedForCompaction": message.pinned_for_compaction,
        "feedback": message.feedback.map(|reaction| match reaction {
            zorai_protocol::Reaction::Up => "up",
            zorai_protocol::Reaction::Down => "down",
        }),
    })
    .to_string()
}

#[derive(Debug, Clone, Default)]
struct ForkThreadProfile {
    provider: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    context_window_tokens: Option<u32>,
}

fn effective_thread_profile(parent: &chat::AgentThread) -> ForkThreadProfile {
    ForkThreadProfile {
        provider: nonempty_string(parent.profile_provider.as_ref())
            .or_else(|| nonempty_string(parent.runtime_provider.as_ref())),
        model: nonempty_string(parent.profile_model.as_ref())
            .or_else(|| nonempty_string(parent.runtime_model.as_ref())),
        reasoning_effort: nonempty_string(parent.profile_reasoning_effort.as_ref())
            .or_else(|| nonempty_string(parent.runtime_reasoning_effort.as_ref())),
        context_window_tokens: parent.profile_context_window_tokens,
    }
}

#[derive(Debug, Clone)]
struct ForkResponder {
    agent_id: String,
    agent_name: String,
}

fn fork_responder(parent: &chat::AgentThread, messages: &[chat::AgentMessage]) -> ForkResponder {
    if let Some(responder) = messages.iter().rev().find_map(|message| {
        let agent_id = nonempty_string(message.author_agent_id.as_ref())?;
        let agent_name = nonempty_string(message.author_agent_name.as_ref())
            .or_else(|| nonempty_string(parent.agent_name.as_ref()))
            .unwrap_or_else(|| agent_id.clone());
        Some(ForkResponder {
            agent_id,
            agent_name,
        })
    }) {
        return responder;
    }

    if let Some(participant) = parent.thread_participants.iter().rev().find(|participant| {
        !participant.agent_id.trim().is_empty()
            && (participant.status.trim().is_empty()
                || participant.status.eq_ignore_ascii_case("active"))
    }) {
        return ForkResponder {
            agent_id: participant.agent_id.trim().to_string(),
            agent_name: nonempty_string(Some(&participant.agent_name))
                .unwrap_or_else(|| participant.agent_id.trim().to_string()),
        };
    }

    let agent_name =
        nonempty_string(parent.agent_name.as_ref()).unwrap_or_else(|| "Swarog".to_string());
    ForkResponder {
        agent_id: agent_name.clone(),
        agent_name,
    }
}

fn fork_thread_metadata_json(
    parent: &chat::AgentThread,
    forked_messages: &[chat::AgentMessage],
    fork_thread_id: &str,
    forked_message_index: usize,
    forked_message_id: &str,
    entered_at: u64,
    profile: &ForkThreadProfile,
) -> Option<String> {
    let responder = fork_responder(parent, forked_messages);
    let execution_profile = json!({
        "provider": profile.provider,
        "model": profile.model,
        "reasoning_effort": profile.reasoning_effort,
        "context_window_tokens": profile.context_window_tokens,
    });
    Some(
        json!({
            "thread_id": fork_thread_id,
            "threadId": fork_thread_id,
            "source": "tui_message_fork",
            "upstream_thread_id": parent.id,
            "upstreamThreadId": parent.id,
            "forked_from_thread_id": parent.id,
            "forkedFromThreadId": parent.id,
            "forked_message_index": forked_message_index,
            "forkedMessageIndex": forked_message_index,
            "forked_message_id": forked_message_id,
            "forkedMessageId": forked_message_id,
            "execution_profile": execution_profile,
            "thread_profile": execution_profile,
            "origin_agent_id": responder.agent_id,
            "active_agent_id": responder.agent_id,
            "handoff_stack": [
                {
                    "agent_id": responder.agent_id,
                    "agent_name": responder.agent_name,
                    "entered_at": entered_at,
                }
            ],
            "handoff_events": [],
        })
        .to_string(),
    )
}

fn nonempty_string(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn content_blocks_json(blocks: &[chat::AgentContentBlock]) -> Vec<serde_json::Value> {
    blocks
        .iter()
        .map(|block| match block {
            chat::AgentContentBlock::Text { text } => json!({
                "type": "text",
                "text": text,
            }),
            chat::AgentContentBlock::Image {
                url,
                data_url,
                mime_type,
            } => json!({
                "type": "image",
                "url": url,
                "data_url": data_url,
                "mime_type": mime_type,
            }),
            chat::AgentContentBlock::Audio {
                url,
                data_url,
                mime_type,
            } => json!({
                "type": "audio",
                "url": url,
                "data_url": data_url,
                "mime_type": mime_type,
            }),
            chat::AgentContentBlock::Video {
                url,
                data_url,
                mime_type,
            } => json!({
                "type": "video",
                "url": url,
                "data_url": data_url,
                "mime_type": mime_type,
            }),
        })
        .collect()
}

fn weles_review_json(review: &chat::WelesReviewMetaVm) -> serde_json::Value {
    json!({
        "weles_reviewed": review.weles_reviewed,
        "verdict": review.verdict,
        "reasons": review.reasons,
        "audit_id": review.audit_id,
        "security_override_mode": review.security_override_mode,
    })
}

fn fork_thread_title(parent: &chat::AgentThread, index: usize) -> String {
    let base = parent
        .messages
        .get(index)
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .unwrap_or(parent.title.as_str());
    let base = truncate_preview(base, 48);
    if base.is_empty() {
        "Forked thread".to_string()
    } else {
        format!("Fork: {base}")
    }
}

fn truncate_preview(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        format!("{}...", trimmed.chars().take(max_chars).collect::<String>())
    }
}

fn role_to_db(role: &chat::MessageRole) -> &'static str {
    match role {
        chat::MessageRole::System => "system",
        chat::MessageRole::User => "user",
        chat::MessageRole::Assistant => "assistant",
        chat::MessageRole::Tool => "tool",
        chat::MessageRole::Unknown => "unknown",
    }
}

fn fork_message_id(fork_thread_id: &str, index: usize) -> String {
    format!("{fork_thread_id}-msg-{index}")
}

fn nonzero_i64(value: u64) -> Option<i64> {
    (value > 0).then_some(value.min(i64::MAX as u64) as i64)
}

fn current_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
