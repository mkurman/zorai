use super::*;

#[path = "modal_handlers_enter.rs"]
mod enter;

fn is_settings_textarea_submit_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        code,
        KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n')
    ) || (code == KeyCode::Char('m') && modifiers.contains(KeyModifiers::CONTROL))
        || (code == KeyCode::Char('j') && modifiers.contains(KeyModifiers::CONTROL))
        || (code == KeyCode::Char('s') && modifiers.contains(KeyModifiers::CONTROL))
}

fn matches_shift_char(code: KeyCode, modifiers: KeyModifiers, expected: char) -> bool {
    modifiers.contains(KeyModifiers::SHIFT)
        && matches!(code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&expected))
}

impl TuiModel {
    pub(crate) fn queue_threads_for_picker_tab_refresh(
        &mut self,
        tab: &crate::state::modal::ThreadPickerTab,
    ) {
        self.queue_threads_for_picker_tab_refresh_at(tab, Instant::now());
    }

    fn queue_threads_for_picker_tab_refresh_at(
        &mut self,
        tab: &crate::state::modal::ThreadPickerTab,
        switched_at: Instant,
    ) {
        self.pending_thread_picker_refresh = Some(PendingThreadPickerRefresh {
            tab: tab.clone(),
            agent_filter: tab.agent_filter(),
            ready_at: switched_at + THREAD_PICKER_REFRESH_DEBOUNCE,
        });
    }

    pub(crate) fn pending_thread_picker_refresh_deadline(&self) -> Option<Instant> {
        self.pending_thread_picker_refresh
            .as_ref()
            .map(|pending| pending.ready_at)
    }

    pub(crate) fn thread_picker_loading_tab(&self) -> Option<crate::state::modal::ThreadPickerTab> {
        self.thread_picker_loading_tab.clone()
    }

    pub(crate) fn begin_custom_model_edit(&mut self) {
        enter::begin_custom_model_edit(self);
    }

    /// Dispatch a thread-list refresh keyed on the current picker tab. For
    /// agent-scoped tabs the daemon returns all matching threads; for tabs not
    /// scoped to an agent it returns the full thread summary list.
    pub(crate) fn refresh_threads_for_picker_tab(
        &mut self,
        tab: &crate::state::modal::ThreadPickerTab,
    ) {
        self.pending_thread_picker_refresh = None;
        self.thread_picker_loading_tab = Some(tab.clone());
        self.send_daemon_command(crate::state::DaemonCommand::RefreshThreadsForAgent {
            agent_filter: tab.agent_filter(),
        });
    }
    pub(super) fn handle_key_modal(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: modal::ModalKind,
    ) -> bool {
        if kind == modal::ModalKind::Notifications {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    if self.notifications.selected_row_action_index().is_some() {
                        let header_action = self.notifications.first_enabled_header_action();
                        self.notifications
                            .reduce(crate::state::NotificationsAction::FocusHeader(
                                header_action,
                            ));
                    } else {
                        let row_action = self.notifications.first_enabled_row_action_index();
                        self.notifications.reduce(
                            crate::state::NotificationsAction::FocusRowAction(row_action),
                        );
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if self.notifications.selected_row_action_index().is_some() {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepRowAction(-1));
                    } else {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepHeader(-1));
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if self.notifications.selected_row_action_index().is_some() {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepRowAction(1));
                    } else {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepHeader(1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(-1));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(1));
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(action_index) = self.notifications.selected_row_action_index() {
                        if let Some(id) = self
                            .notifications
                            .selected_item()
                            .map(|notification| notification.id.clone())
                        {
                            self.execute_notification_row_action(&id, action_index);
                        }
                    } else {
                        match self.notifications.selected_header_action() {
                            Some(crate::state::NotificationsHeaderAction::MarkAllRead) => {
                                self.mark_all_notifications_read();
                            }
                            Some(crate::state::NotificationsHeaderAction::ArchiveRead) => {
                                self.archive_read_notifications();
                            }
                            Some(crate::state::NotificationsHeaderAction::Close) => {
                                self.close_top_modal();
                            }
                            None => {
                                if let Some(id) = self
                                    .notifications
                                    .selected_item()
                                    .map(|notification| notification.id.clone())
                                {
                                    self.toggle_notification_expand(id);
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.toggle_notification_expand(id);
                    }
                }
                KeyCode::Char('r') if !modifiers.contains(KeyModifiers::SHIFT) => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.mark_notification_read(&id);
                    }
                }
                KeyCode::Char('a') if !modifiers.contains(KeyModifiers::SHIFT) => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.archive_notification(&id);
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.delete_notification(&id);
                    }
                }
                KeyCode::Char('o') => {
                    if let Some(notification_id) = self
                        .notifications
                        .selected_item()
                        .filter(|notification| !notification.actions.is_empty())
                        .map(|notification| notification.id.clone())
                    {
                        self.execute_notification_action(&notification_id, "", Some(0));
                    }
                }
                KeyCode::Char(ch) if matches_shift_char(KeyCode::Char(ch), modifiers, 'a') => {
                    self.archive_read_notifications();
                }
                KeyCode::Char(ch) if matches_shift_char(KeyCode::Char(ch), modifiers, 'r') => {
                    self.mark_all_notifications_read();
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::WorkspaceCreate {
            return self.handle_workspace_create_workspace_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::WorkspaceCreateTask {
            return self.handle_workspace_create_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::WorkspaceReviewTask {
            return self.handle_workspace_review_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::WorkspaceEditTask {
            return self.handle_workspace_edit_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::WorkspaceTaskDetail {
            return self.handle_workspace_detail_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::WorkspaceTaskHistory {
            return self.handle_workspace_history_modal_key(code, modifiers);
        }

        if kind == modal::ModalKind::ContextWindowEditor {
            match code {
                KeyCode::Enter => self.commit_active_thread_context_window_editor(),
                KeyCode::Esc => self.cancel_active_thread_context_window_editor(),
                KeyCode::Backspace => self.settings.reduce(SettingsAction::Backspace),
                KeyCode::Char(c) if c.is_ascii_digit() || matches!(c, '_' | ',' | ' ') => {
                    self.settings.reduce(SettingsAction::InsertChar(c));
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::CustomModelEditor {
            if matches!(
                code,
                KeyCode::Char('v' | 'V') if modifiers.contains(KeyModifiers::CONTROL)
            ) || code == KeyCode::Char('\u{16}')
                || (code == KeyCode::Insert && modifiers.contains(KeyModifiers::SHIFT))
            {
                self.paste_from_clipboard();
                return false;
            }
            match code {
                KeyCode::Enter => self.commit_active_thread_custom_model_editor(),
                KeyCode::Esc => self.cancel_active_thread_custom_model_editor(),
                KeyCode::Backspace => self.settings.reduce(SettingsAction::Backspace),
                KeyCode::Char(c)
                    if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.settings.reduce(SettingsAction::InsertChar(c));
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Settings {
            if matches!(
                code,
                KeyCode::Char('v' | 'V') if modifiers.contains(KeyModifiers::CONTROL)
            ) || code == KeyCode::Char('\u{16}')
                || (code == KeyCode::Insert && modifiers.contains(KeyModifiers::SHIFT))
            {
                self.paste_from_clipboard();
                return false;
            }

            if self.settings.is_editing() && self.settings.active_tab() == SettingsTab::Plugins {
                match code {
                    KeyCode::Enter => {
                        if self.handle_plugins_settings_key(code) {
                            return false;
                        }
                    }
                    KeyCode::Esc => {
                        self.settings.reduce(SettingsAction::CancelEdit);
                        return false;
                    }
                    KeyCode::Backspace => {
                        self.settings.reduce(SettingsAction::Backspace);
                        return false;
                    }
                    KeyCode::Char(c) => {
                        self.settings.reduce(SettingsAction::InsertChar(c));
                        return false;
                    }
                    _ => return false,
                }
            }

            if self.settings.is_editing() {
                let textarea_submit =
                    self.settings.is_textarea() && is_settings_textarea_submit_key(code, modifiers);
                if self.settings.is_textarea() {
                    match code {
                        KeyCode::Left => {
                            self.settings.reduce(SettingsAction::MoveCursorLeft);
                            return false;
                        }
                        KeyCode::Right => {
                            self.settings.reduce(SettingsAction::MoveCursorRight);
                            return false;
                        }
                        KeyCode::Up => {
                            self.settings.reduce(SettingsAction::MoveCursorUp);
                            return false;
                        }
                        KeyCode::Down => {
                            self.settings.reduce(SettingsAction::MoveCursorDown);
                            return false;
                        }
                        KeyCode::Home => {
                            self.settings.reduce(SettingsAction::MoveCursorHome);
                            return false;
                        }
                        KeyCode::End => {
                            self.settings.reduce(SettingsAction::MoveCursorEnd);
                            return false;
                        }
                        _ if textarea_submit => {}
                        KeyCode::Enter => {
                            self.settings.reduce(SettingsAction::InsertChar('\n'));
                            return false;
                        }
                        KeyCode::Esc => {
                            self.settings.reduce(SettingsAction::CancelEdit);
                            return false;
                        }
                        KeyCode::Backspace => {
                            self.settings.reduce(SettingsAction::Backspace);
                            return false;
                        }
                        KeyCode::Char(c) => {
                            self.settings.reduce(SettingsAction::InsertChar(c));
                            return false;
                        }
                        _ => return false,
                    }
                }

                match code {
                    KeyCode::Left => self.settings.reduce(SettingsAction::MoveCursorLeft),
                    KeyCode::Right => self.settings.reduce(SettingsAction::MoveCursorRight),
                    KeyCode::Up => self.settings.reduce(SettingsAction::MoveCursorUp),
                    KeyCode::Down => self.settings.reduce(SettingsAction::MoveCursorDown),
                    KeyCode::Home => self.settings.reduce(SettingsAction::MoveCursorHome),
                    KeyCode::End => self.settings.reduce(SettingsAction::MoveCursorEnd),
                    _ if textarea_submit || code == KeyCode::Enter => {
                        let field = self.settings.editing_field().unwrap_or("").to_string();
                        let value = self.settings.edit_buffer().to_string();
                        match field.as_str() {
                            "base_url" => self.config.base_url = value,
                            "api_key" => self.config.api_key = value,
                            "assistant_id" => self.config.assistant_id = value,
                            "huggingface_provider" => {
                                self.config.huggingface_provider =
                                    value.trim().trim_start_matches(':').to_ascii_lowercase();
                            }
                            "concierge_huggingface_provider" => {
                                self.concierge.huggingface_provider =
                                    value.trim().trim_start_matches(':').to_ascii_lowercase();
                            }
                            "concierge_base_url" => {
                                self.concierge.base_url = value.trim().to_string();
                            }
                            "compaction_weles_model" => {
                                self.config.compaction_weles_model = value.trim().to_string()
                            }
                            "compaction_custom_base_url" => {
                                self.config.compaction_custom_base_url = value
                            }
                            "compaction_custom_model" => {
                                self.config.compaction_custom_model = value.trim().to_string()
                            }
                            "compaction_custom_api_key" => {
                                self.config.compaction_custom_api_key = value
                            }
                            "compaction_custom_assistant_id" => {
                                self.config.compaction_custom_assistant_id = value
                            }
                            "custom_model_entry" => {
                                let trimmed = value.trim();
                                let (name, model_id) =
                                    if let Some((lhs, rhs)) = trimmed.split_once('|') {
                                        (lhs.trim(), rhs.trim())
                                    } else {
                                        ("", trimmed)
                                    };
                                if !model_id.is_empty() {
                                    self.config.model = model_id.to_string();
                                    let resolved_context_window =
                                        providers::resolve_context_window_for_provider_auth(
                                            &self.config.provider,
                                            &self.config.auth_source,
                                            &self.config.model,
                                            name,
                                        );
                                    if providers::model_uses_context_window_override(
                                        &self.config.provider,
                                        &self.config.auth_source,
                                        &self.config.model,
                                        name,
                                    ) {
                                        self.config.custom_model_name = if name.is_empty() {
                                            model_id.to_string()
                                        } else {
                                            name.to_string()
                                        };
                                        let next_context = resolved_context_window.unwrap_or(
                                            providers::default_custom_model_context_window(),
                                        );
                                        self.config.custom_context_window_tokens =
                                            Some(next_context);
                                        self.config.context_window_tokens = next_context;
                                    } else {
                                        self.config.custom_model_name = if name.is_empty() {
                                            String::new()
                                        } else {
                                            name.to_string()
                                        };
                                        self.config.custom_context_window_tokens = None;
                                        self.config.context_window_tokens =
                                            resolved_context_window.unwrap_or(256_000);
                                    }
                                    self.config.api_transport = self.provider_transport_snapshot(
                                        &self.config.provider,
                                        &self.config.auth_source,
                                        &self.config.model,
                                        &self.config.api_transport,
                                    );
                                }
                            }
                            "target_agent_model" => {
                                let model_id = value.trim();
                                if model_id.is_empty() {
                                    self.status_line = "Model ID cannot be empty".to_string();
                                    return false;
                                }
                                let Some(pending) = self.pending_target_agent_config.take() else {
                                    self.status_line =
                                        "No thread-owned agent target is active".to_string();
                                    return false;
                                };
                                if let Some(thread) = self.chat.active_thread_mut() {
                                    thread.profile_provider = Some(pending.provider_id.clone());
                                    thread.profile_model = Some(model_id.to_string());
                                    thread.runtime_provider = Some(pending.provider_id.clone());
                                    thread.runtime_model = Some(model_id.to_string());
                                }
                                crate::app::commands::apply_active_thread_provider_model_to_daemon(
                                    self,
                                    &pending.provider_id,
                                    model_id,
                                    pending.reasoning_effort.as_deref(),
                                );
                                self.status_line =
                                    format!("{} model: {}", pending.target_agent_name, model_id);
                            }
                            "mlflow_tracking_uri" => {
                                self.config.mlflow_tracking_uri = value.trim().to_string()
                            }
                            "mlflow_experiment_name" => {
                                self.config.mlflow_experiment_name = value.trim().to_string()
                            }
                            "mlflow_experiment_id" => {
                                self.config.mlflow_experiment_id = value.trim().to_string()
                            }
                            "gateway_prefix" => self.config.gateway_prefix = value,
                            "slack_token" => self.config.slack_token = value,
                            "slack_channel_filter" => self.config.slack_channel_filter = value,
                            "telegram_token" => self.config.telegram_token = value,
                            "telegram_allowed_chats" => self.config.telegram_allowed_chats = value,
                            "discord_token" => self.config.discord_token = value,
                            "discord_channel_filter" => self.config.discord_channel_filter = value,
                            "discord_allowed_users" => self.config.discord_allowed_users = value,
                            "whatsapp_allowed_contacts" => {
                                self.config.whatsapp_allowed_contacts = value
                            }
                            "whatsapp_token" => self.config.whatsapp_token = value,
                            "whatsapp_phone_id" => self.config.whatsapp_phone_id = value,
                            "duckduckgo_region" => self.config.duckduckgo_region = value,
                            "firecrawl_api_key" => self.config.firecrawl_api_key = value,
                            "exa_api_key" => self.config.exa_api_key = value,
                            "tavily_api_key" => self.config.tavily_api_key = value,
                            "search_max_results" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.search_max_results = n.clamp(1, 20);
                                }
                            }
                            "search_timeout" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.search_timeout_secs = n.clamp(3, 120);
                                }
                            }
                            "honcho_api_key" => self.config.honcho_api_key = value,
                            "honcho_base_url" => self.config.honcho_base_url = value,
                            "honcho_workspace_id" => self.config.honcho_workspace_id = value,
                            "honcho_editor_api_key" => {
                                if let Some(editor) = self.config.honcho_editor.as_mut() {
                                    editor.api_key = value;
                                }
                            }
                            "honcho_editor_base_url" => {
                                if let Some(editor) = self.config.honcho_editor.as_mut() {
                                    editor.base_url = value;
                                }
                            }
                            "honcho_editor_workspace_id" => {
                                if let Some(editor) = self.config.honcho_editor.as_mut() {
                                    editor.workspace_id = value;
                                }
                            }
                            "compliance_retention_days" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compliance_retention_days = n.clamp(1, 3650);
                                }
                            }
                            "tool_synthesis_max_generated_tools" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.tool_synthesis_max_generated_tools =
                                        n.clamp(1, 200);
                                }
                            }
                            "max_context_messages" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_context_messages = n.clamp(10, 500);
                                }
                            }
                            "tui_chat_history_page_size" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.tui_chat_history_page_size = n.clamp(20, 500);
                                }
                            }
                            "participant_observer_restore_window_hours" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.participant_observer_restore_window_hours =
                                        n.clamp(0, 24 * 30);
                                }
                            }
                            "max_tool_loops" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_tool_loops = n.clamp(0, 1000);
                                }
                            }
                            "max_retries" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_retries = n.clamp(0, 10);
                                }
                            }
                            "auto_refresh_interval_secs" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.auto_refresh_interval_secs = n.clamp(0, 86_400);
                                }
                            }
                            "retry_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.retry_delay_ms = n.clamp(100, 60000);
                                }
                            }
                            "message_loop_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.message_loop_delay_ms = n.clamp(0, 60000);
                                }
                            }
                            "tool_call_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.tool_call_delay_ms = n.clamp(0, 60000);
                                }
                            }
                            "llm_stream_chunk_timeout_secs" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.llm_stream_chunk_timeout_secs = n.clamp(30, 1800);
                                }
                            }
                            "context_window_tokens" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    let next = n.clamp(1000, 2_000_000);
                                    self.config.custom_context_window_tokens = Some(next);
                                    self.config.context_window_tokens = next;
                                    if self.active_thread_target_agent_config().is_none() {
                                        if let Some(thread) = self.chat.active_thread_mut() {
                                            thread.profile_context_window_tokens = Some(next);
                                        }
                                    }
                                }
                            }
                            "target_agent_context_window" => {
                                let Some(tokens) = Self::parse_context_window_tokens(&value) else {
                                    self.status_line =
                                        "Context window must be a number of tokens".to_string();
                                    return false;
                                };
                                self.apply_active_thread_context_window(tokens);
                                self.pending_target_agent_config = None;
                            }
                            "compact_threshold_pct" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compact_threshold_pct = n.clamp(50, 95);
                                }
                            }
                            "keep_recent_on_compact" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.keep_recent_on_compact = n.clamp(1, 50);
                                }
                            }
                            "bash_timeout_secs" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.bash_timeout_secs = n.clamp(5, 300);
                                }
                            }
                            "weles_max_concurrent_reviews" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.weles_max_concurrent_reviews = n.clamp(1, 16);
                                }
                            }
                            "compaction_custom_context_window_tokens" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compaction_custom_context_window_tokens =
                                        n.clamp(1000, 2_000_000);
                                }
                            }
                            "snapshot_max_count" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_count = n.clamp(0, 100);
                                }
                            }
                            "snapshot_max_size_mb" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_size_mb = n.clamp(1_024, 500_000);
                                }
                            }
                            "swarog_alias" | "rarog_alias" | "weles_alias" => {
                                let agent_id = field.trim_end_matches("_alias");
                                let alias = value.trim().to_string();
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: format!("/agent_aliases/{agent_id}"),
                                    value_json: serde_json::Value::String(alias.clone())
                                        .to_string(),
                                });
                                let raw = self
                                    .config
                                    .agent_config_raw
                                    .get_or_insert_with(|| serde_json::json!({}));
                                if !raw.get("agent_aliases").is_some_and(|v| v.is_object()) {
                                    raw["agent_aliases"] = serde_json::json!({});
                                }
                                raw["agent_aliases"][agent_id] = serde_json::Value::String(alias);
                                crate::agent_aliases::set_from_raw(raw);
                                self.send_daemon_command(DaemonCommand::GetConfig);
                            }
                            "system_prompt" => {
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    raw["system_prompt"] = serde_json::Value::String(value);
                                } else {
                                    let mut raw = serde_json::json!({});
                                    raw["system_prompt"] = serde_json::Value::String(value);
                                    self.config.agent_config_raw = Some(raw);
                                }
                            }
                            "subagent_name" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.name = value;
                                }
                            }
                            "subagent_role" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.role = value;
                                    editor.previous_role_preset = None;
                                }
                            }
                            "subagent_model" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.model = value.trim().to_string();
                                }
                            }
                            "subagent_base_url" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.base_url = value.trim().to_string();
                                }
                            }
                            "subagent_context_window_tokens" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.context_window_tokens = if value.trim().is_empty() {
                                        None
                                    } else {
                                        value
                                            .trim()
                                            .parse::<u32>()
                                            .ok()
                                            .map(|tokens| tokens.clamp(1_000, 2_000_000))
                                    };
                                }
                            }
                            "subagent_huggingface_provider" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.huggingface_provider =
                                        value.trim().trim_start_matches(':').to_ascii_lowercase();
                                }
                            }
                            "mission_control_assignment_model" => {
                                let model_id = value.trim().to_string();
                                let updated =
                                    self.update_selected_runtime_assignment(|assignment| {
                                        assignment.model = model_id.clone();
                                    });
                                self.goal_mission_control.clear_runtime_edit();
                                if !updated {
                                    self.status_line =
                                        "Mission Control roster is unavailable".to_string();
                                }
                            }
                            "mission_control_assignment_role" => {
                                let role_id = value.trim().to_string();
                                let updated =
                                    self.update_selected_runtime_assignment(|assignment| {
                                        assignment.role_id = role_id.clone();
                                    });
                                self.goal_mission_control.clear_runtime_edit();
                                if !updated {
                                    self.status_line =
                                        "Mission Control roster is unavailable".to_string();
                                }
                            }
                            "subagent_system_prompt" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.system_prompt = value;
                                }
                            }
                            "concierge_provider" => {
                                self.concierge.provider = if value.trim().is_empty() {
                                    None
                                } else {
                                    Some(value)
                                };
                                self.send_concierge_config();
                            }
                            "concierge_model" => {
                                self.concierge.model = if value.trim().is_empty() {
                                    None
                                } else {
                                    Some(value)
                                };
                                self.send_concierge_config();
                            }
                            "feat_heartbeat_cron" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/cron".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["cron"] = serde_json::Value::String(value);
                                }
                            }
                            "feat_heartbeat_quiet_start" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/quiet_start".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["quiet_start"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_heartbeat_quiet_end" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/quiet_end".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["quiet_end"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_decay_half_life_hours" => {
                                if let Ok(n) = value.parse::<f64>() {
                                    let clamped = n.clamp(1.0, 10000.0);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/consolidation/decay_half_life_hours"
                                            .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("consolidation").is_none() {
                                            raw["consolidation"] = serde_json::json!({});
                                        }
                                        raw["consolidation"]["decay_half_life_hours"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_heuristic_promotion_threshold" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 100);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/consolidation/heuristic_promotion_threshold"
                                            .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("consolidation").is_none() {
                                            raw["consolidation"] = serde_json::json!({});
                                        }
                                        raw["consolidation"]["heuristic_promotion_threshold"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_skill_community_preapprove_timeout_secs" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(5, 300);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path:
                                            "/skill_recommendation/community_preapprove_timeout_secs"
                                                .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("skill_recommendation").is_none() {
                                            raw["skill_recommendation"] = serde_json::json!({});
                                        }
                                        raw["skill_recommendation"]
                                            ["community_preapprove_timeout_secs"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_skill_suggest_global_enable_after_approvals" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 12);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path:
                                            "/skill_recommendation/suggest_global_enable_after_approvals"
                                                .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("skill_recommendation").is_none() {
                                            raw["skill_recommendation"] = serde_json::json!({});
                                        }
                                        raw["skill_recommendation"]
                                            ["suggest_global_enable_after_approvals"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_audio_stt_provider" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/audio/stt/provider".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("audio").is_none() {
                                        raw["audio"] = serde_json::json!({});
                                    }
                                    if raw["audio"].get("stt").is_none() {
                                        raw["audio"]["stt"] = serde_json::json!({});
                                    }
                                    raw["audio"]["stt"]["provider"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_audio_stt_model" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/audio/stt/model".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("audio").is_none() {
                                        raw["audio"] = serde_json::json!({});
                                    }
                                    if raw["audio"].get("stt").is_none() {
                                        raw["audio"]["stt"] = serde_json::json!({});
                                    }
                                    raw["audio"]["stt"]["model"] = serde_json::Value::String(value);
                                }
                            }
                            "feat_audio_tts_provider" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/audio/tts/provider".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("audio").is_none() {
                                        raw["audio"] = serde_json::json!({});
                                    }
                                    if raw["audio"].get("tts").is_none() {
                                        raw["audio"]["tts"] = serde_json::json!({});
                                    }
                                    raw["audio"]["tts"]["provider"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_audio_tts_model" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/audio/tts/model".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("audio").is_none() {
                                        raw["audio"] = serde_json::json!({});
                                    }
                                    if raw["audio"].get("tts").is_none() {
                                        raw["audio"]["tts"] = serde_json::json!({});
                                    }
                                    raw["audio"]["tts"]["model"] = serde_json::Value::String(value);
                                }
                            }
                            "feat_audio_tts_voice" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/audio/tts/voice".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("audio").is_none() {
                                        raw["audio"] = serde_json::json!({});
                                    }
                                    if raw["audio"].get("tts").is_none() {
                                        raw["audio"]["tts"] = serde_json::json!({});
                                    }
                                    raw["audio"]["tts"]["voice"] = serde_json::Value::String(value);
                                }
                            }
                            "feat_image_generation_provider" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/image/generation/provider".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("image").is_none() {
                                        raw["image"] = serde_json::json!({});
                                    }
                                    if raw["image"].get("generation").is_none() {
                                        raw["image"]["generation"] = serde_json::json!({});
                                    }
                                    raw["image"]["generation"]["provider"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_image_generation_model" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/image/generation/model".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("image").is_none() {
                                        raw["image"] = serde_json::json!({});
                                    }
                                    if raw["image"].get("generation").is_none() {
                                        raw["image"]["generation"] = serde_json::json!({});
                                    }
                                    raw["image"]["generation"]["model"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_embedding_model" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/semantic/embedding/model".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("semantic").is_none() {
                                        raw["semantic"] = serde_json::json!({});
                                    }
                                    if raw["semantic"].get("embedding").is_none() {
                                        raw["semantic"]["embedding"] = serde_json::json!({});
                                    }
                                    raw["semantic"]["embedding"]["model"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_embedding_dimensions" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 32768);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/semantic/embedding/dimensions".to_string(),
                                        value_json: clamped.to_string(),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("semantic").is_none() {
                                            raw["semantic"] = serde_json::json!({});
                                        }
                                        if raw["semantic"].get("embedding").is_none() {
                                            raw["semantic"]["embedding"] = serde_json::json!({});
                                        }
                                        raw["semantic"]["embedding"]["dimensions"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.settings.reduce(SettingsAction::ConfirmEdit);
                        if matches!(
                            field.as_str(),
                            "concierge_huggingface_provider" | "concierge_base_url"
                        ) {
                            self.send_concierge_config();
                        }
                        if !matches!(
                            field.as_str(),
                            "subagent_name"
                                | "swarog_alias"
                                | "rarog_alias"
                                | "weles_alias"
                                | "subagent_role"
                                | "subagent_context_window_tokens"
                                | "subagent_huggingface_provider"
                                | "subagent_base_url"
                                | "concierge_huggingface_provider"
                                | "concierge_base_url"
                                | "subagent_system_prompt"
                                | "target_agent_model"
                                | "target_agent_context_window"
                                | "honcho_editor_api_key"
                                | "honcho_editor_base_url"
                                | "honcho_editor_workspace_id"
                        ) {
                            self.sync_config_to_daemon();
                        }
                    }
                    KeyCode::Esc => self.settings.reduce(SettingsAction::CancelEdit),
                    KeyCode::Backspace => self.settings.reduce(SettingsAction::Backspace),
                    KeyCode::Char(c) => self.settings.reduce(SettingsAction::InsertChar(c)),
                    _ => {}
                }
                return false;
            }

            match self.settings.active_tab() {
                SettingsTab::Auth => {
                    if self.handle_auth_settings_key(code) {
                        self.sync_settings_modal_scroll_to_selection();
                        return false;
                    }
                }
                SettingsTab::Chat => {
                    if self.config.honcho_editor.is_some() {
                        self.handle_honcho_settings_key(code);
                        self.sync_settings_modal_scroll_to_selection();
                        return false;
                    }
                }
                SettingsTab::SubAgents => {
                    if self.handle_subagent_settings_key(code) {
                        self.sync_settings_modal_scroll_to_selection();
                        return false;
                    }
                }
                SettingsTab::Plugins => {
                    if self.handle_plugins_settings_key(code) {
                        self.sync_settings_modal_scroll_to_selection();
                        return false;
                    }
                }
                _ => {}
            }

            match code {
                KeyCode::Tab => {
                    let all = SettingsTab::all();
                    let current = self.settings.active_tab();
                    let next_idx = all
                        .iter()
                        .position(|&tab| tab == current)
                        .map(|i| (i + 1) % all.len())
                        .unwrap_or(0);
                    let next_tab = all[next_idx];
                    self.settings.reduce(SettingsAction::SwitchTab(next_tab));
                    self.settings_modal_scroll = 0;
                    if matches!(next_tab, SettingsTab::SubAgents) {
                        self.send_daemon_command(DaemonCommand::ListSubAgents);
                    } else if matches!(next_tab, SettingsTab::Concierge) {
                        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                    } else if matches!(next_tab, SettingsTab::Gateway) {
                        self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                    } else if matches!(next_tab, SettingsTab::Plugins) {
                        self.plugin_settings.list_mode = true;
                        self.send_daemon_command(DaemonCommand::PluginList);
                    } else if matches!(next_tab, SettingsTab::Mlflow) {
                        self.send_daemon_command(DaemonCommand::GetMlflowTracingStatus);
                    }
                    return false;
                }
                KeyCode::BackTab => {
                    let all = SettingsTab::all();
                    let current = self.settings.active_tab();
                    let prev_idx = all
                        .iter()
                        .position(|&tab| tab == current)
                        .map(|i| if i == 0 { all.len() - 1 } else { i - 1 })
                        .unwrap_or(0);
                    let prev_tab = all[prev_idx];
                    self.settings.reduce(SettingsAction::SwitchTab(prev_tab));
                    self.settings_modal_scroll = 0;
                    if matches!(prev_tab, SettingsTab::SubAgents) {
                        self.send_daemon_command(DaemonCommand::ListSubAgents);
                    } else if matches!(prev_tab, SettingsTab::Concierge) {
                        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                    } else if matches!(prev_tab, SettingsTab::Gateway) {
                        self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                    } else if matches!(prev_tab, SettingsTab::Plugins) {
                        self.plugin_settings.list_mode = true;
                        self.send_daemon_command(DaemonCommand::PluginList);
                    } else if matches!(prev_tab, SettingsTab::Mlflow) {
                        self.send_daemon_command(DaemonCommand::GetMlflowTracingStatus);
                    }
                    return false;
                }
                KeyCode::Down => {
                    let before = self.settings.field_cursor();
                    self.settings.navigate_field(1, self.settings_field_count());
                    if self.settings.field_cursor() == before {
                        self.set_settings_modal_scroll(
                            self.settings_modal_scroll.saturating_add(1),
                        );
                    } else {
                        self.sync_settings_modal_scroll_to_selection();
                    }
                    return false;
                }
                KeyCode::Up => {
                    let before = self.settings.field_cursor();
                    self.settings
                        .navigate_field(-1, self.settings_field_count());
                    if self.settings.field_cursor() == before {
                        self.set_settings_modal_scroll(
                            self.settings_modal_scroll.saturating_sub(1),
                        );
                    } else {
                        self.sync_settings_modal_scroll_to_selection();
                    }
                    return false;
                }
                KeyCode::PageDown => {
                    self.page_settings_modal_scroll(1);
                    return false;
                }
                KeyCode::PageUp => {
                    self.page_settings_modal_scroll(-1);
                    return false;
                }
                KeyCode::Home => {
                    self.set_settings_modal_scroll(0);
                    return false;
                }
                KeyCode::End => {
                    let max_scroll = self.settings_modal_max_scroll();
                    self.set_settings_modal_scroll(max_scroll);
                    return false;
                }
                KeyCode::Enter => {
                    if self.current_settings_field_name() == "enable_honcho_memory" {
                        self.activate_settings_field();
                    } else if self.settings_field_click_uses_toggle() {
                        self.toggle_settings_field();
                    } else {
                        self.activate_settings_field();
                    }
                    return false;
                }
                KeyCode::Char(' ') => {
                    self.toggle_settings_field();
                    return false;
                }
                _ => {}
            }
        }

        if kind == modal::ModalKind::QueuedPrompts {
            match code {
                KeyCode::Esc => self.close_top_modal(),
                KeyCode::Up | KeyCode::Char('k') => {
                    self.modal.reduce(modal::ModalAction::Navigate(-1));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.modal.reduce(modal::ModalAction::Navigate(1));
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.queued_prompt_action = self.queued_prompt_action.step(-1);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.queued_prompt_action = self.queued_prompt_action.step(1);
                }
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    self.queued_prompt_action = QueuedPromptAction::Expand;
                    self.execute_selected_queued_prompt_action();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.execute_selected_queued_prompt_action();
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ChatActionConfirm {
            match code {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.cancel_chat_action_confirm();
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Tab => {
                    self.chat_action_confirm_accept_selected =
                        !self.chat_action_confirm_accept_selected;
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::BackTab => {
                    self.chat_action_confirm_accept_selected =
                        !self.chat_action_confirm_accept_selected;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(apply_mode) = self
                        .pending_chat_action_confirm
                        .as_ref()
                        .and_then(|pending| match pending {
                            PendingConfirmAction::ReuseModelAsStt { model_id }
                                if model_id == "__mission_control__:next_turn" =>
                            {
                                Some(goal_mission_control::RuntimeAssignmentApplyMode::NextTurn)
                            }
                            PendingConfirmAction::ReuseModelAsStt { model_id }
                                if model_id == "__mission_control__:reassign_active_step" =>
                            {
                                Some(
                                    goal_mission_control::RuntimeAssignmentApplyMode::ReassignActiveStep,
                                )
                            }
                            PendingConfirmAction::ReuseModelAsStt { model_id }
                                if model_id == "__mission_control__:restart_active_step" =>
                            {
                                Some(
                                    goal_mission_control::RuntimeAssignmentApplyMode::RestartActiveStep,
                                )
                            }
                            _ => None,
                        })
                    {
                        self.close_chat_action_confirm();
                        let _ = self.confirm_runtime_assignment_change(apply_mode);
                    } else {
                        self.confirm_pending_chat_action();
                    }
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if self.chat_action_confirm_accept_selected {
                        if let Some(apply_mode) = self
                            .pending_chat_action_confirm
                            .as_ref()
                            .and_then(|pending| match pending {
                                PendingConfirmAction::ReuseModelAsStt { model_id }
                                    if model_id == "__mission_control__:next_turn" =>
                                {
                                    Some(goal_mission_control::RuntimeAssignmentApplyMode::NextTurn)
                                }
                                PendingConfirmAction::ReuseModelAsStt { model_id }
                                    if model_id == "__mission_control__:reassign_active_step" =>
                                {
                                    Some(
                                        goal_mission_control::RuntimeAssignmentApplyMode::ReassignActiveStep,
                                    )
                                }
                                PendingConfirmAction::ReuseModelAsStt { model_id }
                                    if model_id == "__mission_control__:restart_active_step" =>
                                {
                                    Some(
                                        goal_mission_control::RuntimeAssignmentApplyMode::RestartActiveStep,
                                    )
                                }
                                _ => None,
                            })
                        {
                            self.close_chat_action_confirm();
                            let _ = self.confirm_runtime_assignment_change(apply_mode);
                        } else {
                            self.confirm_pending_chat_action();
                        }
                    } else {
                        self.cancel_chat_action_confirm();
                    }
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::PinnedBudgetExceeded {
            match code {
                KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Char(' ')
                | KeyCode::Char('o')
                | KeyCode::Char('O') => {
                    self.close_pinned_budget_exceeded_modal();
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ApprovalOverlay {
            match code {
                KeyCode::Esc => {}
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.resolve_approval(ap.approval_id.clone(), "allow_once");
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.resolve_approval(ap.approval_id.clone(), "allow_session");
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.create_task_approval_rule(ap.approval_id.clone());
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.handle_reject_selected_approval(ap.approval_id.clone());
                    } else {
                        self.sync_contextual_approval_overlay();
                    }
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::GoalApprovalRejectPrompt {
            match code {
                KeyCode::Esc => self.close_top_modal(),
                KeyCode::Char('r') | KeyCode::Char('R') => self.rewrite_active_goal_after_reject(),
                KeyCode::Char('s') | KeyCode::Char('S') => self.stop_active_goal_after_reject(),
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ApprovalCenter {
            match code {
                KeyCode::Esc => self.close_top_modal(),
                KeyCode::Up | KeyCode::Char('k') => {
                    if self.approval.filter() == crate::state::ApprovalFilter::SavedRules {
                        if let Some(index) = self.approval.selected_rule_id().and_then(|rule_id| {
                            self.approval
                                .saved_rules()
                                .iter()
                                .position(|rule| rule.id == rule_id)
                        }) {
                            self.select_approval_center_rule_row(index.saturating_sub(1));
                        } else {
                            self.select_approval_center_rule_row(0);
                        }
                    } else {
                        self.step_approval_selection(-1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if self.approval.filter() == crate::state::ApprovalFilter::SavedRules {
                        let next = self
                            .approval
                            .selected_rule_id()
                            .and_then(|rule_id| {
                                self.approval
                                    .saved_rules()
                                    .iter()
                                    .position(|rule| rule.id == rule_id)
                            })
                            .map(|index| index.saturating_add(1))
                            .unwrap_or(0);
                        self.select_approval_center_rule_row(next);
                    } else {
                        self.step_approval_selection(1);
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    let next_filter = match self.approval.filter() {
                        crate::state::ApprovalFilter::AllPending => {
                            crate::state::ApprovalFilter::SavedRules
                        }
                        crate::state::ApprovalFilter::CurrentThread => {
                            crate::state::ApprovalFilter::AllPending
                        }
                        crate::state::ApprovalFilter::CurrentWorkspace => {
                            crate::state::ApprovalFilter::CurrentThread
                        }
                        crate::state::ApprovalFilter::SavedRules => {
                            crate::state::ApprovalFilter::CurrentWorkspace
                        }
                    };
                    self.approval
                        .reduce(crate::state::ApprovalAction::SetFilter(next_filter));
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let next_filter = match self.approval.filter() {
                        crate::state::ApprovalFilter::AllPending => {
                            crate::state::ApprovalFilter::CurrentThread
                        }
                        crate::state::ApprovalFilter::CurrentThread => {
                            crate::state::ApprovalFilter::CurrentWorkspace
                        }
                        crate::state::ApprovalFilter::CurrentWorkspace => {
                            crate::state::ApprovalFilter::SavedRules
                        }
                        crate::state::ApprovalFilter::SavedRules => {
                            crate::state::ApprovalFilter::AllPending
                        }
                    };
                    self.approval
                        .reduce(crate::state::ApprovalAction::SetFilter(next_filter));
                }
                KeyCode::Char('a') => {
                    if self.approval.filter() != crate::state::ApprovalFilter::SavedRules {
                        if let Some(ap) = self.approval.selected_visible_approval(
                            self.chat.active_thread_id(),
                            self.current_workspace_id(),
                        ) {
                            self.resolve_approval(ap.approval_id.clone(), "allow_once");
                        }
                    }
                }
                KeyCode::Char('s') => {
                    if self.approval.filter() != crate::state::ApprovalFilter::SavedRules {
                        if let Some(ap) = self.approval.selected_visible_approval(
                            self.chat.active_thread_id(),
                            self.current_workspace_id(),
                        ) {
                            self.resolve_approval(ap.approval_id.clone(), "allow_session");
                        }
                    }
                }
                KeyCode::Char('w') => {
                    if self.approval.filter() != crate::state::ApprovalFilter::SavedRules {
                        if let Some(ap) = self.approval.selected_visible_approval(
                            self.chat.active_thread_id(),
                            self.current_workspace_id(),
                        ) {
                            self.create_task_approval_rule(ap.approval_id.clone());
                        }
                    }
                }
                KeyCode::Char('d') => {
                    if self.approval.filter() != crate::state::ApprovalFilter::SavedRules {
                        if let Some(ap) = self.approval.selected_visible_approval(
                            self.chat.active_thread_id(),
                            self.current_workspace_id(),
                        ) {
                            self.handle_reject_selected_approval(ap.approval_id.clone());
                        }
                    }
                }
                KeyCode::Char('r') => {
                    if self.approval.filter() == crate::state::ApprovalFilter::SavedRules {
                        self.revoke_selected_task_approval_rule();
                    }
                }
                KeyCode::Enter => {}
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::OpenAIAuth {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if let Some(url) = self.openai_auth_url.as_deref() {
                        conversion::copy_to_clipboard(url);
                        self.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Enter => {
                    self.handle_modal_enter(kind);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::PromptViewer {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_prompt_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_prompt_modal_scroll(-1),
                KeyCode::PageDown => self.page_prompt_modal_scroll(1),
                KeyCode::PageUp => self.page_prompt_modal_scroll(-1),
                KeyCode::Home => self.set_prompt_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.prompt_modal_max_scroll();
                    self.set_prompt_modal_scroll(max_scroll);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ThreadParticipants || kind == modal::ModalKind::ThreadHandoff {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal.reduce(modal::ModalAction::Navigate(1))
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal.reduce(modal::ModalAction::Navigate(-1))
                }
                KeyCode::Enter => self.handle_modal_enter(kind),
                KeyCode::PageDown => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal.reduce(modal::ModalAction::Navigate(5))
                }
                KeyCode::PageUp => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal.reduce(modal::ModalAction::Navigate(-5))
                }
                KeyCode::Home => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal
                        .reduce(modal::ModalAction::Navigate(i32::MIN / 2))
                }
                KeyCode::End => {
                    if kind == modal::ModalKind::ThreadParticipants {
                        self.sync_thread_participants_modal_item_count();
                    } else {
                        self.sync_thread_handoff_modal_item_count();
                    }
                    self.modal
                        .reduce(modal::ModalAction::Navigate(i32::MAX / 2))
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Status {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_status_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_status_modal_scroll(-1),
                KeyCode::PageDown => self.page_status_modal_scroll(1),
                KeyCode::PageUp => self.page_status_modal_scroll(-1),
                KeyCode::Home => self.set_status_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.status_modal_max_scroll();
                    self.set_status_modal_scroll(max_scroll);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Statistics {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_statistics_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_statistics_modal_scroll(-1),
                KeyCode::PageDown => self.page_statistics_modal_scroll(1),
                KeyCode::PageUp => self.page_statistics_modal_scroll(-1),
                KeyCode::Home => self.set_statistics_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.statistics_modal_max_scroll();
                    self.set_statistics_modal_scroll(max_scroll);
                }
                KeyCode::Left | KeyCode::Char('h') => self.cycle_statistics_tab(-1),
                KeyCode::Right | KeyCode::Char('l') => self.cycle_statistics_tab(1),
                KeyCode::Char('[') => self.cycle_statistics_window(-1),
                KeyCode::Char(']') => self.cycle_statistics_window(1),
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Help {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_help_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_help_modal_scroll(-1),
                KeyCode::PageDown => self.page_help_modal_scroll(1),
                KeyCode::PageUp => self.page_help_modal_scroll(-1),
                KeyCode::Home => self.set_help_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.help_modal_max_scroll();
                    self.set_help_modal_scroll(max_scroll);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::WhatsAppLink {
            match code {
                KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                    self.close_top_modal();
                    self.status_line = "WhatsApp linking stopped".to_string();
                    return false;
                }
                _ => return false,
            }
        }

        if kind == modal::ModalKind::OperatorProfileOnboarding {
            self.sync_operator_profile_onboarding_item_count();

            match code {
                KeyCode::Esc => {
                    let _ = self.defer_operator_profile_question();
                    self.close_operator_profile_onboarding_modal();
                }
                KeyCode::Char('y') | KeyCode::Char('Y')
                    if self.is_current_operator_profile_bool_question() =>
                {
                    let _ = self.execute_operator_profile_onboarding_target(
                        widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::BoolChoice(true),
                    );
                }
                KeyCode::Char('n') | KeyCode::Char('N')
                    if self.is_current_operator_profile_bool_question() =>
                {
                    let _ = self.execute_operator_profile_onboarding_target(
                        widgets::operator_profile_onboarding::OperatorProfileOnboardingHitTarget::BoolChoice(false),
                    );
                }
                KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = self.skip_operator_profile_question();
                }
                KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = self.defer_operator_profile_question();
                }
                KeyCode::Char('r') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.retry_operator_profile_request();
                    self.status_line = "Retrying operator profile operation…".to_string();
                    self.show_input_notice(
                        "Retrying operator profile operation…",
                        InputNoticeKind::Success,
                        40,
                        true,
                    );
                }
                KeyCode::Backspace if !self.is_current_operator_profile_bool_question() => {
                    self.input.reduce(input::InputAction::Backspace);
                }
                KeyCode::Char(c)
                    if !self.is_current_operator_profile_bool_question()
                        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.input.reduce(input::InputAction::InsertChar(c));
                }
                _ => {}
            }
            if !matches!(
                code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Enter
                    | KeyCode::Char(' ')
            ) {
                return false;
            }
        }

        let is_searchable = matches!(
            kind,
            modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::WorkspacePicker
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenRouterProviderPicker
        );

        match code {
            KeyCode::Esc => {
                if matches!(
                    kind,
                    modal::ModalKind::ProviderPicker | modal::ModalKind::ModelPicker
                ) && matches!(
                    self.settings_picker_target,
                    Some(SettingsPickerTarget::BuiltinPersonaProvider)
                        | Some(SettingsPickerTarget::BuiltinPersonaModel)
                ) {
                    self.restore_builtin_persona_setup_config_snapshot();
                    self.pending_builtin_persona_setup = None;
                    self.settings_picker_target = None;
                } else if matches!(
                    self.settings_picker_target,
                    Some(SettingsPickerTarget::TargetAgentProvider)
                        | Some(SettingsPickerTarget::TargetAgentModel)
                        | Some(SettingsPickerTarget::TargetAgentReasoningEffort)
                ) {
                    self.pending_target_agent_config = None;
                    self.settings_picker_target = None;
                }
                self.close_top_modal();
                if kind != modal::ModalKind::CommandPalette {
                    self.input.reduce(input::InputAction::Clear);
                }
            }
            KeyCode::Left if kind == modal::ModalKind::ThreadPicker => {
                let previous = widgets::thread_picker::adjacent_thread_picker_tab_for_workspace(
                    &self.modal.thread_picker_tab(),
                    &self.chat,
                    &self.subagents,
                    &self.tasks,
                    &self.workspace,
                    -1,
                );
                self.modal.set_thread_picker_tab(previous.clone());
                self.sync_thread_picker_item_count();
                self.queue_threads_for_picker_tab_refresh(&previous);
            }
            KeyCode::Right if kind == modal::ModalKind::ThreadPicker => {
                let next = widgets::thread_picker::adjacent_thread_picker_tab_for_workspace(
                    &self.modal.thread_picker_tab(),
                    &self.chat,
                    &self.subagents,
                    &self.tasks,
                    &self.workspace,
                    1,
                );
                self.modal.set_thread_picker_tab(next.clone());
                self.sync_thread_picker_item_count();
                self.queue_threads_for_picker_tab_refresh(&next);
            }
            KeyCode::Char('R')
                if kind == modal::ModalKind::ThreadPicker
                    && modifiers.contains(KeyModifiers::SHIFT) =>
            {
                let tab = self.modal.thread_picker_tab();
                self.refresh_threads_for_picker_tab(&tab);
                self.status_line = "Refreshing threads…".to_string();
            }
            KeyCode::Delete if kind == modal::ModalKind::ThreadPicker => {
                if let Some(thread) = self.selected_thread_picker_thread() {
                    self.open_pending_action_confirm(PendingConfirmAction::DeleteThread {
                        thread_id: thread.id.clone(),
                        title: widgets::thread_picker::thread_display_title_for_workspace(
                            thread,
                            &self.tasks,
                            &self.workspace,
                        ),
                    });
                }
            }
            KeyCode::Delete if kind == modal::ModalKind::GoalPicker => {
                if let Some(run) = self.selected_goal_picker_run() {
                    self.open_pending_action_confirm(PendingConfirmAction::DeleteGoalRun {
                        goal_run_id: run.id.clone(),
                        title: run.title.clone(),
                    });
                }
            }
            KeyCode::Char('s')
                if modifiers.contains(KeyModifiers::CONTROL)
                    && kind == modal::ModalKind::ThreadPicker =>
            {
                if let Some(action) = self.selected_thread_picker_confirm_action() {
                    self.open_pending_action_confirm(action);
                }
            }
            KeyCode::Char('s')
                if modifiers.contains(KeyModifiers::CONTROL)
                    && kind == modal::ModalKind::GoalPicker =>
            {
                if let Some(action) = self.selected_goal_picker_toggle_action() {
                    self.open_pending_action_confirm(action);
                }
            }
            KeyCode::Char(ch)
                if matches_shift_char(KeyCode::Char(ch), modifiers, 'r')
                    && matches!(
                        kind,
                        modal::ModalKind::ThreadPicker
                            | modal::ModalKind::GoalPicker
                            | modal::ModalKind::WorkspacePicker
                    ) =>
            {
                self.send_daemon_command(DaemonCommand::Refresh);
                self.send_daemon_command(DaemonCommand::RefreshServices);
                if kind == modal::ModalKind::WorkspacePicker {
                    self.send_daemon_command(DaemonCommand::ListWorkspaceSettings);
                    self.status_line = "Refreshing workspaces".to_string();
                } else {
                    self.status_line = "Refreshing thread and goal lists".to_string();
                }
            }
            KeyCode::Down
                if matches!(
                    kind,
                    modal::ModalKind::CommandPalette | modal::ModalKind::OperatorProfileOnboarding
                ) =>
            {
                self.modal_navigate(1)
            }
            KeyCode::Up
                if matches!(
                    kind,
                    modal::ModalKind::CommandPalette | modal::ModalKind::OperatorProfileOnboarding
                ) =>
            {
                self.modal_navigate(-1)
            }
            KeyCode::Right | KeyCode::Tab
                if kind == modal::ModalKind::OperatorProfileOnboarding =>
            {
                self.modal_navigate(1)
            }
            KeyCode::Left | KeyCode::BackTab
                if kind == modal::ModalKind::OperatorProfileOnboarding =>
            {
                self.modal_navigate(-1)
            }
            KeyCode::Down => self.modal.reduce(modal::ModalAction::Navigate(1)),
            KeyCode::Up => self.modal.reduce(modal::ModalAction::Navigate(-1)),
            KeyCode::Enter => {
                self.handle_modal_enter(kind);
                if self.pending_quit {
                    self.pending_quit = false;
                    return true;
                }
            }
            KeyCode::Backspace if kind == modal::ModalKind::CommandPalette => {
                let mut query = self.modal.command_query().to_string();
                query.pop();
                self.modal.reduce(modal::ModalAction::SetQuery(query));
            }
            KeyCode::Backspace if is_searchable => {
                self.input.reduce(input::InputAction::Backspace);
                self.modal.reduce(modal::ModalAction::SetQuery(
                    self.input.buffer().to_string(),
                ));
                if kind == modal::ModalKind::ThreadPicker {
                    self.sync_thread_picker_item_count();
                } else if kind == modal::ModalKind::GoalPicker {
                    self.sync_goal_picker_item_count();
                } else if kind == modal::ModalKind::WorkspacePicker {
                    self.sync_workspace_picker_item_count();
                } else if kind == modal::ModalKind::ProviderPicker {
                    self.sync_provider_picker_item_count();
                } else if kind == modal::ModalKind::ModelPicker {
                    self.sync_model_picker_item_count();
                } else if kind == modal::ModalKind::OpenRouterProviderPicker {
                    self.sync_openrouter_provider_picker_item_count();
                }
            }
            KeyCode::Char(c)
                if is_searchable
                    && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if kind == modal::ModalKind::CommandPalette {
                    let mut query = self.modal.command_query().to_string();
                    query.push(c);
                    self.modal.reduce(modal::ModalAction::SetQuery(query));
                } else {
                    self.input.reduce(input::InputAction::InsertChar(c));
                    self.modal.reduce(modal::ModalAction::SetQuery(
                        self.input.buffer().to_string(),
                    ));
                    if kind == modal::ModalKind::ThreadPicker {
                        self.sync_thread_picker_item_count();
                    } else if kind == modal::ModalKind::GoalPicker {
                        self.sync_goal_picker_item_count();
                    } else if kind == modal::ModalKind::WorkspacePicker {
                        self.sync_workspace_picker_item_count();
                    } else if kind == modal::ModalKind::ProviderPicker {
                        self.sync_provider_picker_item_count();
                    } else if kind == modal::ModalKind::ModelPicker {
                        self.sync_model_picker_item_count();
                    } else if kind == modal::ModalKind::OpenRouterProviderPicker {
                        self.sync_openrouter_provider_picker_item_count();
                    }
                }
            }
            _ => {}
        }

        false
    }

    pub(super) fn handle_modal_enter(&mut self, kind: modal::ModalKind) {
        enter::handle_modal_enter(self, kind);
    }
}

#[cfg(test)]
#[path = "tests/modal_handlers.rs"]
mod tests;
