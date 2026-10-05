#![allow(dead_code)]

use super::provider_resolution::apply_provider_model_override;
use super::*;

const PARTICIPANT_PROMPT_MAX_MESSAGES: usize = 48;
const PARTICIPANT_PROMPT_MAX_TOKENS: usize = 16_000;
const PARTICIPANT_DUPLICATE_MUTE_THRESHOLD: u32 = 3;
const PARTICIPANT_RECENT_CONTRIBUTION_WINDOW: usize = 3;
const PARTICIPANT_STATUS_ACK_MAX_WORDS: usize = 15;

const PARTICIPANT_STATUS_ACK_WORDS: &[&str] = &[
    "ack",
    "acked",
    "acknowledged",
    "all",
    "am",
    "and",
    "as",
    "at",
    "before",
    "being",
    "by",
    "change",
    "changed",
    "changes",
    "clean",
    "continuing",
    "currently",
    "end",
    "fine",
    "for",
    "from",
    "further",
    "good",
    "got",
    "have",
    "healthy",
    "here",
    "hold",
    "holding",
    "i",
    "idle",
    "in",
    "info",
    "information",
    "is",
    "it",
    "moment",
    "monitor",
    "monitoring",
    "my",
    "new",
    "news",
    "no",
    "nominal",
    "none",
    "noted",
    "nothing",
    "observing",
    "of",
    "ok",
    "okay",
    "on",
    "progress",
    "quiet",
    "remain",
    "remains",
    "report",
    "running",
    "same",
    "side",
    "since",
    "stand",
    "standing",
    "status",
    "steady",
    "still",
    "that",
    "the",
    "there",
    "this",
    "time",
    "to",
    "unchanged",
    "understood",
    "until",
    "update",
    "updated",
    "updates",
    "wait",
    "waiting",
    "watch",
    "watching",
    "will",
    "working",
    "yet",
];

fn normalize_participant_message_for_comparison(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn participant_message_is_status_ack(value: &str) -> bool {
    let normalized = normalize_participant_message_for_comparison(value);
    if normalized.is_empty() {
        return true;
    }
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    words.len() <= PARTICIPANT_STATUS_ACK_MAX_WORDS
        && words
            .iter()
            .all(|word| PARTICIPANT_STATUS_ACK_WORDS.contains(word))
}

pub(super) fn participant_message_repeats_recent_contribution(
    visible_messages: &[AgentMessage],
    agent_id: &str,
    candidate: &str,
) -> bool {
    let normalized_candidate = normalize_participant_message_for_comparison(candidate);
    if normalized_candidate.is_empty() {
        return true;
    }
    visible_messages
        .iter()
        .rev()
        .filter(|message| {
            message.role == MessageRole::Assistant
                && message
                    .author_agent_id
                    .as_deref()
                    .is_some_and(|author_id| author_id.eq_ignore_ascii_case(agent_id))
        })
        .take(PARTICIPANT_RECENT_CONTRIBUTION_WINDOW)
        .any(|message| {
            normalize_participant_message_for_comparison(&message.content) == normalized_candidate
        })
}

pub(super) fn thread_novelty_epoch(messages: &[AgentMessage]) -> u64 {
    messages
        .iter()
        .rev()
        .find(|message| match message.role {
            MessageRole::User | MessageRole::Tool => true,
            MessageRole::Assistant => message
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty()),
            MessageRole::System => false,
        })
        .map(|message| message.timestamp)
        .unwrap_or(0)
}

pub(super) fn latest_user_message_timestamp(messages: &[AgentMessage]) -> u64 {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.timestamp)
        .unwrap_or(0)
}

fn normalize_no_suggestion_candidate(value: &str) -> String {
    let mut current = value.trim();
    loop {
        let trimmed = current.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '*' | '`' | '_' | '-' | '•' | '>' | '#')
        });
        let lowercase = trimmed.to_ascii_lowercase();
        let Some(colon_index) = lowercase.find(':') else {
            current = trimmed;
            break;
        };
        let label = lowercase[..colon_index].trim();
        if matches!(label, "response" | "answer" | "result" | "final" | "status") {
            current = &trimmed[colon_index + 1..];
            continue;
        }
        current = trimmed;
        break;
    }

    current
        .trim_matches(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '*' | '`' | '_' | '"' | '\'' | '[' | ']' | '(' | ')' | '.'
                )
        })
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(super) fn participant_response_is_no_suggestion(value: &str) -> bool {
    matches!(
        normalize_no_suggestion_candidate(value).as_str(),
        "nosuggestion" | "no_suggestion" | "nomessage" | "no_message"
    )
}

pub(super) fn parse_participant_suggestion_response(response: &str) -> Option<(bool, String)> {
    let trimmed = response.trim();
    if trimmed.is_empty() || participant_response_is_no_suggestion(trimmed) {
        return None;
    }

    let mut force_send = false;
    let mut message_line: Option<String> = None;
    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("FORCE:") {
            force_send = matches!(rest.trim().to_ascii_lowercase().as_str(), "yes" | "true");
            continue;
        }
        if let Some(rest) = line.strip_prefix("MESSAGE:") {
            let message = rest.trim();
            if !message.is_empty() {
                message_line = Some(message.to_string());
            }
        }
    }

    if let Some(message) = message_line {
        if participant_response_is_no_suggestion(&message) {
            return None;
        }
        return Some((force_send, message));
    }

    if trimmed
        .lines()
        .any(|line| participant_response_is_no_suggestion(line))
    {
        return None;
    }

    Some((force_send, trimmed.to_string()))
}

fn should_hide_participant_prompt_message(message: &AgentMessage) -> bool {
    matches!(message.role, MessageRole::System | MessageRole::Tool)
        || message
            .tool_name
            .as_deref()
            .map(|name| name == "internal_delegate")
            .unwrap_or(false)
}

fn latest_visible_message_allows_participant_observers(
    visible_messages: &[AgentMessage],
    participants: &[ThreadParticipantState],
) -> bool {
    let Some(latest_message) = visible_messages.last() else {
        return false;
    };

    match latest_message.role {
        MessageRole::User => true,
        MessageRole::Assistant => latest_message
            .author_agent_id
            .as_ref()
            .is_none_or(|author_id| {
                !participants
                    .iter()
                    .any(|participant| participant.agent_id.eq_ignore_ascii_case(author_id))
            }),
        MessageRole::System | MessageRole::Tool => false,
    }
}

fn build_participant_prompt_from_snapshot(
    participant: &ThreadParticipantState,
    visible_messages: &[AgentMessage],
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Role: participant observer\n");
    prompt.push_str(&format!("Participant: {}\n", participant.agent_name));
    prompt.push_str(&format!("Instruction: {}\n\n", participant.instruction));
    prompt.push_str("Respond with either:\n");
    prompt.push_str("- NO_SUGGESTION\n");
    prompt.push_str("- FORCE: yes|no\n  MESSAGE: <text>\n\n");
    prompt.push_str(
        "Evaluate the thread after the latest visible message, even if that message is from the assistant.\n",
    );
    prompt.push_str(
        "If the operator asked for autonomous progress and work is still pending, you should suggest the next concrete participant action instead of NO_SUGGESTION.\n",
    );
    prompt.push_str(
        "If the latest assistant message proposes next steps, ongoing work, or an unfinished plan, continue that flow when your participant role can help.\n",
    );
    prompt.push_str(
        "Return NO_SUGGESTION only when the visible thread is naturally complete, blocked on external input, or the participant truly has nothing useful to add.\n",
    );
    prompt.push_str(
        "Never post status-only acknowledgements such as 'holding', 'still monitoring', or 'nothing new'. If you have no new information, decision, or concrete action to contribute, return NO_SUGGESTION.\n",
    );
    prompt.push_str(
        "Never repeat a message you already posted on this thread. Silence is always better than filler.\n\n",
    );
    prompt.push_str("Visible thread:\n");
    for message in visible_messages {
        let role = match message.role {
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
            MessageRole::User => "user",
        };
        if !message.content.trim().is_empty() {
            prompt.push_str(&format!("- {role}: {}\n", message.content.trim()));
        }
    }
    prompt
}

fn build_visible_participant_message_prompt(
    participant: &ThreadParticipantState,
    visible_messages: &[AgentMessage],
    request: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Role: visible thread participant\n");
    prompt.push_str(&format!("Participant: {}\n", participant.agent_name));
    prompt.push_str(&format!(
        "Participant registration instruction: {}\n\n",
        participant.instruction
    ));
    prompt.push_str("Generate the one visible thread message you want to post now.\n");
    prompt.push_str("Respond with only the exact message text to post.\n");
    prompt.push_str("Do not include analysis, XML, tool calls, or extra framing.\n\n");
    prompt.push_str("Current request:\n");
    prompt.push_str(request.trim());
    prompt.push_str("\n\nVisible thread:\n");
    for message in visible_messages {
        let role = match message.role {
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
            MessageRole::User => "user",
        };
        if !message.content.trim().is_empty() {
            prompt.push_str(&format!("- {role}: {}\n", message.content.trim()));
        }
    }
    prompt
}

fn trim_participant_prompt_snapshot(messages: &mut Vec<AgentMessage>) {
    let mut total_tokens = estimate_message_tokens(messages);
    while (messages.len() > PARTICIPANT_PROMPT_MAX_MESSAGES
        || total_tokens > PARTICIPANT_PROMPT_MAX_TOKENS)
        && messages.len() > 1
    {
        let remove_index = if messages
            .first()
            .is_some_and(crate::agent::message_is_compaction_summary)
            && messages.len() > 1
        {
            1
        } else {
            0
        };
        total_tokens =
            total_tokens.saturating_sub(estimate_single_message_tokens(&messages[remove_index]));
        messages.remove(remove_index);
    }

    if messages.len() == 1 && total_tokens > PARTICIPANT_PROMPT_MAX_TOKENS {
        let content = messages[0].content.clone();
        let truncated = content
            .chars()
            .take(2_000)
            .collect::<String>()
            .trim()
            .to_string();
        messages[0].content = if truncated.is_empty() {
            "[Participant prompt snapshot truncated to fit budget.]".to_string()
        } else {
            format!("{truncated}\n\n[Participant prompt snapshot truncated to fit budget.]")
        };
    }
}

fn trim_participant_playground_thread_messages(
    thread: &mut AgentThread,
    max_messages: usize,
    updated_at: u64,
) -> bool {
    if thread.messages.len() <= max_messages {
        return false;
    }

    let latest_artifact_index = thread
        .messages
        .iter()
        .rposition(super::compaction::message_is_compaction_summary);
    thread.messages = if let Some(artifact_index) = latest_artifact_index {
        let tail_start = thread.messages.len().saturating_sub(max_messages);
        if artifact_index < tail_start {
            let trailing_count = max_messages.saturating_sub(1);
            let recent_start = thread.messages.len().saturating_sub(trailing_count);
            let mut retained = Vec::with_capacity(max_messages);
            retained.push(thread.messages[artifact_index].clone());
            retained.extend(thread.messages[recent_start..].iter().cloned());
            retained
        } else {
            thread.messages[tail_start..].to_vec()
        }
    } else {
        let drop_count = thread.messages.len().saturating_sub(max_messages);
        thread.messages[drop_count..].to_vec()
    };
    thread.total_input_tokens = thread
        .messages
        .iter()
        .map(|message| message.input_tokens)
        .sum();
    thread.total_output_tokens = thread
        .messages
        .iter()
        .map(|message| message.output_tokens)
        .sum();
    thread.updated_at = updated_at;
    true
}

#[derive(Clone)]
struct ParticipantObserverResponderConfig {
    provider_id: String,
    provider_config: ProviderConfig,
    base_prompt: String,
}

impl AgentEngine {
    pub(crate) async fn latest_visible_main_agent_message_timestamp_for_auto_response(
        &self,
        thread_id: &str,
    ) -> Option<u64> {
        let participants = self.list_thread_participants(thread_id).await;
        let participant_agent_ids = participants
            .iter()
            .map(|participant| participant.agent_id.clone())
            .collect::<Vec<_>>();
        match self
            .history
            .latest_visible_main_assistant_message_timestamp(thread_id, &participant_agent_ids)
            .await
        {
            Ok(timestamp) => timestamp,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to load latest visible main assistant message timestamp from sqlite"
                );
                None
            }
        }
    }

    async fn participant_playground_message_limit(&self) -> usize {
        let config = self.config.read().await;
        config
            .max_context_messages
            .max(config.keep_recent_on_compact)
            .max(1) as usize
    }

    async fn trim_participant_playground_thread_by_id(&self, thread_id: &str) -> bool {
        if !crate::agent::agent_identity::is_participant_playground_thread(thread_id) {
            return false;
        }

        let max_messages = self.participant_playground_message_limit().await;
        let trimmed = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            trim_participant_playground_thread_messages(thread, max_messages, now_millis())
        };

        if trimmed {
            self.persist_thread_by_id(thread_id).await;
        }

        trimmed
    }

    pub(crate) async fn restore_participant_observer_state_after_hydrate(&self) {
        let thread_ids = {
            let participants = self.thread_participants.read().await;
            participants.keys().cloned().collect::<Vec<_>>()
        };

        for thread_id in thread_ids {
            if crate::agent::agent_identity::is_participant_playground_thread(&thread_id) {
                continue;
            }
            if !self
                .should_restore_participant_observers_after_hydrate(&thread_id)
                .await
            {
                tracing::debug!(
                    thread_id = %thread_id,
                    "skipping participant observer restore for stale lazy thread"
                );
                continue;
            }
            if let Err(error) = self
                .maybe_auto_send_next_thread_participant_suggestion(&thread_id)
                .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to restore queued participant suggestions during hydrate"
                );
            }
            if let Err(error) = self.run_participant_observers(&thread_id).await {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to restore participant observer state during hydrate"
                );
            }
        }
    }

    async fn should_restore_participant_observers_after_hydrate(&self, thread_id: &str) -> bool {
        let is_lazy = self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id);
        if !is_lazy {
            return true;
        }

        let restore_window_hours = self
            .config
            .read()
            .await
            .participant_observer_restore_window_hours;
        if restore_window_hours == 0 {
            return false;
        }

        let Some(updated_at) = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.updated_at)
        else {
            return false;
        };
        let window_ms = (restore_window_hours as u64).saturating_mul(60 * 60 * 1000);
        updated_at.saturating_add(window_ms) >= now_millis()
    }

    pub(crate) async fn trim_persisted_participant_playground_threads_on_hydrate(&self) -> usize {
        let max_messages = self.participant_playground_message_limit().await;
        self.history
            .trim_thread_messages_to_recent_tail_by_prefix(
                crate::agent::agent_identity::PARTICIPANT_PLAYGROUND_THREAD_PREFIX,
                max_messages,
            )
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(
                    %error,
                    "failed to trim participant playground threads from persisted history"
                );
                0
            })
    }

    pub(crate) async fn trim_participant_playground_threads_for_visible_thread(
        &self,
        visible_thread_id: &str,
    ) {
        let suffix = format!(":{visible_thread_id}");
        let playground_thread_ids = {
            let threads = self.threads.read().await;
            threads
                .keys()
                .filter(|thread_id| {
                    crate::agent::agent_identity::is_participant_playground_thread(thread_id)
                        && thread_id.ends_with(&suffix)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        if playground_thread_ids.is_empty() {
            return;
        }

        let max_messages = self.participant_playground_message_limit().await;
        let updated_at = now_millis();
        let mut trimmed_thread_ids = Vec::new();
        {
            let mut threads = self.threads.write().await;
            for thread_id in &playground_thread_ids {
                if let Some(thread) = threads.get_mut(thread_id) {
                    if trim_participant_playground_thread_messages(thread, max_messages, updated_at)
                    {
                        trimmed_thread_ids.push(thread_id.clone());
                    }
                }
            }
        }

        for thread_id in trimmed_thread_ids {
            self.persist_thread_by_id(&thread_id).await;
            let _ = self
                .event_tx
                .send(AgentEvent::ThreadReloadRequired { thread_id });
        }
    }

    async fn prepare_participant_playground_thread(
        &self,
        visible_thread_id: &str,
        target_agent_id: &str,
        wrapped_prompt: &str,
    ) -> String {
        let playground_thread_id =
            participant_playground_thread_id(visible_thread_id, target_agent_id);
        let _ = self
            .get_or_create_thread_with_target(
                Some(&playground_thread_id),
                wrapped_prompt,
                Some(target_agent_id),
            )
            .await;
        self.set_thread_handoff_state(
            &playground_thread_id,
            initial_thread_handoff_state(
                &playground_thread_id,
                Some(canonical_agent_name(target_agent_id)),
                now_millis(),
            ),
        )
        .await;
        self.ensure_thread_identity(
            &playground_thread_id,
            &participant_playground_thread_title(visible_thread_id, target_agent_id),
            false,
        )
        .await;
        playground_thread_id
    }

    async fn compact_participant_prompt_messages(
        &self,
        target_agent_id: &str,
        visible_messages: &[AgentMessage],
    ) -> Result<Vec<AgentMessage>> {
        let responder = self
            .participant_observer_responder_config(target_agent_id)
            .await?;
        let mut request_config = self.config.read().await.clone();
        request_config.provider = responder.provider_id;
        let mut compacted = compact_messages_for_request(
            visible_messages,
            &request_config,
            &responder.provider_config,
        );
        trim_participant_prompt_snapshot(&mut compacted);
        Ok(compacted)
    }

    async fn participant_observer_responder_config(
        &self,
        target_agent_id: &str,
    ) -> Result<ParticipantObserverResponderConfig> {
        let config = self.config.read().await.clone();
        let sub_agents = self.list_sub_agents().await;
        let resolved_target = resolve_agent_target(target_agent_id, &sub_agents);
        let agent_scope_id = resolved_target.scope_id.clone();

        if agent_scope_id == CONCIERGE_AGENT_ID {
            let provider_id = config
                .concierge
                .provider
                .as_deref()
                .unwrap_or(&config.provider)
                .to_string();
            let concierge_provider = crate::agent::concierge::resolve_concierge_provider(&config)?;
            let mut provider_config =
                crate::agent::concierge::fast_concierge_provider_config(&concierge_provider);
            provider_config.api_key = concierge_provider.api_key;
            provider_config.base_url = concierge_provider.base_url;
            provider_config.auth_source = concierge_provider.auth_source;
            provider_config.assistant_id = concierge_provider.assistant_id;
            provider_config.api_transport = concierge_provider.api_transport;

            return Ok(ParticipantObserverResponderConfig {
                provider_id,
                provider_config,
                base_prompt: crate::agent::concierge::concierge_system_prompt(),
            });
        }

        let matched_def = resolved_target.matched_sub_agent.clone();
        let builtin_persona_overrides = builtin_persona_overrides(&config, &agent_scope_id);
        if is_explicit_builtin_persona_scope(&agent_scope_id)
            && builtin_persona_requires_setup(&config, &agent_scope_id)
        {
            return Err(builtin_persona_setup_error(&agent_scope_id));
        }
        let persona_prompt = if agent_scope_id == WELES_AGENT_ID {
            build_weles_persona_prompt(WELES_GOVERNANCE_SCOPE)
        } else if agent_scope_id == MAIN_AGENT_ID {
            String::new()
        } else {
            build_spawned_persona_prompt(&agent_scope_id)
        };
        let provider_id = matched_def
            .as_ref()
            .map(|def| def.provider.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.provider.clone())
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| config.provider.clone());
        let mut provider_config = if matched_def.is_some() || agent_scope_id != MAIN_AGENT_ID {
            self.resolve_sub_agent_provider_config(&config, &provider_id)?
        } else {
            self.resolve_provider_config(&config)?
        };
        if let Some(model) = matched_def
            .as_ref()
            .map(|def| def.model.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.model.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        {
            apply_provider_model_override(&provider_id, &mut provider_config, &model);
        }
        if let Some(base_url) = matched_def
            .as_ref()
            .and_then(|def| def.base_url.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            provider_config.base_url = base_url.to_string();
        }
        if let Some(reasoning_effort) = matched_def
            .as_ref()
            .and_then(|def| def.reasoning_effort.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.reasoning_effort.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        {
            provider_config.reasoning_effort = reasoning_effort;
        }
        let system_prompt = matched_def
            .as_ref()
            .and_then(|def| def.system_prompt.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.system_prompt.clone());
        let base_prompt = if persona_prompt.is_empty() {
            system_prompt
        } else {
            format!("{persona_prompt}\n\n{system_prompt}")
        };

        Ok(ParticipantObserverResponderConfig {
            provider_id,
            provider_config,
            base_prompt,
        })
    }

    async fn run_hidden_participant_prompt(
        &self,
        visible_thread_id: &str,
        target_agent_id: &str,
        prompt: &str,
    ) -> Result<String> {
        let target_agent_id = agent_turn_scope_id(target_agent_id);
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            anyhow::bail!("participant playground prompt cannot be empty");
        }
        let playground_thread_id = self
            .prepare_participant_playground_thread(visible_thread_id, &target_agent_id, &prompt)
            .await;
        let playground_thread_id_for_run = playground_thread_id.clone();
        let output = Box::pin(run_with_agent_scope(target_agent_id.clone(), async move {
            let outcome = self
                .run_internal_send_loop(
                    Some(&playground_thread_id_for_run),
                    &prompt,
                    &[],
                    &prompt,
                    None,
                    None,
                    None,
                    None,
                    false,
                    false,
                )
                .await?;
            self.threads
                .read()
                .await
                .get(&outcome.thread_id)
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            message.role == MessageRole::Assistant
                                && message.tool_calls.as_ref().is_none_or(Vec::is_empty)
                        })
                        .and_then(|message| {
                            if !message.content.trim().is_empty() {
                                Some(message.content.trim().to_string())
                            } else {
                                message
                                    .reasoning
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|value| !value.is_empty())
                                    .map(str::to_string)
                            }
                        })
                })
                .ok_or_else(|| anyhow::anyhow!("participant playground returned empty output"))
        }))
        .await?;
        let _ = self
            .trim_participant_playground_thread_by_id(&playground_thread_id)
            .await;
        Ok(output)
    }

    pub async fn generate_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        request: &str,
    ) -> Result<String> {
        let participants = self.list_thread_participants(thread_id).await;
        let participant = participants
            .iter()
            .find(|participant| {
                participant.agent_id.eq_ignore_ascii_case(target_agent_id)
                    && participant.status == ThreadParticipantStatus::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!("participant is not active on thread: {target_agent_id}")
            })?;
        let thread = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        let visible_messages = thread
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        let compacted_messages = self
            .compact_participant_prompt_messages(target_agent_id, &visible_messages)
            .await?;
        let prompt =
            build_visible_participant_message_prompt(participant, &compacted_messages, request);
        self.run_hidden_participant_prompt(thread_id, target_agent_id, &prompt)
            .await
    }

    pub async fn build_participant_prompt(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<String> {
        let participants = self.list_thread_participants(thread_id).await;
        let participant = participants
            .iter()
            .find(|participant| {
                participant.agent_id.eq_ignore_ascii_case(target_agent_id)
                    && participant.status == ThreadParticipantStatus::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!("participant is not active on thread: {target_agent_id}")
            })?;
        let thread = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        let visible_messages = thread
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        let compacted_messages = self
            .compact_participant_prompt_messages(target_agent_id, &visible_messages)
            .await?;
        Ok(build_participant_prompt_from_snapshot(
            participant,
            &compacted_messages,
        ))
    }

    pub async fn append_internal_delegate_message(
        &self,
        thread_id: &str,
        content: &str,
    ) -> Result<()> {
        let mut threads = self.threads.write().await;
        let thread = threads
            .get_mut(thread_id)
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        thread.messages.push(AgentMessage {
            id: generate_message_id(),
            role: MessageRole::System,
            content: content.to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: Some("internal_delegate".to_string()),
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
            timestamp: now_millis(),
            feedback: None,
        });
        thread.updated_at = now_millis();
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
        Ok(())
    }

    pub async fn run_participant_observers(&self, thread_id: &str) -> Result<()> {
        let participants = self.list_thread_participants(thread_id).await;
        if participants.is_empty() {
            return Ok(());
        }
        let active_responder_agent_id = self.active_agent_id_for_thread(thread_id).await;
        let thread_messages = if self.threads.read().await.contains_key(thread_id) {
            self.messages_for_read_only_context(thread_id)
                .await
                .unwrap_or_default()
        } else {
            self.get_thread(thread_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?
                .messages
        };
        let novelty_epoch = thread_novelty_epoch(&thread_messages);
        let latest_user_timestamp = latest_user_message_timestamp(&thread_messages);
        let visible_messages = thread_messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        if !latest_visible_message_allows_participant_observers(&visible_messages, &participants) {
            tracing::info!(
                thread_id = %thread_id,
                "skipping participant observers because the latest visible message is participant-authored"
            );
            return Ok(());
        }
        let latest_visible_message_timestamp = visible_messages
            .last()
            .map(|message| message.timestamp)
            .unwrap_or(0);
        let queued_suggestions = self.list_thread_participant_suggestions(thread_id).await;
        let mut participant_state_changed = false;

        for participant in participants.into_iter().filter(|participant| {
            participant.status == ThreadParticipantStatus::Active
                && active_responder_agent_id.as_deref() != Some(participant.agent_id.as_str())
        }) {
            if participant
                .last_observed_visible_message_at
                .is_some_and(|timestamp| timestamp >= latest_visible_message_timestamp)
            {
                continue;
            }
            if queued_suggestions.iter().any(|suggestion| {
                suggestion
                    .target_agent_id
                    .eq_ignore_ascii_case(&participant.agent_id)
                    && suggestion.status == ThreadParticipantSuggestionStatus::Queued
            }) {
                continue;
            }
            if participant
                .last_contributed_epoch
                .is_some_and(|epoch| epoch >= novelty_epoch)
            {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %participant.agent_id,
                    novelty_epoch,
                    "skipping participant observer because no new thread activity since last contribution"
                );
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                continue;
            }
            if participant.duplicate_suppression_streak >= PARTICIPANT_DUPLICATE_MUTE_THRESHOLD {
                if participant
                    .last_suppressed_at
                    .is_some_and(|suppressed_at| suppressed_at >= latest_user_timestamp)
                {
                    tracing::info!(
                        thread_id = %thread_id,
                        participant = %participant.agent_id,
                        streak = participant.duplicate_suppression_streak,
                        "skipping muted participant observer until a new operator message arrives"
                    );
                    participant_state_changed |= self
                        .mark_thread_participant_observed_visible_message(
                            thread_id,
                            &participant.agent_id,
                            latest_visible_message_timestamp,
                        )
                        .await;
                    continue;
                }
                participant_state_changed |= self
                    .reset_thread_participant_duplicate_suppressions(
                        thread_id,
                        &participant.agent_id,
                    )
                    .await;
            }
            let compacted_messages = self
                .compact_participant_prompt_messages(&participant.agent_id, &visible_messages)
                .await?;
            tracing::info!(
                thread_id = %thread_id,
                participant = %participant.agent_id,
                visible_message_count = visible_messages.len(),
                compacted_message_count = compacted_messages.len(),
                visible_est_tokens = estimate_message_tokens(&visible_messages),
                compacted_est_tokens = estimate_message_tokens(&compacted_messages),
                "running participant observer"
            );
            let prompt = build_participant_prompt_from_snapshot(&participant, &compacted_messages);
            let response = self
                .run_hidden_participant_prompt(thread_id, &participant.agent_id, &prompt)
                .await?;
            let Some((force_send, message)) = parse_participant_suggestion_response(&response)
            else {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %participant.agent_id,
                    "participant observer returned no suggestion"
                );
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                participant_state_changed |= self
                    .mark_thread_participant_epoch_consumed(
                        thread_id,
                        &participant.agent_id,
                        novelty_epoch,
                    )
                    .await;
                continue;
            };
            if participant_message_is_status_ack(&message)
                || participant_message_repeats_recent_contribution(
                    &visible_messages,
                    &participant.agent_id,
                    &message,
                )
            {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %participant.agent_id,
                    "suppressing status-only or repeated participant message"
                );
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                participant_state_changed |= self
                    .mark_thread_participant_epoch_consumed(
                        thread_id,
                        &participant.agent_id,
                        novelty_epoch,
                    )
                    .await;
                participant_state_changed |= self
                    .record_thread_participant_duplicate_suppression(
                        thread_id,
                        &participant.agent_id,
                    )
                    .await;
                continue;
            }
            tracing::info!(
                thread_id = %thread_id,
                participant = %participant.agent_id,
                force_send,
                "participant observer produced suggestion"
            );
            participant_state_changed |= self
                .mark_thread_participant_epoch_consumed(
                    thread_id,
                    &participant.agent_id,
                    novelty_epoch,
                )
                .await;
            participant_state_changed |= self
                .reset_thread_participant_duplicate_suppressions(thread_id, &participant.agent_id)
                .await;
            if force_send {
                self.append_visible_thread_participant_message(
                    thread_id,
                    &participant.agent_id,
                    &message,
                )
                .await?;
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                self.continue_thread_after_participant_post_or_notice(thread_id)
                    .await;
            } else {
                self.queue_thread_participant_suggestion(
                    thread_id,
                    &participant.agent_id,
                    &message,
                    false,
                )
                .await?;
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
            }
        }

        if participant_state_changed {
            self.persist_thread_by_id(thread_id).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        latest_user_message_timestamp, parse_participant_suggestion_response,
        participant_message_is_status_ack, participant_message_repeats_recent_contribution,
        participant_response_is_no_suggestion, thread_novelty_epoch, AgentMessage, MessageRole,
        ToolCall, ToolFunction,
    };

    fn message(role: MessageRole, content: &str, timestamp: u64) -> AgentMessage {
        let mut message = AgentMessage::user(content, timestamp);
        message.role = role;
        message
    }

    fn participant_post(agent_id: &str, content: &str, timestamp: u64) -> AgentMessage {
        let mut message = message_assistant(content, timestamp);
        message.author_agent_id = Some(agent_id.to_string());
        message
    }

    fn message_assistant(content: &str, timestamp: u64) -> AgentMessage {
        message(MessageRole::Assistant, content, timestamp)
    }

    #[test]
    fn status_acks_are_detected_so_they_never_wake_the_responder() {
        assert!(participant_message_is_status_ack("Holding."));
        assert!(participant_message_is_status_ack("Still monitoring."));
        assert!(participant_message_is_status_ack("Nothing new to report."));
        assert!(participant_message_is_status_ack("Nothing on my end."));
        assert!(participant_message_is_status_ack(
            "Standing by, no updates."
        ));
        assert!(participant_message_is_status_ack(""));
    }

    #[test]
    fn substantive_messages_are_not_classified_as_status_acks() {
        assert!(!participant_message_is_status_ack(
            "Loss 5.82 at step 1125, run the eval harness when the checkpoint lands."
        ));
        assert!(!participant_message_is_status_ack(
            "The gate measurement failed; rerun phase 1 with a lower learning rate."
        ));
    }

    #[test]
    fn repeated_participant_posts_are_suppressed_regardless_of_formatting() {
        let visible = vec![
            message(MessageRole::User, "start the run", 1),
            participant_post(
                "kimus",
                "Escalate the checkpoint failure to the operator.",
                2,
            ),
        ];
        assert!(participant_message_repeats_recent_contribution(
            &visible,
            "kimus",
            "**Escalate the checkpoint failure to the operator!**"
        ));
        assert!(!participant_message_repeats_recent_contribution(
            &visible,
            "kimus",
            "Checkpoint 5000 landed; run the eval harness now."
        ));
        assert!(!participant_message_repeats_recent_contribution(
            &visible,
            "other-agent",
            "Escalate the checkpoint failure to the operator."
        ));
    }

    #[test]
    fn responder_chatter_without_tools_does_not_create_a_new_novelty_epoch() {
        let mut with_tools = message_assistant("checking status", 5);
        with_tools.tool_calls = Some(vec![ToolCall {
            id: "call-1".to_string(),
            function: ToolFunction {
                name: "bash_command".to_string(),
                arguments: "{}".to_string(),
            },
            weles_review: None,
        }]);
        let messages = vec![
            message(MessageRole::User, "start the run", 1),
            with_tools,
            message(MessageRole::Tool, "step 1125 loss 5.82", 6),
            message_assistant("Still monitoring.", 9),
            message_assistant("Holding.", 10),
        ];
        assert_eq!(thread_novelty_epoch(&messages), 6);
        assert_eq!(latest_user_message_timestamp(&messages), 1);
    }

    #[test]
    fn no_message_is_treated_like_no_suggestion() {
        assert!(participant_response_is_no_suggestion("NO_MESSAGE"));
        assert!(participant_response_is_no_suggestion("NO MESSAGE"));
        assert!(participant_response_is_no_suggestion(
            "Response: NO_MESSAGE"
        ));
        assert_eq!(parse_participant_suggestion_response("NO_MESSAGE"), None);
        assert_eq!(
            parse_participant_suggestion_response("FORCE: yes\nMESSAGE: NO_MESSAGE"),
            None
        );
    }
}
