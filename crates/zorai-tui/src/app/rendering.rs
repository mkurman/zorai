use super::*;
use ratatui::style::{Color, Style};

const MIN_HEADER_CONTEXT_TARGET_TOKENS: u32 = 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConversationAgentProfile {
    pub(crate) agent_label: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) context_window_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
struct HeaderContextVm {
    profile: ConversationAgentProfile,
    usage: widgets::header::HeaderUsageDisplay,
    session_duration_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationAgentKind {
    Swarog,
    Rarog,
    Weles,
}

fn current_unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn session_duration_secs_from_created_at(created_at_ms: u64, now_ms: u64) -> Option<u64> {
    if created_at_ms == 0 || created_at_ms > now_ms {
        return None;
    }

    Some(now_ms.saturating_sub(created_at_ms) / 1000)
}

fn comma_list_contains(list: &str, needle: &str) -> bool {
    list.split(',')
        .map(str::trim)
        .any(|item| !item.is_empty() && item == needle)
}

fn openrouter_provider_picker_lines(model: &TuiModel, theme: &ThemeTokens) -> Vec<Line<'static>> {
    let preferred_target = matches!(
        model.settings_picker_target,
        Some(
            SettingsPickerTarget::OpenRouterPreferredProviders
                | SettingsPickerTarget::SubAgentOpenRouterPreferredProviders
                | SettingsPickerTarget::ConciergeOpenRouterPreferredProviders
        )
    );
    let mut lines = vec![
        Line::from(Span::styled(
            if preferred_target {
                "  Toggle preferred OpenRouter endpoint providers"
            } else {
                "  Toggle excluded OpenRouter endpoint providers"
            },
            theme.fg_dim,
        )),
        Line::raw(""),
    ];
    let provider_slugs = model.filtered_openrouter_endpoint_providers();
    if provider_slugs.is_empty() {
        let message = if model.config.openrouter_endpoint_providers.is_empty() {
            "  No endpoint providers loaded for this model".to_string()
        } else {
            format!(
                "  No endpoint providers match '{}'",
                model.modal.command_query().trim()
            )
        };
        lines.push(Line::from(Span::styled(message, theme.fg_dim)));
        return lines;
    }

    for (idx, slug) in provider_slugs.iter().enumerate() {
        let selected = model.modal.picker_cursor() == idx;
        let checked = match model.settings_picker_target {
            Some(SettingsPickerTarget::SubAgentOpenRouterPreferredProviders) => {
                model.subagents.editor.as_ref().is_some_and(|editor| {
                    comma_list_contains(&editor.openrouter_provider_order, slug)
                })
            }
            Some(SettingsPickerTarget::SubAgentOpenRouterExcludedProviders) => {
                model.subagents.editor.as_ref().is_some_and(|editor| {
                    comma_list_contains(&editor.openrouter_provider_ignore, slug)
                })
            }
            Some(SettingsPickerTarget::ConciergeOpenRouterPreferredProviders) => {
                comma_list_contains(&model.concierge.openrouter_provider_order, slug)
            }
            Some(SettingsPickerTarget::ConciergeOpenRouterExcludedProviders) => {
                comma_list_contains(&model.concierge.openrouter_provider_ignore, slug)
            }
            _ if preferred_target => {
                comma_list_contains(&model.config.openrouter_provider_order, slug)
            }
            _ => comma_list_contains(&model.config.openrouter_provider_ignore, slug),
        };
        let marker = if selected { ">" } else { " " };
        let check = if checked { "[x]" } else { "[ ]" };
        let style = if selected {
            theme.accent_primary
        } else if checked {
            theme.fg_active
        } else {
            theme.fg_dim
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {marker} "), style),
            Span::styled(format!("{check} "), style),
            Span::styled((*slug).to_string(), style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  Enter toggles. Esc closes.",
        theme.fg_dim,
    )));
    lines
}

fn render_runtime_effort_picker(
    frame: &mut Frame,
    area: Rect,
    modal: &modal::ModalState,
    current_effort: Option<&str>,
    theme: &ThemeTokens,
) {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

    let block = Block::default()
        .title(" EFFORT ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let efforts = [
        ("", "None"),
        ("minimal", "Minimal"),
        ("low", "Low"),
        ("medium", "Medium"),
        ("high", "High"),
        ("xhigh", "Extra High"),
        ("max", "Max"),
    ];
    let cursor = modal.picker_cursor();
    let current = current_effort.unwrap_or("");
    let items: Vec<ListItem> = efforts
        .iter()
        .enumerate()
        .map(|(i, (value, label))| {
            let is_current = *value == current;
            let marker = if is_current { "\u{25cf} " } else { "  " };
            if i == cursor {
                ListItem::new(Line::from(vec![
                    Span::raw("> "),
                    Span::raw(marker),
                    Span::raw(*label),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else {
                let style = if is_current {
                    theme.accent_primary
                } else {
                    theme.fg_dim
                };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(marker),
                    Span::styled(*label, style),
                ]))
            }
        })
        .collect();
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    frame.render_widget(List::new(items), inner_chunks[0]);
    let hints = Line::from(vec![
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), inner_chunks[1]);
}

impl TuiModel {
    fn sticky_thread_activity_text(&self, activity: &str) -> String {
        let normalized = activity.trim().to_ascii_lowercase();
        let variants = if normalized == "thinking"
            || normalized == "reasoning"
            || normalized == "skill gate"
        {
            &[
                "thinking",
                "analyzing",
                "working through",
                "mapping context",
                "sorting context",
            ][..]
        } else if normalized == "writing" || normalized.contains("crafting response") {
            &[
                "crafting",
                "writing",
                "composing",
                "drafting",
                "shaping reply",
            ][..]
        } else if normalized.starts_with('\u{2699}') || normalized.contains("tool") {
            &[
                "calling tools",
                "using tools",
                "checking tools",
                "running tools",
                "waiting on tools",
            ][..]
        } else {
            return activity.trim().to_string();
        };
        let variant_index = ((self.tick_counter / 40) as usize) % variants.len();

        variants[variant_index].to_string()
    }

    fn effective_primary_context_window_tokens(&self) -> u32 {
        if let Some(context_window) = providers::resolve_context_window_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            context_window
        } else if providers::model_uses_context_window_override(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            self.config
                .custom_context_window_tokens
                .unwrap_or(providers::default_custom_model_context_window())
        } else {
            providers::known_context_window_for(&self.config.provider, &self.config.model)
                .unwrap_or(self.config.context_window_tokens)
        }
    }

    fn current_header_context_window_tokens(&self, profile: &ConversationAgentProfile) -> u32 {
        let fallback = if matches!(
            self.main_pane_view,
            MainPaneView::Task(SidebarItemTarget::GoalRun { .. })
        ) {
            self.effective_primary_context_window_tokens()
        } else {
            match self.current_conversation_agent_kind() {
                ConversationAgentKind::Swarog => self.effective_primary_context_window_tokens(),
                ConversationAgentKind::Rarog => self.effective_primary_context_window_tokens(),
                ConversationAgentKind::Weles => match self.config.compaction_strategy.as_str() {
                    "custom_model" => self.config.compaction_custom_context_window_tokens,
                    _ => self.config.context_window_tokens,
                },
            }
        }
        .max(1);

        profile
            .context_window_tokens
            .or_else(|| providers::known_context_window_for(&profile.provider, &profile.model))
            .unwrap_or(fallback)
            .max(1)
    }

    fn current_header_weles_compaction_window_tokens(&self, primary_window: u32) -> u32 {
        let provider = self.config.compaction_weles_provider.trim();
        let model = self.config.compaction_weles_model.trim();
        if provider.is_empty() || model.is_empty() {
            return primary_window.max(1);
        }

        providers::known_context_window_for(provider, model)
            .unwrap_or(primary_window)
            .max(1)
    }

    fn current_header_context_target_tokens(&self, profile: &ConversationAgentProfile) -> u32 {
        let context_window = self.current_header_context_window_tokens(profile).max(1);
        if !self.config.auto_compact_context {
            return context_window;
        }

        let threshold_pct = self.config.compact_threshold_pct.clamp(1, 100);
        let threshold_target = context_window.saturating_mul(threshold_pct) / 100;
        let strategy_cap = match self.config.compaction_strategy.as_str() {
            "weles" => {
                self.current_header_weles_compaction_window_tokens(context_window)
                    .saturating_mul(threshold_pct)
                    / 100
            }
            "custom_model" => {
                self.config
                    .compaction_custom_context_window_tokens
                    .max(1)
                    .saturating_mul(threshold_pct)
                    / 100
            }
            _ => threshold_target,
        };

        threshold_target
            .max(MIN_HEADER_CONTEXT_TARGET_TOKENS)
            .min(strategy_cap.max(MIN_HEADER_CONTEXT_TARGET_TOKENS))
            .max(1)
    }

    pub(super) fn conversation_participant_summary_area(&self) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return None;
        }
        if self.should_show_provider_onboarding()
            || self.should_show_daemon_connection_loading()
            || self.should_show_local_landing()
            || self.should_show_concierge_hero_loading()
            || self.should_show_thread_loading()
        {
            return None;
        }

        let summary_height = self.thread_participants_panel_height()?;
        let chat_area = self.conversation_area_below_return_banner();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(summary_height), Constraint::Min(1)])
            .split(chat_area);
        Some(chunks[0])
    }

    fn conversation_area_below_return_banner(&self) -> Rect {
        let chat_area = self.pane_layout().chat;
        if self.conversation_return_area().is_some() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(chat_area);
            chunks[1]
        } else {
            chat_area
        }
    }

    pub(super) fn conversation_content_area(&self) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return None;
        }

        let area = self.conversation_area_below_return_banner();
        if let Some(summary_height) = self.thread_participants_panel_height() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(summary_height), Constraint::Min(1)])
                .split(area);
            Some(chunks[1])
        } else {
            Some(area)
        }
    }

    pub(super) fn thread_participants_summary(&self) -> Option<String> {
        let has_auto_response = self.active_auto_response_suggestion().is_some()
            || self.active_always_auto_response_participant().is_some();
        self.chat.active_thread().and_then(|thread| {
            let active: Vec<&str> = thread
                .thread_participants
                .iter()
                .filter(|participant| participant.status.eq_ignore_ascii_case("active"))
                .map(|participant| participant.agent_name.as_str())
                .collect();
            let inactive_count = thread
                .thread_participants
                .iter()
                .filter(|participant| !participant.status.eq_ignore_ascii_case("active"))
                .count();
            let queued_count = thread
                .queued_participant_suggestions
                .iter()
                .filter(|suggestion| {
                    !suggestion
                        .suggestion_kind
                        .eq_ignore_ascii_case("auto_response")
                })
                .count();
            if active.is_empty() && inactive_count == 0 && queued_count == 0 && !has_auto_response {
                return None;
            }

            let active_summary = if active.is_empty() {
                "active: none".to_string()
            } else {
                let names = active.into_iter().take(3).collect::<Vec<_>>().join(", ");
                format!("active: {names}")
            };

            Some(format!(
                "Participants  •  {}  •  inactive: {}  •  queued: {}  •  /participants",
                active_summary, inactive_count, queued_count
            ))
        })
    }

    pub(super) fn thread_participants_panel_height(&self) -> Option<u16> {
        self.thread_participants_summary()?;
        if self.active_auto_response_countdown_secs().is_some()
            || self.active_always_auto_response_participant().is_some()
        {
            Some(4)
        } else {
            Some(3)
        }
    }

    fn conversation_return_area(&self) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return None;
        }
        if self.should_show_provider_onboarding()
            || self.should_show_daemon_connection_loading()
            || self.should_show_local_landing()
            || self.should_show_concierge_hero_loading()
            || !self.has_mission_control_return_target()
        {
            return None;
        }

        let chat_area = self.pane_layout().chat;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(chat_area);
        Some(chunks[0])
    }

    pub(super) fn conversation_return_to_goal_button_area(&self) -> Option<Rect> {
        let area = self.conversation_return_area()?;
        if self.mission_control_return_to_thread_id().is_some() {
            widgets::goal_mission_control::return_to_thread_button_area(area)
        } else if self.mission_control_return_to_workspace() {
            widgets::goal_mission_control::return_to_workspace_button_area(area)
        } else {
            widgets::goal_mission_control::return_to_goal_button_area(area)
        }
    }

    fn render_conversation_return_banner(&self, frame: &mut Frame, area: Rect) {
        if self.mission_control_return_to_thread_id().is_some() {
            widgets::goal_mission_control::render_return_to_thread_banner(frame, area, &self.theme);
        } else if self.mission_control_return_to_workspace() {
            widgets::goal_mission_control::render_return_to_workspace_banner(
                frame,
                area,
                &self.theme,
            );
        } else {
            widgets::goal_mission_control::render_return_to_goal_banner(frame, area, &self.theme);
        }
    }

    fn task_return_area_for(&self, area: Rect) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Task(_))
            || !self.mission_control_return_to_workspace()
        {
            return None;
        }
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);
        Some(chunks[0])
    }

    pub(super) fn task_content_area(&self) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Task(_)) {
            return None;
        }
        let chat_area = self.pane_layout().chat;
        if self.task_return_area_for(chat_area).is_some() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(chat_area);
            Some(chunks[1])
        } else {
            Some(chat_area)
        }
    }

    pub(super) fn task_return_to_workspace_button_area(&self) -> Option<Rect> {
        let area = self.task_return_area_for(self.pane_layout().chat)?;
        widgets::goal_mission_control::return_to_workspace_button_area(area)
    }

    fn render_task_return_banner(&self, frame: &mut Frame, area: Rect) {
        widgets::goal_mission_control::render_return_to_workspace_banner(frame, area, &self.theme);
    }

    fn configured_model_label(model: &str, custom_model_name: &str) -> String {
        let custom = custom_model_name.trim();
        if !custom.is_empty() && custom != model {
            custom.to_string()
        } else if model.trim().is_empty() {
            "no model".to_string()
        } else {
            model.to_string()
        }
    }

    fn current_conversation_agent_kind(&self) -> ConversationAgentKind {
        if let Some(thread) = self.chat.active_thread() {
            if thread.id == "concierge"
                || thread
                    .agent_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(zorai_protocol::AGENT_NAME_RAROG))
                || widgets::thread_picker::is_rarog_thread(thread)
            {
                return ConversationAgentKind::Rarog;
            }

            if thread
                .agent_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("weles"))
                || widgets::thread_picker::is_weles_thread(thread)
            {
                return ConversationAgentKind::Weles;
            }

            return ConversationAgentKind::Swarog;
        }

        match self.pending_new_thread_target_agent.as_deref() {
            Some(agent_id) if agent_id == zorai_protocol::AGENT_ID_RAROG => {
                ConversationAgentKind::Rarog
            }
            Some("weles") => ConversationAgentKind::Weles,
            _ => ConversationAgentKind::Swarog,
        }
    }

    fn identity_matches_alias(
        alias: &str,
        agent_id: Option<&str>,
        agent_name: Option<&str>,
    ) -> bool {
        agent_id.is_some_and(|value| value.eq_ignore_ascii_case(alias))
            || agent_name.is_some_and(|value| value.eq_ignore_ascii_case(alias))
    }

    fn subagent_entry_context_window_tokens(entry: &crate::state::SubAgentEntry) -> Option<u32> {
        entry
            .raw_json
            .as_ref()
            .and_then(|raw| raw.get("context_window_tokens"))
            .and_then(|value| value.as_u64())
            .filter(|tokens| *tokens > 0)
            .map(|tokens| tokens.min(u32::MAX as u64) as u32)
    }

    fn subagent_profile_for_identity(
        &self,
        agent_id: Option<&str>,
        agent_name: Option<&str>,
    ) -> Option<ConversationAgentProfile> {
        let entry = self.subagents.entries.iter().find(|entry| {
            Self::identity_matches_alias(&entry.id, agent_id, agent_name)
                || Self::identity_matches_alias(&entry.name, agent_id, agent_name)
                || entry
                    .id
                    .strip_suffix("_builtin")
                    .is_some_and(|alias| Self::identity_matches_alias(alias, agent_id, agent_name))
        })?;

        Some(ConversationAgentProfile {
            agent_label: entry.name.clone(),
            provider: entry.provider.clone(),
            model: entry.model.clone(),
            reasoning_effort: entry
                .reasoning_effort
                .clone()
                .filter(|value| !value.is_empty()),
            context_window_tokens: Self::subagent_entry_context_window_tokens(entry)
                .or_else(|| providers::known_context_window_for(&entry.provider, &entry.model)),
        })
    }

    fn thread_profile(thread: &chat::AgentThread) -> Option<ConversationAgentProfile> {
        let provider = thread.profile_provider.as_deref()?.trim();
        let model = thread.profile_model.as_deref()?.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }

        Some(ConversationAgentProfile {
            agent_label: thread
                .agent_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("Swarog")
                .to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            reasoning_effort: thread
                .profile_reasoning_effort
                .clone()
                .filter(|value| !value.trim().is_empty()),
            context_window_tokens: thread.profile_context_window_tokens,
        })
    }

    fn active_thread_profile(&self) -> Option<ConversationAgentProfile> {
        self.chat
            .active_thread()
            .and_then(|thread| self.thread_owner_profile(thread))
    }

    fn thread_owner_profile(&self, thread: &chat::AgentThread) -> Option<ConversationAgentProfile> {
        if let Some(profile) = Self::thread_profile(thread) {
            return Some(profile);
        }

        let agent_name = thread
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let agent_name = agent_name?;

        if Self::identity_matches_alias(zorai_protocol::AGENT_NAME_RAROG, None, Some(agent_name))
            || Self::identity_matches_alias(zorai_protocol::AGENT_ID_RAROG, None, Some(agent_name))
        {
            return Some(self.rarog_profile());
        }

        if Self::identity_matches_alias("weles", None, Some(agent_name)) {
            return Some(self.weles_profile());
        }

        if let Some(profile) = self.subagent_profile_for_identity(None, Some(agent_name)) {
            return Some(profile);
        }

        let mut profile = self.svarog_profile();
        profile.agent_label = agent_name.to_string();
        Some(profile)
    }

    fn svarog_profile(&self) -> ConversationAgentProfile {
        ConversationAgentProfile {
            agent_label: "Swarog".to_string(),
            provider: self.config.provider.clone(),
            model: Self::configured_model_label(&self.config.model, &self.config.custom_model_name),
            reasoning_effort: (!self.config.reasoning_effort.trim().is_empty())
                .then(|| self.config.reasoning_effort.clone()),
            context_window_tokens: Some(self.effective_primary_context_window_tokens()),
        }
    }

    fn rarog_profile(&self) -> ConversationAgentProfile {
        let provider = self
            .concierge
            .provider
            .as_deref()
            .filter(|value| !value.is_empty())
            .zip(
                self.concierge
                    .model
                    .as_deref()
                    .filter(|value| !value.is_empty()),
            );

        if let Some((provider, model)) = provider {
            ConversationAgentProfile {
                agent_label: zorai_protocol::AGENT_NAME_RAROG.to_string(),
                provider: provider.to_string(),
                model: model.to_string(),
                reasoning_effort: self
                    .concierge
                    .reasoning_effort
                    .clone()
                    .filter(|value| !value.is_empty()),
                context_window_tokens: providers::known_context_window_for(provider, model),
            }
        } else {
            ConversationAgentProfile {
                agent_label: zorai_protocol::AGENT_NAME_RAROG.to_string(),
                provider: self.config.provider.clone(),
                model: Self::configured_model_label(
                    &self.config.model,
                    &self.config.custom_model_name,
                ),
                reasoning_effort: (!self.config.reasoning_effort.trim().is_empty())
                    .then(|| self.config.reasoning_effort.clone()),
                context_window_tokens: Some(self.effective_primary_context_window_tokens()),
            }
        }
    }

    fn weles_profile(&self) -> ConversationAgentProfile {
        if let Some(entry) = self.subagents.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case("weles_builtin")
                || entry.name.eq_ignore_ascii_case("weles")
        }) {
            return ConversationAgentProfile {
                agent_label: "Weles".to_string(),
                provider: entry.provider.clone(),
                model: entry.model.clone(),
                reasoning_effort: entry
                    .reasoning_effort
                    .clone()
                    .filter(|value| !value.is_empty()),
                context_window_tokens: Self::subagent_entry_context_window_tokens(entry)
                    .or_else(|| providers::known_context_window_for(&entry.provider, &entry.model)),
            };
        }

        let raw_weles = self
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("builtin_sub_agents"))
            .and_then(|value| value.get("weles"));

        let provider = raw_weles
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(self.config.compaction_weles_provider.as_str())
            .to_string();
        let model = raw_weles
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(self.config.compaction_weles_model.as_str())
            .to_string();
        let reasoning_effort = raw_weles
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                (!self
                    .config
                    .compaction_weles_reasoning_effort
                    .trim()
                    .is_empty())
                .then(|| self.config.compaction_weles_reasoning_effort.clone())
            });
        let context_window_tokens =
            providers::known_context_window_for(&provider, &model).or_else(|| {
                match self.config.compaction_strategy.as_str() {
                    "custom_model" => Some(self.config.compaction_custom_context_window_tokens),
                    _ => Some(self.config.context_window_tokens),
                }
            });

        ConversationAgentProfile {
            agent_label: "Weles".to_string(),
            provider,
            model: if model.trim().is_empty() {
                "no model".to_string()
            } else {
                model
            },
            reasoning_effort,
            context_window_tokens,
        }
    }

    pub(crate) fn current_conversation_agent_profile(&self) -> ConversationAgentProfile {
        if let Some(profile) = self.active_thread_profile() {
            return profile;
        }
        if let Some(agent_id) = self.pending_new_thread_target_agent.as_deref() {
            if agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_SWAROG) {
                return self.svarog_profile();
            }
            if agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_RAROG) {
                return self.rarog_profile();
            }
            if agent_id.eq_ignore_ascii_case("weles") {
                return self.weles_profile();
            }
            if let Some(profile) =
                self.subagent_profile_for_identity(Some(agent_id), Some(agent_id))
            {
                return profile;
            }
        }

        match self.current_conversation_agent_kind() {
            ConversationAgentKind::Swarog => self.svarog_profile(),
            ConversationAgentKind::Rarog => self.rarog_profile(),
            ConversationAgentKind::Weles => self.weles_profile(),
        }
    }

    fn goal_runtime_owner_profile_to_header_profile(
        profile: &task::GoalRuntimeOwnerProfile,
    ) -> Option<ConversationAgentProfile> {
        let provider = profile.provider.trim();
        let model = profile.model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }

        Some(ConversationAgentProfile {
            agent_label: if profile.agent_label.trim().is_empty() {
                "Swarog".to_string()
            } else {
                profile.agent_label.clone()
            },
            provider: provider.to_string(),
            model: model.to_string(),
            reasoning_effort: profile
                .reasoning_effort
                .clone()
                .filter(|value| !value.trim().is_empty()),
            context_window_tokens: providers::known_context_window_for(provider, model),
        })
    }

    fn goal_assignment_to_header_profile(
        assignment: &task::GoalAgentAssignment,
    ) -> Option<ConversationAgentProfile> {
        let provider = assignment.provider.trim();
        let model = assignment.model.trim();
        if provider.is_empty() || model.is_empty() {
            return None;
        }

        let normalized_role = assignment.role_id.trim().to_ascii_lowercase();
        let agent_label = if normalized_role == zorai_protocol::AGENT_ID_SWAROG {
            "Swarog".to_string()
        } else if normalized_role == zorai_protocol::AGENT_ID_RAROG {
            zorai_protocol::AGENT_NAME_RAROG.to_string()
        } else {
            normalized_role
                .split(['-', '_'])
                .filter(|segment| !segment.is_empty())
                .map(|segment| {
                    let mut chars = segment.chars();
                    let Some(first) = chars.next() else {
                        return String::new();
                    };
                    let mut title = String::new();
                    title.push(first.to_ascii_uppercase());
                    title.push_str(chars.as_str());
                    title
                })
                .collect::<Vec<_>>()
                .join(" ")
        };

        Some(ConversationAgentProfile {
            agent_label,
            provider: provider.to_string(),
            model: model.to_string(),
            reasoning_effort: assignment
                .reasoning_effort
                .clone()
                .filter(|value| !value.trim().is_empty()),
            context_window_tokens: providers::known_context_window_for(provider, model),
        })
    }

    fn goal_run_reserved_thread_profile(
        &self,
        run: &task::GoalRun,
    ) -> Option<ConversationAgentProfile> {
        self.goal_run_header_thread(run)
            .and_then(Self::thread_profile)
    }

    fn goal_run_header_run(&self) -> Option<&task::GoalRun> {
        let MainPaneView::Task(SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return None;
        };

        self.tasks.goal_run_by_id(goal_run_id)
    }

    fn goal_run_header_thread(&self, run: &task::GoalRun) -> Option<&chat::AgentThread> {
        [
            run.root_thread_id.as_deref(),
            run.thread_id.as_deref(),
            run.active_thread_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find_map(|thread_id| {
            self.chat
                .threads()
                .iter()
                .find(|thread| thread.id == thread_id)
        })
    }

    fn goal_run_launch_header_profile(
        &self,
        run: &task::GoalRun,
    ) -> Option<ConversationAgentProfile> {
        run.launch_assignment_snapshot
            .iter()
            .find(|assignment| assignment.role_id == zorai_protocol::AGENT_ID_SWAROG)
            .or_else(|| run.launch_assignment_snapshot.first())
            .and_then(Self::goal_assignment_to_header_profile)
    }

    fn goal_run_owner_header_profile(
        &self,
        run: &task::GoalRun,
    ) -> Option<ConversationAgentProfile> {
        run.current_step_owner_profile
            .as_ref()
            .and_then(Self::goal_runtime_owner_profile_to_header_profile)
            .or_else(|| {
                run.planner_owner_profile
                    .as_ref()
                    .and_then(Self::goal_runtime_owner_profile_to_header_profile)
            })
    }

    fn goal_run_runtime_thread_profile(
        &self,
        run: &task::GoalRun,
    ) -> Option<ConversationAgentProfile> {
        let thread = self.goal_run_header_thread(run)?;
        if thread.runtime_provider.is_none()
            && thread.runtime_model.is_none()
            && thread.runtime_reasoning_effort.is_none()
        {
            return None;
        }

        let fallback = self
            .goal_run_owner_header_profile(run)
            .or_else(|| self.goal_run_launch_header_profile(run))
            .unwrap_or_else(|| self.svarog_profile());
        let provider = thread
            .runtime_provider
            .clone()
            .unwrap_or_else(|| fallback.provider.clone());
        let model = thread
            .runtime_model
            .clone()
            .unwrap_or_else(|| fallback.model.clone());
        let fallback_context_window_tokens = (provider == fallback.provider
            && model == fallback.model)
            .then_some(fallback.context_window_tokens)
            .flatten();
        let context_window_tokens = fallback_context_window_tokens
            .or_else(|| providers::known_context_window_for(&provider, &model));
        Some(ConversationAgentProfile {
            agent_label: thread
                .agent_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .unwrap_or(fallback.agent_label),
            provider,
            model,
            reasoning_effort: thread
                .runtime_reasoning_effort
                .clone()
                .or(fallback.reasoning_effort),
            context_window_tokens,
        })
    }

    fn goal_run_header_profile(&self) -> Option<ConversationAgentProfile> {
        let run = self.goal_run_header_run()?;

        Some(
            self.goal_run_runtime_thread_profile(run)
                .or_else(|| self.goal_run_reserved_thread_profile(run))
                .or_else(|| self.goal_run_owner_header_profile(run))
                .or_else(|| self.goal_run_launch_header_profile(run))
                .unwrap_or_else(|| self.svarog_profile()),
        )
    }

    fn current_header_profile_for_active_pane(&self) -> ConversationAgentProfile {
        if let Some(profile) = self.goal_run_header_profile() {
            return profile;
        }
        if self.should_show_concierge_hero_loading() {
            return self.rarog_profile();
        }

        let fallback = self.current_conversation_agent_profile();
        if let Some(runtime) = self.chat.active_thread_runtime_metadata() {
            let provider = runtime
                .provider
                .unwrap_or_else(|| fallback.provider.clone());
            let model = runtime.model.unwrap_or_else(|| fallback.model.clone());
            let fallback_context_window_tokens = (provider == fallback.provider
                && model == fallback.model)
                .then_some(fallback.context_window_tokens)
                .flatten();
            let context_window_tokens = fallback_context_window_tokens
                .or_else(|| providers::known_context_window_for(&provider, &model));
            return ConversationAgentProfile {
                agent_label: fallback.agent_label,
                provider,
                model,
                reasoning_effort: runtime.reasoning_effort.or(fallback.reasoning_effort),
                context_window_tokens,
            };
        }

        fallback
    }

    fn goal_run_usage_thread(&self) -> Option<&chat::AgentThread> {
        self.goal_run_header_run()
            .and_then(|run| self.goal_run_header_thread(run))
    }

    fn current_header_usage_thread(&self) -> Option<&chat::AgentThread> {
        if matches!(
            self.main_pane_view,
            MainPaneView::Task(SidebarItemTarget::GoalRun { .. })
        ) {
            return self.goal_run_usage_thread();
        }

        self.chat.active_thread()
    }

    fn current_header_context_vm(&self) -> HeaderContextVm {
        let profile = self.current_header_profile_for_active_pane();
        let thread = self.current_header_usage_thread();
        let usage = self.current_header_usage_summary_for_profile(&profile, thread);
        let session_duration_secs = thread.and_then(|thread| {
            session_duration_secs_from_created_at(thread.created_at, current_unix_time_ms())
        });
        HeaderContextVm {
            profile,
            usage,
            session_duration_secs,
        }
    }

    #[cfg(test)]
    pub(crate) fn current_header_agent_profile(&self) -> ConversationAgentProfile {
        self.current_header_profile_for_active_pane()
    }

    pub(crate) fn invalidate_active_header_runtime_profile_if_profile_changed(
        &mut self,
        before: &ConversationAgentProfile,
    ) {
        let after = self.current_conversation_agent_profile();
        if before.agent_label == after.agent_label
            && (before.provider != after.provider
                || before.model != after.model
                || before.reasoning_effort != after.reasoning_effort)
        {
            self.chat.clear_active_thread_runtime_metadata();
        }
    }

    fn current_header_usage_summary_for_profile(
        &self,
        profile: &ConversationAgentProfile,
        thread: Option<&chat::AgentThread>,
    ) -> widgets::header::HeaderUsageDisplay {
        let context_window_tokens =
            self.current_header_context_window_tokens(profile).max(1) as u64;
        let compaction_target_tokens =
            self.current_header_context_target_tokens(profile).max(1) as u64;
        let compaction_model_window_tokens =
            self.current_header_compaction_model_window_tokens(context_window_tokens);
        let Some(thread) = thread else {
            return widgets::header::HeaderUsageDisplay {
                total_thread_tokens: 0,
                current_tokens: 0,
                context_window_tokens,
                compaction_target_tokens,
                compaction_model_window_tokens,
                utilization_pct: 0,
                total_cost_usd: None,
            };
        };

        let total_thread_tokens = thread.total_input_tokens + thread.total_output_tokens;
        let current_tokens = thread.active_context_window_tokens.unwrap_or(0);
        let total_cost_usd = thread
            .messages
            .iter()
            .filter_map(|message| message.cost)
            .reduce(|acc, cost| acc + cost);

        let utilization_pct = current_tokens
            .saturating_mul(100)
            .checked_div(context_window_tokens)
            .unwrap_or(0)
            .min(100) as u8;

        widgets::header::HeaderUsageDisplay {
            total_thread_tokens,
            current_tokens,
            context_window_tokens,
            compaction_target_tokens,
            compaction_model_window_tokens,
            utilization_pct,
            total_cost_usd,
        }
    }

    fn current_header_compaction_model_window_tokens(
        &self,
        primary_window_tokens: u64,
    ) -> Option<u64> {
        let primary_window_u32 = primary_window_tokens.min(u32::MAX as u64) as u32;
        let candidate = match self.config.compaction_strategy.as_str() {
            "weles" => self.current_header_weles_compaction_window_tokens(primary_window_u32),
            "custom_model" => self.config.compaction_custom_context_window_tokens.max(1),
            _ => return None,
        } as u64;
        if candidate < primary_window_tokens {
            Some(candidate)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn current_header_usage_summary(&self) -> widgets::header::HeaderUsageDisplay {
        self.current_header_context_vm().usage
    }

    fn render_conversation_panel(&mut self, frame: &mut Frame, area: Rect) {
        if self.should_show_provider_onboarding() {
            widgets::onboarding::render(frame, area, &self.config, &self.theme);
            return;
        }

        if self.should_show_daemon_connection_loading() {
            widgets::landing::render_connection_waiting(
                frame,
                area,
                &self.theme,
                self.tick_counter,
            );
            return;
        }

        if self.should_show_local_landing() {
            let profile = self.current_conversation_agent_profile();
            widgets::landing::render(frame, area, &self.theme, &profile.agent_label);
            return;
        }

        if self.should_show_concierge_hero_loading() {
            widgets::concierge_loading::render(frame, area, &self.theme, self.tick_counter);
            return;
        }

        if self.should_show_thread_loading() {
            let mut loading_area = area;
            if let Some(return_area) = self.conversation_return_area() {
                self.render_conversation_return_banner(frame, return_area);
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(1)])
                    .split(area);
                loading_area = chunks[1];
            }
            let thread_title = self
                .chat
                .active_thread()
                .map(|thread| thread.title.as_str());
            widgets::concierge_loading::render_thread(
                frame,
                loading_area,
                &self.theme,
                self.tick_counter,
                thread_title,
            );
            return;
        }

        if self
            .chat_selection_snapshot
            .as_ref()
            .is_some_and(|snapshot| !widgets::chat::cached_snapshot_matches_area(snapshot, area))
        {
            self.chat_selection_snapshot = None;
        }

        let mut area = area;
        if let Some(return_area) = self.conversation_return_area() {
            self.render_conversation_return_banner(frame, return_area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            area = chunks[1];
        }

        let participant_summary = self.thread_participants_summary();
        let auto_response_countdown_secs = self.active_auto_response_countdown_secs();
        let always_auto_response_participant = self.active_always_auto_response_participant();

        let area = if let Some(summary) = participant_summary.as_deref() {
            let summary_height = self.thread_participants_panel_height().unwrap_or(3);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(summary_height), Constraint::Min(1)])
                .split(area);
            let mut lines = vec![ratatui::text::Line::from(ratatui::text::Span::styled(
                summary.to_string(),
                self.theme.fg_dim,
            ))];
            if let Some(countdown_secs) = auto_response_countdown_secs {
                let yes_style = if self.auto_response_selection == AutoResponseActionSelection::Yes
                {
                    self.theme.accent_primary
                } else {
                    self.theme.fg_dim
                };
                let no_style = if self.auto_response_selection == AutoResponseActionSelection::No {
                    self.theme.accent_danger
                } else {
                    self.theme.fg_dim
                };
                let always_style =
                    if self.auto_response_selection == AutoResponseActionSelection::Always {
                        self.theme.accent_secondary
                    } else {
                        self.theme.fg_dim
                    };
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("Auto response in ", self.theme.fg_active),
                    ratatui::text::Span::styled(
                        format!("{countdown_secs}s"),
                        self.theme.accent_primary,
                    ),
                    ratatui::text::Span::styled("  ", self.theme.fg_dim),
                    ratatui::text::Span::styled(format!("[Yes {}s]", countdown_secs), yes_style),
                    ratatui::text::Span::styled(" ", self.theme.fg_dim),
                    ratatui::text::Span::styled("[No]", no_style),
                    ratatui::text::Span::styled(" ", self.theme.fg_dim),
                    ratatui::text::Span::styled("[Always for this thread]", always_style),
                ]));
            } else if let Some(participant) = always_auto_response_participant {
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("Auto response: ", self.theme.fg_active),
                    ratatui::text::Span::styled("always ", self.theme.accent_secondary),
                    ratatui::text::Span::styled(
                        participant.agent_name.clone(),
                        self.theme.accent_primary,
                    ),
                ]));
            }
            frame.render_widget(
                ratatui::widgets::Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title(" THREAD PARTICIPANTS ")
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(self.theme.accent_secondary),
                    )
                    .style(self.theme.fg_dim),
                chunks[0],
            );
            chunks[1]
        } else {
            area
        };

        self.clamp_chat_scroll_offset_for_area(area);

        let mouse_selection = self
            .chat_drag_anchor_point
            .zip(self.chat_drag_current_point)
            .or_else(|| {
                let cached_snapshot = self
                    .chat_selection_snapshot
                    .as_ref()
                    .filter(|snapshot| widgets::chat::cached_snapshot_matches_area(snapshot, area));
                self.chat_drag_anchor.and_then(|anchor| {
                    self.chat_drag_current.and_then(|current| {
                        if let Some(snapshot) = cached_snapshot {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, anchor)
                                .zip(widgets::chat::selection_point_from_cached_snapshot(
                                    snapshot, current,
                                ))
                        } else {
                            widgets::chat::selection_points_from_mouse(
                                area,
                                &self.chat,
                                &self.theme,
                                self.tick_counter,
                                anchor,
                                current,
                                self.retry_wait_start_selected,
                            )
                        }
                    })
                })
            })
            .filter(|(start, end)| start != end);

        let active_drag_selection = self.chat_drag_anchor.is_some() && mouse_selection.is_some();
        if active_drag_selection {
            if let Some(snapshot) = self.chat_selection_snapshot.as_ref().filter(|snapshot| {
                widgets::chat::cached_snapshot_matches_area(snapshot, area)
                    && widgets::chat::cached_snapshot_covers_window(snapshot, &self.chat)
            }) {
                widgets::chat::render_cached(
                    frame,
                    area,
                    &self.chat,
                    &self.theme,
                    snapshot,
                    mouse_selection,
                );
                return;
            }
        }

        if let Some(snapshot) = self.chat_selection_snapshot.as_ref().filter(|snapshot| {
            widgets::chat::cached_snapshot_matches_render(
                snapshot,
                area,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
            )
        }) {
            widgets::chat::render_cached(
                frame,
                area,
                &self.chat,
                &self.theme,
                snapshot,
                mouse_selection,
            );
            return;
        }

        if self
            .chat_selection_snapshot
            .as_ref()
            .is_some_and(|snapshot| {
                widgets::chat::cached_snapshot_matches_render_key(
                    snapshot,
                    area,
                    &self.chat,
                    self.tick_counter,
                    self.retry_wait_start_selected,
                )
            })
        {
            self.chat_selection_snapshot =
                self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
                    widgets::chat::refresh_cached_snapshot_window(
                        snapshot,
                        area,
                        &self.chat,
                        &self.theme,
                        self.tick_counter,
                        self.retry_wait_start_selected,
                    )
                });

            if let Some(snapshot) = self.chat_selection_snapshot.as_ref() {
                widgets::chat::render_cached(
                    frame,
                    area,
                    &self.chat,
                    &self.theme,
                    snapshot,
                    mouse_selection,
                );
                return;
            }
        }

        self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
        );

        if let Some(snapshot) = self.chat_selection_snapshot.as_ref() {
            widgets::chat::render_cached(
                frame,
                area,
                &self.chat,
                &self.theme,
                snapshot,
                mouse_selection,
            );
            return;
        }

        widgets::chat::render(
            frame,
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
            self.focus == FocusArea::Chat,
            mouse_selection,
        );
    }

    pub(crate) fn terminal_image_overlay_spec(
        &self,
    ) -> Option<crate::terminal_graphics::TerminalImageOverlaySpec> {
        if self.modal.top().is_some() {
            return None;
        }
        let layout = self.pane_layout();
        match &self.main_pane_view {
            MainPaneView::FilePreview(target) => {
                widgets::file_preview::terminal_image_overlay_spec(
                    layout.chat,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                )
            }
            MainPaneView::WorkContext => widgets::work_context_view::terminal_image_overlay_spec(
                layout.chat,
                &self.tasks,
                self.chat.active_thread_id(),
                self.sidebar.active_tab(),
                self.sidebar.selected_item(),
                &self.theme,
                self.task_view_scroll,
            ),
            _ => None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.width = area.width;
        self.height = area.height;
        let layout = self.pane_layout_for_area(area);
        let input_height = self.input_height();
        let anticipatory_height = self.anticipatory_banner_height();
        let concierge_height = self.concierge_banner_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(anticipatory_height),
                Constraint::Length(concierge_height),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

        let header = self.current_header_context_vm();

        widgets::header::render(
            frame,
            chunks[0],
            &header.profile.provider,
            &header.profile.model,
            header.profile.reasoning_effort.as_deref(),
            &header.usage,
            header.session_duration_secs,
            &self.theme,
            self.approval.pending_approvals().len(),
            self.modal.top() == Some(modal::ModalKind::ApprovalCenter),
            self.notifications.unread_count(),
            self.modal.top() == Some(modal::ModalKind::Notifications),
            self.system_monitor.as_ref(),
        );

        if let Some(sidebar_area) = layout.sidebar {
            match &self.main_pane_view {
                MainPaneView::Conversation => {
                    self.render_conversation_panel(frame, layout.chat);
                }
                MainPaneView::Collaboration => widgets::collaboration_view::render(
                    frame,
                    layout.chat,
                    &self.collaboration,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Workspace => widgets::workspace_board::render_with_scroll(
                    frame,
                    layout.chat,
                    &self.workspace,
                    &self.workspace_expanded_task_ids,
                    &self.workspace_board_scroll,
                    self.workspace_board_selection.as_ref(),
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Task(target) => {
                    let task_area = self.task_content_area().unwrap_or(layout.chat);
                    if let Some(return_area) = self.task_return_area_for(layout.chat) {
                        self.render_task_return_banner(frame, return_area);
                    }
                    if let SidebarItemTarget::GoalRun { goal_run_id, .. } = target {
                        widgets::goal_workspace::render_with_selection(
                            frame,
                            task_area,
                            &self.tasks,
                            goal_run_id,
                            &self.goal_workspace,
                            &self.theme,
                            self.tick_counter,
                            self.task_view_drag_anchor_point
                                .zip(self.task_view_drag_current_point)
                                .or_else(|| {
                                    self.task_view_drag_anchor.and_then(|anchor| {
                                        self.task_view_drag_current.and_then(|current| {
                                            widgets::goal_workspace::selection_points_from_mouse(
                                                task_area,
                                                &self.tasks,
                                                goal_run_id,
                                                &self.goal_workspace,
                                                anchor,
                                                current,
                                            )
                                        })
                                    })
                                }),
                        );
                    } else {
                        widgets::task_view::render(
                            frame,
                            task_area,
                            &self.tasks,
                            target,
                            &self.theme,
                            self.focus == FocusArea::Chat,
                            self.task_view_scroll,
                            self.task_show_live_todos,
                            self.task_show_timeline,
                            self.task_show_files,
                            self.tick_counter,
                            self.task_view_drag_anchor_point
                                .zip(self.task_view_drag_current_point)
                                .or_else(|| {
                                    self.task_view_drag_anchor.and_then(|anchor| {
                                        self.task_view_drag_current.and_then(|current| {
                                            widgets::task_view::selection_points_from_mouse(
                                                task_area,
                                                &self.tasks,
                                                target,
                                                &self.theme,
                                                self.task_view_scroll,
                                                self.task_show_live_todos,
                                                self.task_show_timeline,
                                                self.task_show_files,
                                                anchor,
                                                current,
                                            )
                                        })
                                    })
                                }),
                        );
                    }
                }
                MainPaneView::WorkContext => widgets::work_context_view::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                    self.work_context_drag_anchor_point
                        .zip(self.work_context_drag_current_point)
                        .or_else(|| {
                            self.work_context_drag_anchor.and_then(|anchor| {
                                self.work_context_drag_current.and_then(|current| {
                                    widgets::work_context_view::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        self.chat.active_thread_id(),
                                        self.sidebar.active_tab(),
                                        self.sidebar.selected_item(),
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::FilePreview(target) => widgets::file_preview::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                    self.task_view_drag_anchor_point
                        .zip(self.task_view_drag_current_point)
                        .or_else(|| {
                            self.task_view_drag_anchor.and_then(|anchor| {
                                self.task_view_drag_current.and_then(|current| {
                                    widgets::file_preview::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        target,
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_mission_control_preflight(
                        frame,
                        layout.chat,
                        &self.goal_mission_control,
                        self.mission_control_has_thread_target(),
                        &self.theme,
                    )
                }
            }
            if matches!(
                self.main_pane_view,
                MainPaneView::Task(SidebarItemTarget::GoalRun { .. })
            ) {
                if let MainPaneView::Task(SidebarItemTarget::GoalRun { goal_run_id, .. }) =
                    &self.main_pane_view
                {
                    widgets::goal_sidebar::render(
                        frame,
                        sidebar_area,
                        &self.tasks,
                        goal_run_id,
                        &self.goal_sidebar,
                        &self.theme,
                    );
                }
            } else {
                let sidebar_snapshot = self
                    .sidebar_snapshot
                    .as_ref()
                    .filter(|snapshot| {
                        widgets::sidebar::cached_snapshot_matches_render(
                            snapshot,
                            sidebar_area,
                            &self.chat,
                            &self.sidebar,
                            &self.tasks,
                            self.chat.active_thread_id(),
                        )
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        let snapshot = widgets::sidebar::build_cached_snapshot(
                            sidebar_area,
                            &self.chat,
                            &self.sidebar,
                            &self.tasks,
                            self.chat.active_thread_id(),
                        );
                        self.sidebar_snapshot = Some(snapshot.clone());
                        snapshot
                    });
                self.sidebar_body_area = widgets::sidebar::render_cached(
                    frame,
                    sidebar_area,
                    &self.chat,
                    &self.sidebar,
                    &self.theme,
                    self.focus == FocusArea::Sidebar,
                    &self.gateway_statuses,
                    &self.tier,
                    &self.recent_actions,
                    &sidebar_snapshot,
                )
                .map(|body| (sidebar_area, body));
            }
        } else {
            match &self.main_pane_view {
                MainPaneView::Conversation => self.render_conversation_panel(frame, layout.chat),
                MainPaneView::Collaboration => widgets::collaboration_view::render(
                    frame,
                    layout.chat,
                    &self.collaboration,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Workspace => widgets::workspace_board::render_with_scroll(
                    frame,
                    layout.chat,
                    &self.workspace,
                    &self.workspace_expanded_task_ids,
                    &self.workspace_board_scroll,
                    self.workspace_board_selection.as_ref(),
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Task(target) => {
                    let task_area = self.task_content_area().unwrap_or(layout.chat);
                    if let Some(return_area) = self.task_return_area_for(layout.chat) {
                        self.render_task_return_banner(frame, return_area);
                    }
                    if let SidebarItemTarget::GoalRun { goal_run_id, .. } = target {
                        widgets::goal_workspace::render_with_selection(
                            frame,
                            task_area,
                            &self.tasks,
                            goal_run_id,
                            &self.goal_workspace,
                            &self.theme,
                            self.tick_counter,
                            self.task_view_drag_anchor_point
                                .zip(self.task_view_drag_current_point)
                                .or_else(|| {
                                    self.task_view_drag_anchor.and_then(|anchor| {
                                        self.task_view_drag_current.and_then(|current| {
                                            widgets::goal_workspace::selection_points_from_mouse(
                                                task_area,
                                                &self.tasks,
                                                goal_run_id,
                                                &self.goal_workspace,
                                                anchor,
                                                current,
                                            )
                                        })
                                    })
                                }),
                        );
                    } else {
                        widgets::task_view::render(
                            frame,
                            task_area,
                            &self.tasks,
                            target,
                            &self.theme,
                            self.focus == FocusArea::Chat,
                            self.task_view_scroll,
                            self.task_show_live_todos,
                            self.task_show_timeline,
                            self.task_show_files,
                            self.tick_counter,
                            self.task_view_drag_anchor_point
                                .zip(self.task_view_drag_current_point)
                                .or_else(|| {
                                    self.task_view_drag_anchor.and_then(|anchor| {
                                        self.task_view_drag_current.and_then(|current| {
                                            widgets::task_view::selection_points_from_mouse(
                                                task_area,
                                                &self.tasks,
                                                target,
                                                &self.theme,
                                                self.task_view_scroll,
                                                self.task_show_live_todos,
                                                self.task_show_timeline,
                                                self.task_show_files,
                                                anchor,
                                                current,
                                            )
                                        })
                                    })
                                }),
                        );
                    }
                }
                MainPaneView::WorkContext => widgets::work_context_view::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                    self.work_context_drag_anchor_point
                        .zip(self.work_context_drag_current_point)
                        .or_else(|| {
                            self.work_context_drag_anchor.and_then(|anchor| {
                                self.work_context_drag_current.and_then(|current| {
                                    widgets::work_context_view::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        self.chat.active_thread_id(),
                                        self.sidebar.active_tab(),
                                        self.sidebar.selected_item(),
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::FilePreview(target) => widgets::file_preview::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                    self.task_view_drag_anchor_point
                        .zip(self.task_view_drag_current_point)
                        .or_else(|| {
                            self.task_view_drag_anchor.and_then(|anchor| {
                                self.task_view_drag_current.and_then(|current| {
                                    widgets::file_preview::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        target,
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_mission_control_preflight(
                        frame,
                        layout.chat,
                        &self.goal_mission_control,
                        self.mission_control_has_thread_target(),
                        &self.theme,
                    )
                }
            }
        }

        if anticipatory_height > 0 {
            widgets::anticipatory::render(frame, chunks[2], &self.anticipatory, &self.theme);
        }

        let footer_activity = self.footer_activity_text();
        let sticky_activity = footer_activity
            .as_deref()
            .map(|activity| self.sticky_thread_activity_text(activity));
        let sticky_owner_label = self.current_conversation_agent_profile().agent_label;

        if concierge_height > 0 {
            widgets::concierge::render(
                frame,
                chunks[3],
                &self.concierge,
                &self.chat,
                &self.theme,
                self.focus == FocusArea::Chat,
                sticky_activity.as_deref(),
                &sticky_owner_label,
            );
        }

        let thread_budget_notice = self.active_thread_budget_exceeded_notice();
        let footer_notice = thread_budget_notice
            .as_deref()
            .map(|notice| (notice, Style::default().fg(Color::Indexed(203))))
            .or_else(|| self.input_notice_style());
        widgets::footer::render_input(
            frame,
            chunks[4],
            &self.input,
            &self.theme,
            self.focus == FocusArea::Input,
            self.modal.top().is_some(),
            &self.attachments,
            self.tick_counter,
            None,
            footer_notice,
        );
        widgets::footer::render_status_bar(
            frame,
            chunks[5],
            &self.theme,
            self.connected,
            self.last_error.is_some(),
            self.error_active,
            self.tick_counter,
            self.error_tick,
            self.voice_recording,
            self.voice_player.is_some(),
            self.queued_prompts.len(),
            &self.status_line,
        );

        if let Some(modal_kind) = self.modal.top() {
            let overlay_area = match modal_kind {
                modal::ModalKind::Settings => render_helpers::centered_rect(90, 88, area),
                modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
                modal::ModalKind::GoalApprovalRejectPrompt => {
                    render_helpers::centered_rect(54, 32, area)
                }
                modal::ModalKind::OperatorQuestionOverlay => {
                    render_helpers::centered_rect(68, 34, area)
                }
                modal::ModalKind::OperatorProfileOnboarding => {
                    render_helpers::centered_fixed_rect(92, 21, area)
                }
                modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
                modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
                modal::ModalKind::PinnedBudgetExceeded => {
                    render_helpers::centered_rect(62, 36, area)
                }
                modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
                modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
                modal::ModalKind::Statistics => render_helpers::centered_rect(84, 84, area),
                modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
                modal::ModalKind::ThreadParticipants => render_helpers::centered_rect(76, 68, area),
                modal::ModalKind::ThreadHandoff => render_helpers::centered_rect(76, 68, area),
                modal::ModalKind::ThreadParticipantAgentPicker => {
                    render_helpers::centered_rect(48, 46, area)
                }
                modal::ModalKind::ThreadParticipantActions => {
                    render_helpers::centered_rect(44, 30, area)
                }
                modal::ModalKind::GatewayDefaultAgentPicker => {
                    render_helpers::centered_rect(48, 46, area)
                }
                modal::ModalKind::GatewayLink => render_helpers::centered_rect(64, 50, area),
                modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::WorkspacePicker => render_helpers::centered_rect(54, 42, area),
                modal::ModalKind::WorkspaceCreate => render_helpers::centered_rect(54, 28, area),
                modal::ModalKind::WorkspaceCreateTask => {
                    render_helpers::centered_rect(64, 46, area)
                }
                modal::ModalKind::WorkspaceReviewTask => {
                    render_helpers::centered_rect(58, 34, area)
                }
                modal::ModalKind::WorkspaceEditTask => render_helpers::centered_rect(64, 42, area),
                modal::ModalKind::WorkspaceTaskDetail => {
                    render_helpers::centered_rect(72, 58, area)
                }
                modal::ModalKind::WorkspaceTaskHistory => {
                    render_helpers::centered_rect(80, 58, area)
                }
                modal::ModalKind::WorkspaceActorPicker => {
                    render_helpers::centered_rect(46, 36, area)
                }
                modal::ModalKind::GoalStepActionPicker => {
                    render_helpers::centered_rect(46, 28, area)
                }
                modal::ModalKind::QueuedPrompts => render_helpers::centered_rect(72, 42, area),
                modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
                modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
                modal::ModalKind::OpenRouterProviderPicker => {
                    render_helpers::centered_rect(48, 48, area)
                }
                modal::ModalKind::RolePicker => render_helpers::centered_rect(42, 38, area),
                modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
                modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
                modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
                modal::ModalKind::ContextWindowEditor => {
                    render_helpers::centered_rect(46, 28, area)
                }
                modal::ModalKind::CustomModelEditor => render_helpers::centered_rect(46, 28, area),
                modal::ModalKind::Notifications => render_helpers::centered_rect(78, 78, area),
                modal::ModalKind::WhatsAppLink => render_helpers::centered_rect(70, 80, area),
                modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {
                    render_helpers::centered_rect(40, 35, area)
                }
                modal::ModalKind::Help => render_helpers::centered_rect(70, 80, area),
            };
            frame.render_widget(Clear, overlay_area);

            match modal_kind {
                modal::ModalKind::CommandPalette => {
                    widgets::command_palette::render(frame, overlay_area, &self.modal, &self.theme);
                }
                modal::ModalKind::ThreadPicker => {
                    widgets::thread_picker::render_for_workspace(
                        frame,
                        overlay_area,
                        &self.chat,
                        &self.modal,
                        &self.subagents,
                        &self.tasks,
                        &self.workspace,
                        &self.theme,
                        self.thread_picker_loading_tab(),
                        self.tick_counter,
                    );
                }
                modal::ModalKind::GoalPicker => {
                    widgets::goal_picker::render(
                        frame,
                        overlay_area,
                        &self.tasks,
                        &self.modal,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspacePicker => {
                    widgets::workspace_picker::render(
                        frame,
                        overlay_area,
                        &self.workspace,
                        &self.modal,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceCreate => {
                    let body = self
                        .pending_workspace_create_workspace_form
                        .as_ref()
                        .map(|form| {
                            crate::app::workspace_create_workspace_modal::workspace_create_modal_lines(
                                form,
                                &self.theme,
                            )
                        })
                        .unwrap_or_else(|| vec![ratatui::text::Line::raw("No workspace draft")]);
                    render_helpers::render_status_modal_lines(
                        frame,
                        overlay_area,
                        "NEW WORKSPACE",
                        body,
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceCreateTask => {
                    let body = self
                        .pending_workspace_create_form
                        .as_ref()
                        .map(|form| {
                            crate::app::workspace_create_modal::workspace_create_modal_lines_with_subagents(
                                form,
                                &self.subagents,
                                &self.theme,
                            )
                        })
                        .unwrap_or_else(|| vec![ratatui::text::Line::raw("No workspace task draft")]);
                    render_helpers::render_status_modal_lines(
                        frame,
                        overlay_area,
                        "WORKSPACE TASK",
                        body,
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceReviewTask => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "WORKSPACE REVIEW",
                        &self.workspace_review_modal_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceEditTask => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "WORKSPACE EDIT",
                        &self.workspace_edit_modal_body(),
                        self.workspace_edit_modal_scroll,
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceTaskDetail => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "WORKSPACE DETAIL",
                        &self.workspace_detail_modal_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceTaskHistory => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "WORKSPACE HISTORY",
                        &self.workspace_history_modal_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::WorkspaceActorPicker => {
                    let title = self
                        .pending_workspace_actor_picker
                        .as_ref()
                        .map(|pending| pending.mode.title())
                        .unwrap_or("WORKSPACE ACTOR");
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        title,
                        &self.workspace_actor_picker_body(),
                        self.workspace_actor_picker_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::GoalStepActionPicker => {
                    let step_context = self.selected_goal_step_context();
                    let action_items = self.goal_action_picker_items();
                    let action_labels: Vec<_> =
                        action_items.iter().map(|item| item.label()).collect();
                    let goal_title = self
                        .selected_goal_run()
                        .map(|run| run.title.as_str())
                        .or_else(|| {
                            self.goal_mission_control
                                .runtime_goal_run_id
                                .as_deref()
                                .and_then(|goal_run_id| self.tasks.goal_run_by_id(goal_run_id))
                                .map(|run| run.title.as_str())
                        });
                    render_helpers::render_goal_step_action_picker_modal(
                        frame,
                        overlay_area,
                        goal_title,
                        step_context
                            .as_ref()
                            .map(|(_, _, _, step)| step.title.as_str()),
                        &action_labels,
                        self.modal.picker_cursor(),
                        &self.theme,
                    );
                }
                modal::ModalKind::QueuedPrompts => {
                    widgets::queued_prompts::render(
                        frame,
                        overlay_area,
                        &self.queued_prompts,
                        self.modal.picker_cursor(),
                        self.queued_prompt_action,
                        self.tick_counter,
                        &self.theme,
                    );
                }
                modal::ModalKind::ApprovalOverlay => {
                    widgets::approval::render(frame, overlay_area, &self.approval, &self.theme);
                }
                modal::ModalKind::GoalApprovalRejectPrompt => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "GOAL APPROVAL REJECTED",
                        &self.goal_approval_reject_prompt_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::OperatorQuestionOverlay => {}
                modal::ModalKind::OperatorProfileOnboarding => {
                    let view = self.operator_profile_onboarding_view();
                    widgets::operator_profile_onboarding::render(
                        frame,
                        overlay_area,
                        &view,
                        self.modal.picker_cursor(),
                        &self.theme,
                    );
                }
                modal::ModalKind::ApprovalCenter => {
                    widgets::approval_center::render(
                        frame,
                        overlay_area,
                        &self.approval,
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                        &self.theme,
                    );
                }
                modal::ModalKind::ChatActionConfirm => {
                    let pending = self
                        .pending_chat_action_confirm
                        .as_ref()
                        .map(|pending| pending.modal_body());
                    render_helpers::render_chat_action_confirm_modal(
                        frame,
                        overlay_area,
                        pending.as_deref(),
                        self.chat_action_confirm_accept_selected,
                        &self.theme,
                    );
                }
                modal::ModalKind::PinnedBudgetExceeded => {
                    if let Some(payload) = self.pending_pinned_budget_exceeded.as_ref() {
                        render_helpers::render_pinned_budget_exceeded_modal(
                            frame,
                            overlay_area,
                            payload.current_pinned_chars,
                            payload.pinned_budget_chars,
                            payload.candidate_pinned_chars,
                            &self.theme,
                        );
                    }
                }
                modal::ModalKind::Settings => {
                    widgets::settings::render(
                        frame,
                        overlay_area,
                        &self.settings,
                        &self.config,
                        &self.modal,
                        &self.auth,
                        &self.subagents,
                        &self.concierge,
                        &self.tier,
                        &self.plugin_settings,
                        self.settings_modal_scroll,
                        &self.theme,
                    );
                }
                modal::ModalKind::ProviderPicker => {
                    widgets::provider_picker::render(
                        frame,
                        overlay_area,
                        &self.modal,
                        &self.config,
                        &self.auth,
                        match self.settings_picker_target {
                            Some(SettingsPickerTarget::AudioSttProvider) => {
                                Some(zorai_shared::providers::AudioToolKind::SpeechToText)
                            }
                            Some(SettingsPickerTarget::AudioTtsProvider) => {
                                Some(zorai_shared::providers::AudioToolKind::TextToSpeech)
                            }
                            _ => None,
                        },
                        matches!(
                            self.settings_picker_target,
                            Some(SettingsPickerTarget::EmbeddingProvider)
                        ),
                        &self.theme,
                    );
                }
                modal::ModalKind::ModelPicker => {
                    let (current_model, _custom_model_name) = self
                        .runtime_model_picker_current_selection()
                        .unwrap_or_else(|| self.model_picker_current_selection());
                    let models = if self
                        .goal_mission_control
                        .pending_runtime_edit
                        .as_ref()
                        .is_some_and(|edit| {
                            edit.field == goal_mission_control::RuntimeAssignmentEditField::Model
                        }) {
                        self.available_runtime_assignment_models()
                    } else {
                        self.available_model_picker_models()
                    };
                    widgets::model_picker::render_with_models(
                        frame,
                        overlay_area,
                        &self.modal,
                        &models,
                        &current_model,
                        &self.theme,
                    );
                }
                modal::ModalKind::OpenRouterProviderPicker => {
                    render_helpers::render_status_modal_lines(
                        frame,
                        overlay_area,
                        "OPENROUTER PROVIDERS",
                        openrouter_provider_picker_lines(self, &self.theme),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::RolePicker => {
                    let current_role = self.mission_control_role_picker_value();
                    widgets::role_picker::render(
                        frame,
                        overlay_area,
                        &self.modal,
                        current_role.as_str(),
                        &self.theme,
                    );
                }
                modal::ModalKind::OpenAIAuth => {
                    render_helpers::render_openai_auth_modal(
                        frame,
                        overlay_area,
                        self.openai_auth_url.as_deref(),
                        self.openai_auth_status_text.as_deref(),
                        &self.theme,
                    );
                }
                modal::ModalKind::ErrorViewer => {
                    render_helpers::render_error_modal(
                        frame,
                        overlay_area,
                        self.last_error.as_deref(),
                        &self.theme,
                    );
                }
                modal::ModalKind::EffortPicker => {
                    if let Some(current_effort) = self.mission_control_effort_picker_value() {
                        render_runtime_effort_picker(
                            frame,
                            overlay_area,
                            &self.modal,
                            Some(current_effort.as_str()),
                            &self.theme,
                        );
                    } else {
                        let current_effort = self.effort_picker_current_value();
                        render_runtime_effort_picker(
                            frame,
                            overlay_area,
                            &self.modal,
                            current_effort.as_deref(),
                            &self.theme,
                        );
                    }
                }
                modal::ModalKind::ContextWindowEditor => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "CONTEXT WINDOW",
                        &self.context_window_editor_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::CustomModelEditor => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "CUSTOM MODEL",
                        &self.custom_model_editor_body(),
                        0,
                        false,
                        &self.theme,
                    );
                }
                modal::ModalKind::Notifications => {
                    widgets::notifications::render(
                        frame,
                        overlay_area,
                        &self.notifications,
                        &self.theme,
                    );
                }
                modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {}
                modal::ModalKind::Status => {
                    let max_scroll = self.status_modal_max_scroll();
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "STATUS",
                        &self.status_modal_body(),
                        self.status_modal_scroll,
                        max_scroll > 0,
                        &self.theme,
                    );
                }
                modal::ModalKind::Statistics => {
                    widgets::statistics::render(
                        frame,
                        overlay_area,
                        self.statistics_modal_snapshot.as_ref(),
                        self.statistics_modal_loading,
                        self.statistics_modal_error.as_deref(),
                        self.statistics_modal_tab,
                        self.statistics_modal_window,
                        self.statistics_modal_scroll,
                        &self.theme,
                    );
                }
                modal::ModalKind::PromptViewer => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        self.prompt_modal_title(),
                        &self.prompt_modal_body(),
                        self.prompt_modal_scroll,
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::ThreadParticipants => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "THREAD PARTICIPANTS",
                        &self.thread_participants_modal_body(),
                        self.thread_participants_modal_cursor_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::ThreadHandoff => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "THREAD HANDOFF",
                        &self.thread_handoff_modal_body(),
                        self.thread_handoff_modal_cursor_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::ThreadParticipantAgentPicker => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "ADD PARTICIPANT",
                        &self.thread_participant_agent_picker_body(),
                        self.thread_participant_agent_picker_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::ThreadParticipantActions => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "PARTICIPANT ACTIONS",
                        &self.thread_participant_actions_body(),
                        self.thread_participant_actions_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::GatewayDefaultAgentPicker => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "DEFAULT GATEWAY AGENT",
                        &self.gateway_default_agent_picker_body(),
                        self.gateway_default_agent_picker_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::GatewayLink => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "CONTINUE ON GATEWAY",
                        &self.gateway_link_modal_body(),
                        self.gateway_link_modal_scroll(),
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::Help => {
                    render_helpers::render_help_modal(
                        frame,
                        overlay_area,
                        self.help_modal_scroll,
                        &self.theme,
                    );
                }
                modal::ModalKind::WhatsAppLink => {
                    widgets::whatsapp_link::render(frame, overlay_area, &self.modal, &self.theme);
                }
            }
        }
    }

    pub(super) fn current_modal_area(&self) -> Option<(modal::ModalKind, Rect)> {
        let kind = self.modal.top()?;
        let area = Rect::new(0, 0, self.width, self.height);
        let rect = match kind {
            modal::ModalKind::Settings => render_helpers::centered_rect(90, 88, area),
            modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
            modal::ModalKind::GoalApprovalRejectPrompt => {
                render_helpers::centered_rect(54, 32, area)
            }
            modal::ModalKind::OperatorQuestionOverlay => {
                render_helpers::centered_rect(68, 34, area)
            }
            modal::ModalKind::OperatorProfileOnboarding => {
                render_helpers::centered_fixed_rect(92, 21, area)
            }
            modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
            modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
            modal::ModalKind::PinnedBudgetExceeded => render_helpers::centered_rect(62, 36, area),
            modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
            modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
            modal::ModalKind::Statistics => render_helpers::centered_rect(84, 84, area),
            modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
            modal::ModalKind::ThreadParticipants => render_helpers::centered_rect(76, 68, area),
            modal::ModalKind::ThreadHandoff => render_helpers::centered_rect(76, 68, area),
            modal::ModalKind::ThreadParticipantAgentPicker => {
                render_helpers::centered_rect(48, 46, area)
            }
            modal::ModalKind::ThreadParticipantActions => {
                render_helpers::centered_rect(44, 30, area)
            }
            modal::ModalKind::GatewayDefaultAgentPicker => {
                render_helpers::centered_rect(48, 46, area)
            }
            modal::ModalKind::GatewayLink => render_helpers::centered_rect(64, 50, area),
            modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::WorkspacePicker => render_helpers::centered_rect(54, 42, area),
            modal::ModalKind::WorkspaceCreate => render_helpers::centered_rect(54, 28, area),
            modal::ModalKind::WorkspaceCreateTask => render_helpers::centered_rect(64, 46, area),
            modal::ModalKind::WorkspaceReviewTask => render_helpers::centered_rect(58, 34, area),
            modal::ModalKind::WorkspaceEditTask => render_helpers::centered_rect(64, 42, area),
            modal::ModalKind::WorkspaceTaskDetail => render_helpers::centered_rect(72, 58, area),
            modal::ModalKind::WorkspaceTaskHistory => render_helpers::centered_rect(80, 58, area),
            modal::ModalKind::WorkspaceActorPicker => render_helpers::centered_rect(46, 36, area),
            modal::ModalKind::GoalStepActionPicker => render_helpers::centered_rect(46, 28, area),
            modal::ModalKind::QueuedPrompts => render_helpers::centered_rect(72, 42, area),
            modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
            modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
            modal::ModalKind::OpenRouterProviderPicker => {
                render_helpers::centered_rect(48, 48, area)
            }
            modal::ModalKind::RolePicker => render_helpers::centered_rect(42, 38, area),
            modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
            modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
            modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
            modal::ModalKind::ContextWindowEditor => render_helpers::centered_rect(46, 28, area),
            modal::ModalKind::CustomModelEditor => render_helpers::centered_rect(46, 28, area),
            modal::ModalKind::Notifications => render_helpers::centered_rect(78, 78, area),
            modal::ModalKind::WhatsAppLink => render_helpers::centered_rect(70, 80, area),
            modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {
                render_helpers::centered_rect(40, 35, area)
            }
            modal::ModalKind::Help => render_helpers::centered_rect(70, 80, area),
        };
        Some((kind, rect))
    }
}
