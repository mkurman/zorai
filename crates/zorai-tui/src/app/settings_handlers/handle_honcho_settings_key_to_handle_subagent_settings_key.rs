use super::*;
use crossterm::event::KeyCode;
impl TuiModel {
    pub(crate) fn handle_honcho_settings_key(&mut self, code: KeyCode) -> bool {
        let Some(editor) = self.config.honcho_editor.as_mut() else {
            return false;
        };

        match code {
            KeyCode::Esc => {
                self.close_honcho_editor();
                true
            }
            KeyCode::Up => {
                editor.field = editor.field.prev();
                true
            }
            KeyCode::Down | KeyCode::Tab => {
                editor.field = editor.field.next();
                true
            }
            KeyCode::BackTab => {
                editor.field = editor.field.prev();
                true
            }
            KeyCode::Char(' ') => {
                if editor.field == crate::state::config::HonchoEditorField::Enabled {
                    editor.enabled = !editor.enabled;
                    return true;
                }
                false
            }
            KeyCode::Enter => {
                match editor.field {
                    crate::state::config::HonchoEditorField::Enabled => {
                        editor.enabled = !editor.enabled;
                    }
                    crate::state::config::HonchoEditorField::ApiKey => {
                        let current = editor.api_key.clone();
                        self.settings
                            .start_editing("honcho_editor_api_key", &current);
                    }
                    crate::state::config::HonchoEditorField::BaseUrl => {
                        let current = editor.base_url.clone();
                        self.settings
                            .start_editing("honcho_editor_base_url", &current);
                    }
                    crate::state::config::HonchoEditorField::WorkspaceId => {
                        let current = editor.workspace_id.clone();
                        self.settings
                            .start_editing("honcho_editor_workspace_id", &current);
                    }
                    crate::state::config::HonchoEditorField::Save => {
                        self.commit_honcho_editor();
                    }
                    crate::state::config::HonchoEditorField::Cancel => {
                        self.close_honcho_editor();
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn handle_auth_settings_key(&mut self, code: KeyCode) -> bool {
        if self.auth.login_target.is_some() {
            match code {
                KeyCode::Esc => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::CancelLogin);
                    self.status_line = "Login cancelled".to_string();
                    true
                }
                KeyCode::Enter => {
                    self.confirm_auth_login();
                    true
                }
                KeyCode::Backspace => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::LoginKeyBackspace);
                    true
                }
                KeyCode::Left => {
                    self.auth.login_cursor = self.auth.login_cursor.saturating_sub(1);
                    true
                }
                KeyCode::Right => {
                    self.auth.login_cursor =
                        (self.auth.login_cursor + 1).min(self.auth.login_buffer.chars().count());
                    true
                }
                KeyCode::Char(c) => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::LoginKeyChar(c));
                    true
                }
                _ => true,
            }
        } else if self.auth.searching {
            match code {
                KeyCode::Esc => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::ClearSearch);
                    true
                }
                KeyCode::Enter | KeyCode::Down => {
                    self.auth.searching = false;
                    true
                }
                KeyCode::Backspace => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::SearchBackspace);
                    true
                }
                KeyCode::Char(c) => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::SearchChar(c));
                    true
                }
                KeyCode::Tab | KeyCode::BackTab => false,
                _ => true,
            }
        } else {
            match code {
                KeyCode::Char('/') => {
                    self.auth.searching = true;
                    self.auth.actions_focused = false;
                    true
                }
                KeyCode::Esc if !self.auth.query.is_empty() => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::ClearSearch);
                    true
                }
                KeyCode::Up => {
                    self.auth.selected = self.auth.selected.saturating_sub(1);
                    true
                }
                KeyCode::Down => {
                    if self.auth.selected + 1 < self.auth.visible_entries().len() {
                        self.auth.selected += 1;
                    }
                    true
                }
                KeyCode::Left => {
                    if self.auth.actions_focused && self.auth.action_cursor > 0 {
                        self.auth.action_cursor -= 1;
                    } else {
                        self.auth.actions_focused = false;
                    }
                    true
                }
                KeyCode::Right => {
                    if self.auth.actions_focused {
                        self.auth.action_cursor = (self.auth.action_cursor + 1).min(1);
                    } else {
                        self.auth.actions_focused = true;
                    }
                    true
                }
                KeyCode::Enter => {
                    if !self.auth.actions_focused {
                        self.auth.actions_focused = true;
                    } else {
                        self.run_auth_tab_action();
                    }
                    true
                }
                KeyCode::Char(' ') => {
                    self.run_auth_tab_action();
                    true
                }
                KeyCode::Char('h') => {
                    if self.auth.actions_focused && self.auth.action_cursor > 0 {
                        self.auth.action_cursor -= 1;
                    } else {
                        self.auth.actions_focused = false;
                    }
                    true
                }
                KeyCode::Char('l') => {
                    if self.auth.actions_focused {
                        self.auth.action_cursor = (self.auth.action_cursor + 1).min(1);
                    } else {
                        self.auth.actions_focused = true;
                    }
                    true
                }
                _ => false,
            }
        }
    }

    pub(crate) fn handle_subagent_settings_key(&mut self, code: KeyCode) -> bool {
        if self.subagents.editor.is_some() {
            match code {
                KeyCode::Esc => {
                    self.close_subagent_editor();
                    true
                }
                KeyCode::Up => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.prev_for_provider(&editor.provider);
                    }
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.next_for_provider(&editor.provider);
                    }
                    true
                }
                KeyCode::BackTab => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.prev_for_provider(&editor.provider);
                    }
                    true
                }
                KeyCode::Left => false,
                KeyCode::Right => false,
                KeyCode::Enter => {
                    let Some(field) = self.subagents.editor.as_ref().map(|editor| editor.field)
                    else {
                        return true;
                    };
                    match field {
                        crate::state::subagents::SubAgentEditorField::Name => {
                            if self
                                .subagents
                                .editor
                                .as_ref()
                                .is_some_and(|editor| editor.id.as_deref() == Some("weles_builtin"))
                            {
                                let current = self
                                    .config
                                    .agent_config_raw
                                    .as_ref()
                                    .and_then(|raw| raw.get("agent_aliases"))
                                    .and_then(|aliases| aliases.get("weles"))
                                    .and_then(|value| value.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                self.settings.start_editing("weles_alias", &current);
                                return true;
                            }
                            if self
                                .subagents
                                .editor
                                .as_ref()
                                .is_some_and(|editor| !editor.identity_is_mutable())
                            {
                                self.status_line =
                                    "This sub-agent identity cannot be changed".to_string();
                                return true;
                            }
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.name.clone())
                                .unwrap_or_default();
                            self.settings.start_editing("subagent_name", &current);
                        }
                        crate::state::subagents::SubAgentEditorField::Provider => {
                            self.open_subagent_provider_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::Model => {
                            self.open_subagent_model_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::BaseUrl => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.base_url.clone())
                                .unwrap_or_default();
                            self.settings.start_editing("subagent_base_url", &current);
                            self.status_line =
                                "Enter base URL override; empty uses the provider default"
                                    .to_string();
                        }
                        crate::state::subagents::SubAgentEditorField::ContextWindowTokens => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .and_then(|editor| editor.context_window_tokens)
                                .map(|tokens| tokens.to_string())
                                .unwrap_or_default();
                            self.settings
                                .start_editing("subagent_context_window_tokens", &current);
                            self.status_line =
                                "Enter sub-agent context window tokens; empty uses auto"
                                    .to_string();
                        }
                        crate::state::subagents::SubAgentEditorField::OpenRouterProviderOrder => {
                            self.open_subagent_openrouter_provider_picker(
                                SettingsPickerTarget::SubAgentOpenRouterPreferredProviders,
                            );
                        }
                        crate::state::subagents::SubAgentEditorField::OpenRouterProviderIgnore => {
                            self.open_subagent_openrouter_provider_picker(
                                SettingsPickerTarget::SubAgentOpenRouterExcludedProviders,
                            );
                        }
                        crate::state::subagents::SubAgentEditorField::OpenRouterAllowFallbacks => {
                            if let Some(editor) = self.subagents.editor.as_mut() {
                                editor.openrouter_allow_fallbacks =
                                    !editor.openrouter_allow_fallbacks;
                            }
                        }
                        crate::state::subagents::SubAgentEditorField::HuggingFaceProvider => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.huggingface_provider.clone())
                                .unwrap_or_default();
                            self.settings
                                .start_editing("subagent_huggingface_provider", &current);
                            self.status_line =
                                "Enter HF route: fastest, cheapest, preferred, or provider slug"
                                    .to_string();
                        }
                        crate::state::subagents::SubAgentEditorField::ReasoningEffort => {
                            self.open_subagent_effort_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::ApiTransport => {
                            if let Some(editor) = self.subagents.editor.as_mut() {
                                let supported =
                                    crate::providers::supported_transports_for(&editor.provider);
                                let mut options: Vec<&str> = vec![""];
                                options.extend_from_slice(supported);
                                let current = editor.api_transport.clone().unwrap_or_default();
                                let current_idx = options
                                    .iter()
                                    .position(|transport| *transport == current)
                                    .unwrap_or(0);
                                let next_idx = (current_idx + 1) % options.len().max(1);
                                let next = options.get(next_idx).copied().unwrap_or("");
                                editor.api_transport = if next.is_empty() {
                                    None
                                } else {
                                    Some(next.to_string())
                                };
                            }
                        }
                        crate::state::subagents::SubAgentEditorField::ClaudePermissionMode => {
                            if let Some(editor) = self.subagents.editor.as_mut() {
                                let options =
                                    crate::state::subagents::CLAUDE_PERMISSION_MODE_OPTIONS;
                                let current =
                                    editor.claude_permission_mode.clone().unwrap_or_default();
                                let current_idx = options
                                    .iter()
                                    .position(|mode| *mode == current)
                                    .unwrap_or(0);
                                let next_idx = (current_idx + 1) % options.len().max(1);
                                let next = options.get(next_idx).copied().unwrap_or("");
                                editor.claude_permission_mode = if next.is_empty() {
                                    None
                                } else {
                                    Some(next.to_string())
                                };
                            }
                        }
                        crate::state::subagents::SubAgentEditorField::Role => {
                            self.open_subagent_role_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::SystemPrompt => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.system_prompt.clone())
                                .unwrap_or_default();
                            self.settings
                                .start_editing("subagent_system_prompt", &current);
                        }
                        crate::state::subagents::SubAgentEditorField::Save => {
                            self.commit_subagent_editor();
                        }
                        crate::state::subagents::SubAgentEditorField::Cancel => {
                            self.close_subagent_editor();
                        }
                    }
                    true
                }
                KeyCode::Char('s') => {
                    self.commit_subagent_editor();
                    true
                }
                _ => false,
            }
        } else {
            match code {
                KeyCode::Up => {
                    self.subagents.selected = self.subagents.selected.saturating_sub(1);
                    true
                }
                KeyCode::Down => {
                    if self.subagents.selected + 1 < self.subagents.entries.len() {
                        self.subagents.selected += 1;
                    }
                    true
                }
                KeyCode::Left => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            self.subagents.action_cursor.max(1).saturating_sub(1).max(1);
                    }
                    true
                }
                KeyCode::Right => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            (self.subagents.action_cursor.max(1) + 1).min(3);
                    }
                    true
                }
                KeyCode::Enter => {
                    if self.subagents.entries.is_empty() {
                        self.subagents.action_cursor = 0;
                        self.run_subagent_action();
                    } else if self.subagents.actions_focused {
                        self.run_subagent_action();
                    } else {
                        self.subagents.action_cursor = 1;
                        self.subagents.actions_focused = true;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char(' ') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 3;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char('a') => {
                    self.subagents.action_cursor = 0;
                    self.run_subagent_action();
                    true
                }
                KeyCode::Char('e') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 1;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 2;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char('h') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            self.subagents.action_cursor.max(1).saturating_sub(1).max(1);
                    }
                    true
                }
                KeyCode::Char('l') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            (self.subagents.action_cursor.max(1) + 1).min(3);
                    }
                    true
                }
                _ => false,
            }
        }
    }
}
