use super::*;

impl ConciergeEngine {
    pub(super) async fn compose_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> (String, Vec<ConciergeAction>) {
        let started = std::time::Instant::now();
        let actions = self.build_welcome_actions(detail_level, context);
        let signature = build_welcome_signature(detail_level, context);
        if let Some(cached) = self.cached_welcome(&signature).await {
            tracing::info!(
                detail_level = ?detail_level,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "concierge: compose_welcome served from cache"
            );
            return cached;
        }

        let content = if detail_level == ConciergeDetailLevel::Minimal {
            minimal_welcome_content(context)
        } else {
            let llm_started = std::time::Instant::now();
            tracing::info!(
                detail_level = ?detail_level,
                "concierge: starting LLM welcome call"
            );
            match self.call_llm_for_welcome(detail_level, context, true).await {
                Ok(response) => {
                    tracing::info!(
                        detail_level = ?detail_level,
                        elapsed_ms = llm_started.elapsed().as_millis() as u64,
                        response_len = response.len(),
                        "concierge: LLM welcome call complete"
                    );
                    strip_trailing_actions(&response)
                }
                Err(e) => {
                    tracing::warn!(
                        elapsed_ms = llm_started.elapsed().as_millis() as u64,
                        "concierge LLM call failed, falling back to template: {e}"
                    );
                    self.template_fallback(context)
                }
            }
        };

        self.cache_welcome(&signature, &content, &actions).await;
        tracing::info!(
            detail_level = ?detail_level,
            elapsed_ms = started.elapsed().as_millis() as u64,
            content_len = content.len(),
            "concierge: compose_welcome done"
        );
        (content, actions)
    }

    pub(super) fn build_welcome_actions(
        &self,
        _detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> Vec<ConciergeAction> {
        let mut actions = Vec::new();
        if let Some(last) = context.recent_threads.first() {
            actions.push(ConciergeAction {
                label: format!("Continue: {}", truncate_str(&last.title, 40)),
                action_type: ConciergeActionType::ContinueSession,
                thread_id: Some(last.id.clone()),
            });
        }
        actions.push(ConciergeAction {
            label: "Start new session".into(),
            action_type: ConciergeActionType::StartNew,
            thread_id: None,
        });
        actions.push(ConciergeAction {
            label: "Search history".into(),
            action_type: ConciergeActionType::Search,
            thread_id: None,
        });
        actions.push(ConciergeAction {
            label: "Dismiss".into(),
            action_type: ConciergeActionType::Dismiss,
            thread_id: None,
        });
        actions
    }

    async fn call_llm_for_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
        emit_partial_progress: bool,
    ) -> Result<String> {
        let config = self.config.read().await;
        let provider_config = fast_concierge_provider_config(&resolve_concierge_provider(&config)?);
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let base_prompt = concierge_system_prompt(&config.concierge);
        drop(config);

        let system_prompt = format!(
            "{}\n\n{}",
            base_prompt,
            super::super::build_concierge_runtime_identity_prompt(
                &provider_id,
                &provider_config.model,
            )
        );
        let user_prompt = self.build_llm_prompt(detail_level, context);
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(user_prompt),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        self.check_circuit_breaker(&provider_id).await?;

        let stream = llm_client::send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            &system_prompt,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );

        let mut full_content = String::new();
        let mut last_emitted_partial = String::new();
        let partial_emit_interval = std::time::Duration::from_millis(250);
        let partial_emit_min_growth: usize = 256;
        let mut last_emit_at: Option<std::time::Instant> = None;
        let mut last_emitted_len: usize = 0;
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => {
                    full_content.push_str(&content);
                    if emit_partial_progress {
                        let now = std::time::Instant::now();
                        let is_first_emit = last_emit_at.is_none();
                        let interval_due = last_emit_at
                            .map(|prev| now.duration_since(prev) >= partial_emit_interval)
                            .unwrap_or(true);
                        let grew_enough = full_content.len().saturating_sub(last_emitted_len)
                            >= partial_emit_min_growth;
                        if interval_due
                            && (is_first_emit || grew_enough)
                            && !full_content.trim().is_empty()
                        {
                            self.emit_partial_welcome_progress(
                                detail_level,
                                &full_content,
                                &mut last_emitted_partial,
                            );
                            last_emit_at = Some(now);
                            last_emitted_len = full_content.len();
                        }
                    }
                }
                Ok(CompletionChunk::Done { content, .. }) => {
                    self.record_llm_outcome(&provider_id, true).await;
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("LLM error: {message}");
                }
                Ok(_) => {}
                Err(e) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("Stream error: {e}");
                }
            }
        }

        if full_content.trim().is_empty() {
            anyhow::bail!("Empty LLM response");
        }

        Ok(full_content)
    }

    fn emit_partial_welcome_progress(
        &self,
        detail_level: ConciergeDetailLevel,
        full_content: &str,
        last_emitted_partial: &mut String,
    ) {
        let partial_content = full_content.trim_end();
        if partial_content.trim().is_empty() || partial_content == last_emitted_partial {
            return;
        }

        let _ = self.event_tx.send(AgentEvent::ConciergeWelcome {
            thread_id: CONCIERGE_THREAD_ID.to_string(),
            content: partial_content.to_string(),
            detail_level,
            actions: Vec::new(),
        });
        last_emitted_partial.clear();
        last_emitted_partial.push_str(partial_content);
    }

    fn build_llm_prompt(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "Generate a concise welcome greeting for the user who just opened zorai.\n\
             IMPORTANT: Do NOT include action buttons, next steps, or numbered suggestions \
             at the end — the UI renders interactive buttons separately. \
             Just provide the status summary and context.\n\n",
        );

        if let Some(last) = context.recent_threads.first() {
            prompt.push_str(&format!(
                "Last session: \"{}\" ({}, {} messages)\n",
                last.title,
                format_timestamp(last.updated_at),
                last.message_count
            ));
            if let Some(opening_message) = &last.opening_message {
                prompt.push_str(&format!(
                    "Conversation opened with:\n  {}\n",
                    opening_message
                ));
            }
            if !last.last_messages.is_empty() {
                prompt.push_str("Recent conversation:\n");
                for msg in &last.last_messages {
                    prompt.push_str(&format!("  {}\n", msg));
                }
            }
        } else {
            prompt.push_str("This is the user's first session — no history yet.\n");
        }

        if context.recent_threads.len() > 1 {
            prompt.push_str("\nOther recent sessions:\n");
            for t in &context.recent_threads[1..] {
                prompt.push_str(&format!(
                    "  - \"{}\" ({}, {} msgs)\n",
                    t.title,
                    format_timestamp(t.updated_at),
                    t.message_count
                ));
            }
        }

        if let Some(task) = context.latest_goal_run.as_ref() {
            prompt.push_str(&format!(
                "\nLatest goal: \"{}\" [{}] ({})\n",
                task.label,
                goal_run_status_label(task.status),
                format_timestamp(task.updated_at)
            ));
            if let Some(goal_prompt) = task.prompt.as_deref() {
                prompt.push_str(&format!("Goal prompt: {}\n", goal_prompt));
            }
            if let Some(summary) = task.summary.as_deref() {
                prompt.push_str(&format!("Goal summary: {}\n", summary));
            }
            if let Some(latest_step_result) = task.latest_step_result.as_deref() {
                prompt.push_str(&format!("Latest step result: {}\n", latest_step_result));
            }
        }

        if context.running_goal_total > 0 || context.paused_goal_total > 0 {
            prompt.push_str(&format!(
                "\nGoal counts: {} running, {} paused\n",
                context.running_goal_total, context.paused_goal_total
            ));
        }

        match detail_level {
            ConciergeDetailLevel::ContextSummary => {
                prompt.push_str(
                    "\nSummarize what the user was working on in 1-2 sentences. Mention the latest goal and active goal counts if helpful. Then ask what they'd like to do.",
                );
            }
            ConciergeDetailLevel::ProactiveTriage => {
                prompt.push_str("\nProvide a smart triage: summarize the last session, mention the latest goal and any active goal runs, and suggest 2-3 prioritized next steps.");
            }
            ConciergeDetailLevel::DailyBriefing => {
                prompt.push_str("\nProvide a full operational briefing: session summary, latest goal status, active goal counts, and actionable recommendations. Be comprehensive but concise.");
            }
            ConciergeDetailLevel::Minimal => unreachable!(),
        }

        prompt
    }

    fn template_fallback(&self, context: &WelcomeContext) -> String {
        if let Some(last) = context.recent_threads.first() {
            let mut parts = vec![format!(
                "**Last session:** {} ({}, {} messages)",
                last.title,
                format_timestamp(last.updated_at),
                last.message_count
            )];
            if let Some(task) = context.latest_goal_run.as_ref() {
                parts.push(format!(
                    "**Latest goal:** {} [{}] ({})",
                    task.label,
                    goal_run_status_label(task.status),
                    format_timestamp(task.updated_at)
                ));
                if let Some(summary) = task.summary.as_deref() {
                    parts.push(format!("**Goal summary:** {}", summary));
                }
                if let Some(goal_prompt) = task.prompt.as_deref() {
                    parts.push(format!("**Goal prompt:** {}", goal_prompt));
                }
                if let Some(latest_step_result) = task.latest_step_result.as_deref() {
                    parts.push(format!("**Latest step result:** {}", latest_step_result));
                }
            }
            if context.running_goal_total > 0 || context.paused_goal_total > 0 {
                parts.push(format!(
                    "**Goals:** {} running, {} paused",
                    context.running_goal_total, context.paused_goal_total
                ));
            }
            parts.push("What would you like to work on?".into());
            parts.join("\n")
        } else if let Some(task) = context.latest_goal_run.as_ref() {
            format!(
                "Welcome back! Latest goal: **{}** [{}] ({}).{}{}{}",
                task.label,
                goal_run_status_label(task.status),
                format_timestamp(task.updated_at),
                task.summary
                    .as_deref()
                    .map(|summary| format!(" Summary: {}", summary))
                    .unwrap_or_default(),
                task.prompt
                    .as_deref()
                    .map(|goal_prompt| format!(" Goal prompt: {}", goal_prompt))
                    .unwrap_or_default(),
                task.latest_step_result
                    .as_deref()
                    .map(|result| format!(" Latest step result: {}", result))
                    .unwrap_or_default()
            )
        } else {
            "Welcome to zorai! Ready to start your first session.".into()
        }
    }
}

pub(crate) fn fast_concierge_provider_config(config: &ProviderConfig) -> ProviderConfig {
    let mut fast = config.clone();
    if fast.reasoning_effort.trim().is_empty() {
        fast.reasoning_effort = "off".to_string();
    }
    fast
}

pub(crate) fn concierge_system_prompt(concierge: &ConciergeConfig) -> String {
    if let Some(custom) = concierge
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        return custom.to_string();
    }
    format!(
        "You are the zorai concierge — a lightweight operational assistant named {}. \
         You handle greetings, session navigation, status checks, housekeeping, \
         and quick lookups. For coding tasks, deep analysis, or complex work, \
         coordinate with {}, the main agent, when needed instead of pretending you did the deeper work yourself.\n\n\
         Be concise. One paragraph max for greetings. Use bullet points for \
         status summaries. Always offer 2-3 actionable next steps.",
        CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME,
    )
}

pub(crate) fn resolve_concierge_provider(config: &AgentConfig) -> Result<ProviderConfig> {
    let provider_id = config
        .concierge
        .provider
        .as_deref()
        .unwrap_or(&config.provider);
    let mut resolved =
        resolve_provider_config_for(config, provider_id, config.concierge.model.as_deref())?;
    crate::agent::provider_resolution::apply_role_transport_override(
        provider_id,
        &mut resolved,
        config.concierge.api_transport,
    );
    if let Some(base_url) = config
        .concierge
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        resolved.base_url = base_url.to_string();
    }
    resolved.reasoning_effort = config
        .concierge
        .reasoning_effort
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "off".to_string());
    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        if !config.concierge.openrouter_provider_order.is_empty() {
            resolved.openrouter_provider_order = config.concierge.openrouter_provider_order.clone();
        }
        if !config.concierge.openrouter_provider_ignore.is_empty() {
            resolved.openrouter_provider_ignore =
                config.concierge.openrouter_provider_ignore.clone();
        }
        if config.concierge.openrouter_allow_fallbacks.is_some() {
            resolved.openrouter_allow_fallbacks = config.concierge.openrouter_allow_fallbacks;
        }
    }
    if provider_id == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
        if let Some(huggingface_provider) = config
            .concierge
            .huggingface_provider
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            resolved.huggingface_provider = Some(huggingface_provider.trim().to_string());
        }
    }
    Ok(resolved)
}

fn strip_trailing_actions(content: &str) -> String {
    let patterns = [
        "\nNext Steps",
        "\n**Next Steps",
        "\nRecommended Next Steps",
        "\n**Recommended Next Steps",
        "\nRecommended Actions",
        "\n**Recommended Actions",
        "\nSuggested Actions",
        "\n**Suggested Actions",
        "\nAction Items",
        "\n**Action Items",
    ];
    let mut result = content.to_string();
    for pat in &patterns {
        if let Some(pos) = result.find(pat) {
            result.truncate(pos);
        }
    }
    result.trim_end().to_string()
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

pub(super) fn format_timestamp(ts: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH + Duration::from_millis(ts);
    let now = std::time::SystemTime::now();
    let elapsed = now.duration_since(dt).unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub(super) fn build_welcome_signature(
    detail_level: ConciergeDetailLevel,
    context: &WelcomeContext,
) -> String {
    let thread_sig = context
        .recent_threads
        .iter()
        .map(|thread| {
            format!(
                "{}|{}|{}|{}|{}",
                thread.id,
                thread.title,
                thread.updated_at,
                thread.message_count,
                thread.opening_message.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let task_sig = format!(
        "{}|{}|{}",
        context.running_goal_total,
        context.paused_goal_total,
        context
            .latest_goal_run
            .as_ref()
            .map(|task| {
                format!(
                    "{}|{}|{:?}|{}|{}|{}",
                    task.label,
                    task.prompt.as_deref().unwrap_or(""),
                    task.status,
                    task.updated_at,
                    task.summary.as_deref().unwrap_or(""),
                    task.latest_step_result.as_deref().unwrap_or("")
                )
            })
            .unwrap_or_default()
    );
    format!("{detail_level:?}::{thread_sig}::{task_sig}")
}

fn minimal_welcome_content(context: &WelcomeContext) -> String {
    match (
        context.recent_threads.first(),
        context.latest_goal_run.as_ref(),
    ) {
        (Some(last), Some(task)) => format!(
            "Welcome back! Last session: **{}** ({}). Latest goal: **{}** [{}] ({}).{}{}{} Goals: {} running, {} paused.",
            last.title,
            format_timestamp(last.updated_at),
            task.label,
            goal_run_status_label(task.status),
            format_timestamp(task.updated_at),
            task.summary
                .as_deref()
                .map(|summary| format!(" Summary: {}.", summary))
                .unwrap_or_default(),
            task.prompt
                .as_deref()
                .map(|goal_prompt| format!(" Goal prompt: {}.", goal_prompt))
                .unwrap_or_default(),
            task.latest_step_result
                .as_deref()
                .map(|result| format!(" Latest step result: {}.", result))
                .unwrap_or_default(),
            context.running_goal_total,
            context.paused_goal_total
        ),
        (Some(last), None) => format!(
            "Welcome back! Last session: **{}** ({}). {} messages.",
            last.title,
            format_timestamp(last.updated_at),
            last.message_count
        ),
        (None, Some(task)) => format!(
            "Welcome back! Latest goal: **{}** [{}] ({}).{}{}{} Goals: {} running, {} paused.",
            task.label,
            goal_run_status_label(task.status),
            format_timestamp(task.updated_at),
            task.summary
                .as_deref()
                .map(|summary| format!(" Summary: {}.", summary))
                .unwrap_or_default(),
            task.prompt
                .as_deref()
                .map(|goal_prompt| format!(" Goal prompt: {}.", goal_prompt))
                .unwrap_or_default(),
            task.latest_step_result
                .as_deref()
                .map(|result| format!(" Latest step result: {}.", result))
                .unwrap_or_default(),
            context.running_goal_total,
            context.paused_goal_total
        ),
        (None, None) => "Welcome to zorai! Ready to start your first session.".into(),
    }
}

fn goal_run_status_label(status: GoalRunStatus) -> &'static str {
    match status {
        GoalRunStatus::Queued => "queued",
        GoalRunStatus::Planning => "planning",
        GoalRunStatus::Running => "running",
        GoalRunStatus::AwaitingApproval => "awaiting approval",
        GoalRunStatus::AwaitingReview => "awaiting review",
        GoalRunStatus::Paused => "paused",
        GoalRunStatus::Blocked => "blocked",
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed => "failed",
        GoalRunStatus::Cancelled => "cancelled",
        GoalRunStatus::Contained => "contained",
        GoalRunStatus::Compensated => "compensated",
        GoalRunStatus::PartiallyCompensated => "partially compensated",
        GoalRunStatus::BreakGlass => "break-glass",
    }
}
