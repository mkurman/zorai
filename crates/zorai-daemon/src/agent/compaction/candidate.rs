use super::*;

pub(crate) fn compact_messages_for_request(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    let runtime_messages = active_request_messages(messages);
    let Some(candidate) = compaction_candidate(&runtime_messages, config, provider_config) else {
        let model_window = model_context_window(
            &config.provider,
            &provider_config.model,
            provider_config
                .context_window_tokens
                .max(config.context_window_tokens),
        ) as usize;
        let current = predicted_request_tokens(&runtime_messages);
        if current > model_window {
            return hard_truncate_to_fit(&runtime_messages, model_window);
        }
        return runtime_messages;
    };
    let max_messages = config.max_context_messages.max(1) as usize;
    let split_at = candidate.split_at;
    let mut compacted = Vec::new();
    let mut has_summary = false;

    if split_at > 0 {
        let summary =
            build_compaction_summary(&runtime_messages[..split_at], candidate.target_tokens);
        if !summary.is_empty() {
            has_summary = true;
            let summary_payload = summary.clone();
            compacted.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: summary,
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
                message_kind: AgentMessageKind::CompactionArtifact,
                compaction_strategy: Some(CompactionStrategy::Heuristic),
                compaction_payload: Some(summary_payload),
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: messages[split_at - 1].timestamp,
                feedback: None,
            });
        }
    }

    compacted.extend(runtime_messages[split_at..].iter().cloned());
    trim_compacted_messages(
        &mut compacted,
        max_messages,
        candidate.target_tokens,
        has_summary,
    );
    compacted
}

pub(crate) fn compaction_candidate(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Option<CompactionCandidate> {
    compaction_candidate_with_mode(
        messages,
        config,
        provider_config,
        CompactionCandidateMode::Automatic,
    )
}

pub(crate) fn forced_compaction_candidate(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Option<CompactionCandidate> {
    compaction_candidate_with_mode(
        messages,
        config,
        provider_config,
        CompactionCandidateMode::Forced,
    )
}

pub(crate) fn compaction_candidate_with_mode(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    mode: CompactionCandidateMode,
) -> Option<CompactionCandidate> {
    let (_, active_messages) = active_compaction_window(messages);
    if active_messages.is_empty()
        || (mode == CompactionCandidateMode::Automatic && !config.auto_compact_context)
    {
        return None;
    }
    if mode == CompactionCandidateMode::Automatic
        && crate::agent::types::provider_uses_local_subscription_cli(&config.provider)
    {
        return None;
    }

    let max_messages = config.max_context_messages.max(1) as usize;
    let target_tokens = effective_context_target_tokens(config, provider_config);
    let trigger = match mode {
        CompactionCandidateMode::Forced => CompactionTrigger::ManualRequest,
        CompactionCandidateMode::Automatic => {
            let message_count_gate_enabled =
                matches!(config.compaction.strategy, CompactionStrategy::Heuristic);
            let over_message_limit =
                message_count_gate_enabled && active_messages.len() > max_messages;
            let over_token_limit = predicted_request_tokens(active_messages) > target_tokens;
            match (over_message_limit, over_token_limit) {
                (false, false) => return None,
                (true, false) => CompactionTrigger::MessageCount,
                (false, true) => CompactionTrigger::TokenThreshold,
                (true, true) => CompactionTrigger::MessageCountAndTokenThreshold,
            }
        }
    };

    let keep_recent = config
        .keep_recent_on_compact
        .max(1)
        .min(active_messages.len() as u32) as usize;
    let mut split_at = active_messages.len().saturating_sub(keep_recent);
    if split_at == 0 {
        return None;
    }

    while split_at > 0 && active_messages[split_at].role == MessageRole::Tool {
        split_at -= 1;
    }

    while split_at > 0 {
        let Some(dangling_start) = trailing_dangling_tool_turn_start(&active_messages[..split_at])
        else {
            break;
        };
        split_at = dangling_start;
    }

    if split_at == 0 {
        return None;
    }

    Some(CompactionCandidate {
        split_at,
        target_tokens,
        trigger,
    })
}

pub(crate) fn trim_compacted_messages(
    messages: &mut Vec<AgentMessage>,
    max_messages: usize,
    target_tokens: usize,
    has_summary: bool,
) {
    let removable_floor = if has_summary { 2 } else { 1 };
    let mut total_tokens = estimate_message_tokens(messages) + measured_context_overhead(messages);
    while (messages.len() > max_messages || total_tokens > target_tokens)
        && messages.len() > removable_floor
    {
        let remove_index = if has_summary { 1 } else { 0 };

        if remove_index < messages.len()
            && messages[remove_index].role == MessageRole::Assistant
            && messages[remove_index].tool_calls.is_some()
        {
            let mut end = remove_index + 1;
            while end < messages.len() && messages[end].role == MessageRole::Tool {
                end += 1;
            }
            for i in (remove_index..end).rev() {
                total_tokens -= estimate_single_message_tokens(&messages[i]);
                messages.remove(i);
            }
        } else {
            total_tokens -= estimate_single_message_tokens(&messages[remove_index]);
            messages.remove(remove_index);
        }
    }

    if has_summary
        && messages.len() > 1
        && (messages.len() > max_messages || total_tokens > target_tokens)
    {
        messages.remove(0);
    }
}

pub(crate) fn effective_context_target_tokens(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    let primary_context_window = primary_context_window_tokens(config, provider_config);
    let threshold_pct = config.compact_threshold_pct.clamp(1, 100) as usize;
    let primary_target = primary_context_window.saturating_mul(threshold_pct) / 100;
    let strategy_target_cap =
        strategy_target_cap_tokens(config, primary_context_window as u32, threshold_pct as u32)
            .unwrap_or(primary_target);

    primary_target
        .min(strategy_target_cap)
        .max(MIN_CONTEXT_TARGET_TOKENS)
}

pub(crate) fn primary_context_window_tokens(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    model_context_window(
        &config.provider,
        &provider_config.model,
        provider_config
            .context_window_tokens
            .max(config.context_window_tokens),
    ) as usize
}

pub(crate) fn effective_compaction_window_tokens(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    let primary_context_window = primary_context_window_tokens(config, provider_config);
    match config.compaction.strategy {
        CompactionStrategy::Heuristic => primary_context_window,
        CompactionStrategy::Weles => {
            let (provider_id, model_id) = resolved_weles_compaction_model(config);
            model_context_window(&provider_id, &model_id, primary_context_window as u32) as usize
        }
        CompactionStrategy::CustomModel => {
            config.compaction.custom_model.context_window_tokens.max(1) as usize
        }
    }
    .min(primary_context_window)
    .max(1)
}

pub(crate) fn format_token_count(value: usize) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

pub(crate) fn compaction_visible_strategy_label(strategy: CompactionStrategy) -> &'static str {
    match strategy {
        CompactionStrategy::Heuristic => HEURISTIC_COMPACTION_VISIBLE_TEXT,
        CompactionStrategy::Weles => "model generated",
        CompactionStrategy::CustomModel => "custom model generated",
    }
}

pub(crate) fn compaction_visible_trigger_label(trigger: CompactionTrigger) -> &'static str {
    match trigger {
        CompactionTrigger::MessageCount => "message-count",
        CompactionTrigger::TokenThreshold => "token-threshold",
        CompactionTrigger::MessageCountAndTokenThreshold => "message-count + token-threshold",
        CompactionTrigger::ManualRequest => "manual-request",
    }
}

pub(crate) fn compaction_trigger_detail_value(trigger: CompactionTrigger) -> &'static str {
    match trigger {
        CompactionTrigger::MessageCount => "message_count",
        CompactionTrigger::TokenThreshold => "token_threshold",
        CompactionTrigger::MessageCountAndTokenThreshold => "message_count_and_token_threshold",
        CompactionTrigger::ManualRequest => "manual_request",
    }
}

pub(crate) fn build_compaction_visible_content(
    pre_compaction_total_tokens: usize,
    effective_context_window_tokens: usize,
    target_tokens: usize,
    trigger: CompactionTrigger,
    strategy_used: CompactionStrategy,
) -> String {
    format!(
        "Pre-compaction context: ~{} / {} tokens (threshold {})\nTrigger: {}\nStrategy: {}",
        format_token_count(pre_compaction_total_tokens),
        format_token_count(effective_context_window_tokens),
        format_token_count(target_tokens),
        compaction_visible_trigger_label(trigger),
        compaction_visible_strategy_label(strategy_used),
    )
}

pub(crate) fn strategy_target_cap_tokens(
    config: &AgentConfig,
    primary_context_window: u32,
    threshold_pct: u32,
) -> Option<usize> {
    let threshold_pct = threshold_pct.clamp(1, 100) as usize;
    match config.compaction.strategy {
        CompactionStrategy::Heuristic => None,
        CompactionStrategy::Weles => {
            let (provider_id, model_id) = resolved_weles_compaction_model(config);
            Some(
                model_context_window(&provider_id, &model_id, primary_context_window) as usize
                    * threshold_pct
                    / 100,
            )
        }
        CompactionStrategy::CustomModel => Some(
            config.compaction.custom_model.context_window_tokens.max(1) as usize * threshold_pct
                / 100,
        ),
    }
}

pub(crate) fn resolved_weles_compaction_model(config: &AgentConfig) -> (String, String) {
    let provider_id = config.compaction.weles.provider.trim().to_string();
    let provider_id = if provider_id.is_empty() {
        config
            .builtin_sub_agents
            .weles
            .provider
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.provider.clone())
    } else {
        provider_id
    };

    let model_id = config.compaction.weles.model.trim().to_string();
    let model_id = if model_id.is_empty() {
        config
            .builtin_sub_agents
            .weles
            .model
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.model.clone())
    } else {
        model_id
    };

    (provider_id, model_id)
}

pub(crate) fn estimate_message_tokens(messages: &[AgentMessage]) -> usize {
    messages.iter().map(estimate_single_message_tokens).sum()
}

pub(crate) fn measured_context_overhead(messages: &[AgentMessage]) -> usize {
    let Some(anchor) = messages
        .iter()
        .rposition(|message| message.input_tokens > 0)
    else {
        return 0;
    };
    let measured = messages[anchor].input_tokens as usize;
    let estimated_prefix = estimate_message_tokens(&messages[..anchor]);
    measured.saturating_sub(estimated_prefix)
}

pub(crate) fn predicted_request_tokens(messages: &[AgentMessage]) -> usize {
    estimate_message_tokens(messages).saturating_add(measured_context_overhead(messages))
}

pub(crate) fn estimate_single_message_tokens(message: &AgentMessage) -> usize {
    let mut chars = compaction_runtime_content(message).chars().count();

    if let Some(tool_calls) = &message.tool_calls {
        chars += tool_calls
            .iter()
            .map(|call| {
                call.function.name.chars().count() + call.function.arguments.chars().count()
            })
            .sum::<usize>();
    }

    chars += message
        .tool_name
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);
    chars += message
        .tool_arguments
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);

    chars.div_ceil(APPROX_CHARS_PER_TOKEN) + 12
}

pub(crate) fn hard_truncate_to_fit(
    messages: &[AgentMessage],
    max_tokens: usize,
) -> Vec<AgentMessage> {
    let mut kept: Vec<AgentMessage> = Vec::new();
    let mut total = measured_context_overhead(messages);
    for msg in messages.iter().rev() {
        let msg_tokens = estimate_single_message_tokens(msg);
        if total + msg_tokens > max_tokens && !kept.is_empty() {
            break;
        }
        total += msg_tokens;
        kept.push(msg.clone());
    }
    kept.reverse();

    while !kept.is_empty() && kept[0].role == MessageRole::Tool {
        kept.remove(0);
    }

    tracing::warn!(
        "hard_truncate_to_fit: trimmed {} -> {} messages ({} -> {} est tokens)",
        messages.len(),
        kept.len(),
        estimate_message_tokens(messages),
        estimate_message_tokens(&kept),
    );

    kept
}

pub(crate) fn model_context_window(provider_id: &str, model_id: &str, config_fallback: u32) -> u32 {
    crate::agent::types::get_provider_definition(provider_id)
        .and_then(|def| def.models.iter().find(|m| m.id == model_id))
        .map(|m| m.context_window)
        .unwrap_or(config_fallback)
        .max(1)
}
