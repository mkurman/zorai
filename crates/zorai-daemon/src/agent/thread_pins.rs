use super::*;
use serde::{Deserialize, Serialize};

const PINNED_CONTEXT_BUDGET_DENOMINATOR: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThreadMessagePinMutationResult {
    pub ok: bool,
    pub thread_id: String,
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub current_pinned_chars: usize,
    pub pinned_budget_chars: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_pinned_chars: Option<usize>,
}

impl ThreadMessagePinMutationResult {
    pub(crate) fn success(
        thread_id: &str,
        message_id: &str,
        current_pinned_chars: usize,
        pinned_budget_chars: usize,
    ) -> Self {
        Self {
            ok: true,
            thread_id: thread_id.to_string(),
            message_id: message_id.to_string(),
            error: None,
            current_pinned_chars,
            pinned_budget_chars,
            candidate_pinned_chars: None,
        }
    }

    pub(crate) fn failure(
        thread_id: &str,
        message_id: &str,
        error: impl Into<String>,
        current_pinned_chars: usize,
        pinned_budget_chars: usize,
        candidate_pinned_chars: Option<usize>,
    ) -> Self {
        Self {
            ok: false,
            thread_id: thread_id.to_string(),
            message_id: message_id.to_string(),
            error: Some(error.into()),
            current_pinned_chars,
            pinned_budget_chars,
            candidate_pinned_chars,
        }
    }
}

pub(crate) fn pinned_for_compaction_message_chars(message: &AgentMessage) -> usize {
    message.content.chars().count()
}

pub(crate) fn pinned_for_compaction_budget_chars(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    let context_window_tokens = if provider_config.context_window_tokens > 0 {
        provider_config.context_window_tokens
    } else {
        config.context_window_tokens
    };
    context_window_tokens
        .max(1)
        .saturating_mul(APPROX_CHARS_PER_TOKEN as u32)
        .saturating_div(PINNED_CONTEXT_BUDGET_DENOMINATOR as u32) as usize
}

pub(crate) fn owner_only_pins(thread: &AgentThread) -> Vec<AgentMessage> {
    if current_agent_scope_id() != MAIN_AGENT_ID {
        return Vec::new();
    }

    thread
        .messages
        .iter()
        .filter(|message| message.pinned_for_compaction && !message_is_compaction_summary(message))
        .cloned()
        .collect()
}

pub(crate) fn owner_only_pins_within_budget(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    let budget_chars = pinned_for_compaction_budget_chars(config, provider_config);
    let mut used_chars = 0usize;
    let mut included = Vec::new();

    for message in owner_only_pins(thread) {
        let message_chars = pinned_for_compaction_message_chars(&message);
        if used_chars.saturating_add(message_chars) > budget_chars {
            break;
        }
        used_chars = used_chars.saturating_add(message_chars);
        included.push(message);
    }

    included
}
