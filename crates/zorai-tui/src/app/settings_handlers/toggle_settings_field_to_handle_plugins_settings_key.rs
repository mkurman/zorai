use super::*;
use crossterm::event::KeyCode;

fn clamp_to_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    text[..cursor]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    let cursor = clamp_to_char_boundary(text, cursor);
    if cursor >= text.len() {
        text.len()
    } else {
        cursor + text[cursor..].chars().next().map_or(0, char::len_utf8)
    }
}

impl TuiModel {
    pub(crate) fn toggle_settings_field(&mut self) {
        let field = self.current_settings_field_name().to_string();
        match field.as_str() {
            "managed_sandbox_enabled" => {
                self.config.managed_sandbox_enabled = !self.config.managed_sandbox_enabled;
                self.sync_config_to_daemon();
            }
            "managed_security_level" => {
                let levels = ["highest", "moderate", "lowest", "yolo"];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.config.managed_security_level)
                    .unwrap_or(2);
                self.config.managed_security_level =
                    levels[(current_idx + 1) % levels.len()].to_string();
                self.sync_config_to_daemon();
            }
            "mlflow_enabled" => {
                self.config.mlflow_enabled = !self.config.mlflow_enabled;
                self.sync_config_to_daemon();
            }
            "mlflow_visible_operator" => {
                self.config.mlflow_visible_operator = !self.config.mlflow_visible_operator;
                self.sync_config_to_daemon();
            }
            "mlflow_gateway" => {
                self.config.mlflow_gateway = !self.config.mlflow_gateway;
                self.sync_config_to_daemon();
            }
            "mlflow_goal_task" => {
                self.config.mlflow_goal_task = !self.config.mlflow_goal_task;
                self.sync_config_to_daemon();
            }
            "mlflow_subagent" => {
                self.config.mlflow_subagent = !self.config.mlflow_subagent;
                self.sync_config_to_daemon();
            }
            "mlflow_concierge" => {
                self.config.mlflow_concierge = !self.config.mlflow_concierge;
                self.sync_config_to_daemon();
            }
            "mlflow_heartbeat_autonomous" => {
                self.config.mlflow_heartbeat_autonomous = !self.config.mlflow_heartbeat_autonomous;
                self.sync_config_to_daemon();
            }
            "mlflow_capture_mode" => {
                self.config.mlflow_capture_mode = match self.config.mlflow_capture_mode.as_str() {
                    "metadata" => "guarded",
                    "guarded" => "full",
                    _ => "metadata",
                }
                .to_string();
                self.sync_config_to_daemon();
            }
            "mlflow_test_connection" | "mlflow_send_test_trace" => {
                self.activate_settings_field();
            }
            "gateway_enabled" => {
                self.config.gateway_enabled = !self.config.gateway_enabled;
                self.sync_config_to_daemon();
            }
            "web_search_enabled" => {
                self.config.tool_web_search = !self.config.tool_web_search;
                self.sync_config_to_daemon();
            }
            "enable_streaming" => {
                self.config.enable_streaming = !self.config.enable_streaming;
                self.sync_config_to_daemon();
            }
            "auto_retry" => {
                self.config.auto_retry = !self.config.auto_retry;
                self.sync_config_to_daemon();
            }
            "workspace_repo_monitor_enabled" => {
                let requested_enabled = !self.config.workspace_repo_monitor_enabled;
                self.sync_workspace_repo_monitor_to_daemon(requested_enabled);
            }
            "enable_conversation_memory" => {
                self.config.enable_conversation_memory = !self.config.enable_conversation_memory;
                self.sync_config_to_daemon();
            }
            "enable_honcho_memory" => {
                self.config.enable_honcho_memory = !self.config.enable_honcho_memory;
                self.sync_config_to_daemon();
            }
            "anticipatory_enabled" => {
                self.config.anticipatory_enabled = !self.config.anticipatory_enabled;
                self.sync_config_to_daemon();
            }
            "anticipatory_morning_brief" => {
                self.config.anticipatory_morning_brief = !self.config.anticipatory_morning_brief;
                self.sync_config_to_daemon();
            }
            "anticipatory_predictive_hydration" => {
                self.config.anticipatory_predictive_hydration =
                    !self.config.anticipatory_predictive_hydration;
                self.sync_config_to_daemon();
            }
            "anticipatory_stuck_detection" => {
                self.config.anticipatory_stuck_detection =
                    !self.config.anticipatory_stuck_detection;
                self.sync_config_to_daemon();
            }
            "operator_model_enabled" => {
                self.config.operator_model_enabled = !self.config.operator_model_enabled;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_message_statistics" => {
                self.config.operator_model_allow_message_statistics =
                    !self.config.operator_model_allow_message_statistics;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_approval_learning" => {
                self.config.operator_model_allow_approval_learning =
                    !self.config.operator_model_allow_approval_learning;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_attention_tracking" => {
                self.config.operator_model_allow_attention_tracking =
                    !self.config.operator_model_allow_attention_tracking;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_implicit_feedback" => {
                self.config.operator_model_allow_implicit_feedback =
                    !self.config.operator_model_allow_implicit_feedback;
                self.sync_config_to_daemon();
            }
            "collaboration_enabled" => {
                self.config.collaboration_enabled = !self.config.collaboration_enabled;
                self.sync_config_to_daemon();
            }
            "compliance_sign_all_events" => {
                self.config.compliance_sign_all_events = !self.config.compliance_sign_all_events;
                self.sync_config_to_daemon();
            }
            "tool_synthesis_enabled" => {
                self.config.tool_synthesis_enabled = !self.config.tool_synthesis_enabled;
                self.sync_config_to_daemon();
            }
            "tool_synthesis_require_activation" => {
                self.config.tool_synthesis_require_activation =
                    !self.config.tool_synthesis_require_activation;
                self.sync_config_to_daemon();
            }
            "auto_compact_context" => {
                self.config.auto_compact_context = !self.config.auto_compact_context;
                self.sync_config_to_daemon();
            }
            "compaction_strategy"
            | "compaction_weles_provider"
            | "compaction_weles_reasoning_effort"
            | "compaction_weles_api_transport"
            | "compaction_custom_provider"
            | "compaction_custom_auth_source"
            | "compaction_custom_api_transport"
            | "compaction_custom_reasoning_effort" => {
                self.activate_settings_field();
            }
            "snapshot_auto_cleanup" => {
                self.config.snapshot_auto_cleanup = !self.config.snapshot_auto_cleanup;
                self.sync_config_to_daemon();
            }
            "feat_tier_override" => {
                self.activate_settings_field();
            }
            "desktop_notifications" => {
                let enabled = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("desktop_notifications"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.set_raw_config_item(
                    &["desktop_notifications"],
                    serde_json::Value::Bool(!enabled),
                );
            }
            "feat_audio_tts_auto_speak" => {
                let enabled = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("audio"))
                    .and_then(|a| a.get("tts"))
                    .and_then(|t| t.get("auto_speak"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.set_raw_config_item(
                    &["audio", "tts", "auto_speak"],
                    serde_json::Value::Bool(!enabled),
                );
            }
            "feat_heartbeat_enabled" => {
                let enabled = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat_enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.set_raw_config_item(&["heartbeat_enabled"], serde_json::Value::Bool(!enabled));
            }
            "feat_check_stale_todos"
            | "feat_check_stuck_goals"
            | "feat_check_unreplied_messages"
            | "feat_check_repo_changes"
            | "feat_check_plugin_auth" => {
                let key = match field.as_str() {
                    "feat_check_stale_todos" => "stale_todos_enabled",
                    "feat_check_stuck_goals" => "stuck_goals_enabled",
                    "feat_check_unreplied_messages" => "unreplied_messages_enabled",
                    "feat_check_repo_changes" => "repo_changes_enabled",
                    "feat_check_plugin_auth" => "plugin_auth_enabled",
                    _ => return,
                };
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat_checks"))
                    .and_then(|h| h.get(key))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.set_raw_config_item(
                    &["heartbeat_checks", key],
                    serde_json::Value::Bool(!current),
                );
            }
            "feat_consolidation_enabled" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/consolidation/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("consolidation").is_none() {
                        raw["consolidation"] = serde_json::json!({});
                    }
                    raw["consolidation"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "feat_skill_recommendation_enabled" | "feat_skill_background_community_search" => {
                let key = match field.as_str() {
                    "feat_skill_recommendation_enabled" => "enabled",
                    "feat_skill_background_community_search" => "background_community_search",
                    _ => return,
                };
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("skill_recommendation"))
                    .and_then(|s| s.get(key))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/skill_recommendation/{key}"),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("skill_recommendation").is_none() {
                        raw["skill_recommendation"] = serde_json::json!({});
                    }
                    raw["skill_recommendation"][key] = serde_json::Value::Bool(next);
                }
            }
            "feat_audio_stt_enabled" => {
                let current = self.config.audio_stt_enabled();
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/audio/stt/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("audio").is_none() {
                        raw["audio"] = serde_json::json!({});
                    }
                    if raw["audio"].get("stt").is_none() {
                        raw["audio"]["stt"] = serde_json::json!({});
                    }
                    raw["audio"]["stt"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "feat_audio_tts_enabled" => {
                let current = self.config.audio_tts_enabled();
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/audio/tts/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("audio").is_none() {
                        raw["audio"] = serde_json::json!({});
                    }
                    if raw["audio"].get("tts").is_none() {
                        raw["audio"]["tts"] = serde_json::json!({});
                    }
                    raw["audio"]["tts"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "feat_embedding_enabled" => {
                let current = self.config.semantic_embedding_enabled();
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/semantic/embedding/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("semantic").is_none() {
                        raw["semantic"] = serde_json::json!({});
                    }
                    if raw["semantic"].get("embedding").is_none() {
                        raw["semantic"]["embedding"] = serde_json::json!({});
                    }
                    raw["semantic"]["embedding"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "feat_auto_thread_title" => {
                self.activate_settings_field();
            }
            "whatsapp_link_device" => {
                self.activate_settings_field();
            }
            "concierge_enabled" => {
                self.concierge.enabled = !self.concierge.enabled;
                self.send_concierge_config();
            }
            "concierge_provider" => {
                self.concierge.provider = None;
                self.send_concierge_config();
            }
            "concierge_model" => {
                self.concierge.model = None;
                self.send_concierge_config();
            }
            field if crate::state::settings::is_tool_toggle_field(field) => {
                let tool_name = field.strip_prefix("tool_").unwrap_or(field).to_string();
                self.config
                    .reduce(config::ConfigAction::ToggleTool(tool_name));
                self.sync_config_to_daemon();
            }
            _ => self.settings.reduce(SettingsAction::ToggleCheckbox),
        }
    }

    pub(crate) fn handle_plugins_settings_key(&mut self, code: KeyCode) -> bool {
        if self.plugin_settings.list_mode {
            if self.plugin_settings.install_mode {
                match code {
                    KeyCode::Enter | KeyCode::Char('\r') | KeyCode::Char('\n') => {
                        let source = self
                            .plugin_settings
                            .install_source_buffer
                            .trim()
                            .to_string();
                        if source.is_empty() {
                            self.status_line = "Enter a plugin source to install".to_string();
                        } else {
                            self.plugin_settings.install_mode = false;
                            self.plugin_settings.install_source_buffer.clear();
                            self.plugin_settings.install_source_cursor = 0;
                            self.plugin_settings.loading = true;
                            self.send_daemon_command(DaemonCommand::PluginInstallSource(source));
                            self.status_line = "Installing plugin...".to_string();
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.plugin_settings.install_mode = false;
                        self.plugin_settings.install_source_buffer.clear();
                        self.plugin_settings.install_source_cursor = 0;
                        self.status_line = "Plugin installation cancelled".to_string();
                        return true;
                    }
                    KeyCode::Backspace => {
                        let cursor = clamp_to_char_boundary(
                            &self.plugin_settings.install_source_buffer,
                            self.plugin_settings.install_source_cursor,
                        );
                        if cursor > 0 {
                            let previous = previous_char_boundary(
                                &self.plugin_settings.install_source_buffer,
                                cursor,
                            );
                            self.plugin_settings
                                .install_source_buffer
                                .replace_range(previous..cursor, "");
                            self.plugin_settings.install_source_cursor = previous;
                        }
                        return true;
                    }
                    KeyCode::Left => {
                        self.plugin_settings.install_source_cursor = previous_char_boundary(
                            &self.plugin_settings.install_source_buffer,
                            self.plugin_settings.install_source_cursor,
                        );
                        return true;
                    }
                    KeyCode::Right => {
                        self.plugin_settings.install_source_cursor = next_char_boundary(
                            &self.plugin_settings.install_source_buffer,
                            self.plugin_settings.install_source_cursor,
                        );
                        return true;
                    }
                    KeyCode::Home => {
                        self.plugin_settings.install_source_cursor = 0;
                        return true;
                    }
                    KeyCode::End => {
                        self.plugin_settings.install_source_cursor =
                            self.plugin_settings.install_source_buffer.len();
                        return true;
                    }
                    KeyCode::Char(ch) => {
                        let cursor = clamp_to_char_boundary(
                            &self.plugin_settings.install_source_buffer,
                            self.plugin_settings.install_source_cursor,
                        );
                        self.plugin_settings
                            .install_source_buffer
                            .insert(cursor, ch);
                        self.plugin_settings.install_source_cursor = cursor + ch.len_utf8();
                        return true;
                    }
                    _ => return true,
                }
            }
            match code {
                KeyCode::Down => {
                    let count = self.plugin_settings.plugins.len();
                    if count > 0 {
                        self.plugin_settings.selected_index =
                            (self.plugin_settings.selected_index + 1).min(count - 1);
                    }
                    return true;
                }
                KeyCode::Up => {
                    self.plugin_settings.selected_index =
                        self.plugin_settings.selected_index.saturating_sub(1);
                    return true;
                }
                KeyCode::Enter => {
                    if let Some(plugin) = self.plugin_settings.selected_plugin() {
                        let name = plugin.name.clone();
                        self.plugin_settings.list_mode = false;
                        self.plugin_settings.detail_cursor = 0;
                        self.plugin_settings.test_result = None;
                        self.plugin_settings.schema_fields.clear();
                        self.plugin_settings.settings_values.clear();
                        self.send_daemon_command(DaemonCommand::PluginGet(name.clone()));
                        self.send_daemon_command(DaemonCommand::PluginGetSettings(name));
                    }
                    return true;
                }
                KeyCode::Char(' ') => {
                    if let Some(plugin) = self.plugin_settings.selected_plugin() {
                        let name = plugin.name.clone();
                        if plugin.enabled {
                            self.send_daemon_command(DaemonCommand::PluginDisable(name));
                        } else {
                            self.send_daemon_command(DaemonCommand::PluginEnable(name));
                        }
                    }
                    return true;
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.plugin_settings.install_mode = true;
                    self.plugin_settings.install_source_buffer.clear();
                    self.plugin_settings.install_source_cursor = 0;
                    self.status_line = "Enter a plugin source to install".to_string();
                    return true;
                }
                _ => {}
            }
        } else {
            if self.settings.is_editing() {
                match code {
                    KeyCode::Enter => {
                        let value = self.settings.edit_buffer().to_string();
                        if let Some(field_key) = self.settings.editing_field().map(str::to_string) {
                            let plugin_name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            let is_secret = self
                                .plugin_settings
                                .schema_fields
                                .iter()
                                .find(|f| f.key == field_key)
                                .map_or(false, |f| f.secret);
                            if let Some(pname) = plugin_name {
                                if let Some(entry) = self
                                    .plugin_settings
                                    .settings_values
                                    .iter_mut()
                                    .find(|(k, _, _)| *k == field_key)
                                {
                                    entry.1 = value.clone();
                                } else {
                                    self.plugin_settings.settings_values.push((
                                        field_key.clone(),
                                        value.clone(),
                                        is_secret,
                                    ));
                                }
                                self.send_daemon_command(DaemonCommand::PluginUpdateSetting {
                                    plugin_name: pname,
                                    key: field_key,
                                    value,
                                    is_secret,
                                });
                            }
                        }
                        self.settings.reduce(SettingsAction::ConfirmEdit);
                        return true;
                    }
                    KeyCode::Esc => {
                        self.settings.reduce(SettingsAction::CancelEdit);
                        return true;
                    }
                    _ => return false,
                }
            }

            match code {
                KeyCode::Down => {
                    let count = self.plugin_settings.detail_field_count();
                    if count > 0 {
                        self.plugin_settings.detail_cursor =
                            (self.plugin_settings.detail_cursor + 1).min(count - 1);
                    }
                    return true;
                }
                KeyCode::Up => {
                    self.plugin_settings.detail_cursor =
                        self.plugin_settings.detail_cursor.saturating_sub(1);
                    return true;
                }
                KeyCode::Enter => {
                    let cursor = self.plugin_settings.detail_cursor;
                    let field_count = self.plugin_settings.schema_fields.len();
                    if cursor < field_count {
                        let field = &self.plugin_settings.schema_fields[cursor];
                        let key = field.key.clone();
                        let current_value = self
                            .plugin_settings
                            .value_for_key(&key)
                            .unwrap_or("")
                            .to_string();
                        if field.field_type == "boolean" {
                            let new_val = if current_value == "true" {
                                "false"
                            } else {
                                "true"
                            };
                            if let Some(plugin) = self.plugin_settings.selected_plugin() {
                                self.send_daemon_command(DaemonCommand::PluginUpdateSetting {
                                    plugin_name: plugin.name.clone(),
                                    key,
                                    value: new_val.to_string(),
                                    is_secret: false,
                                });
                            }
                        } else {
                            let edit_value = if field.secret { "" } else { &current_value };
                            self.settings.start_editing(&key, edit_value);
                        }
                    } else {
                        let action_offset = field_count;
                        let has_api = self
                            .plugin_settings
                            .selected_plugin()
                            .map_or(false, |p| p.has_api);
                        if has_api && cursor == action_offset {
                            let name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            if let Some(name) = name {
                                self.plugin_settings.test_result = None;
                                self.send_daemon_command(DaemonCommand::PluginTestConnection(name));
                            }
                        }
                        let has_auth = self
                            .plugin_settings
                            .selected_plugin()
                            .map_or(false, |p| p.has_auth);
                        let connect_offset = action_offset + if has_api { 1 } else { 0 };
                        if has_auth && cursor == connect_offset {
                            let name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            if let Some(name) = name {
                                self.send_daemon_command(DaemonCommand::PluginOAuthStart(name));
                                self.status_line = "Starting OAuth flow...".to_string();
                            }
                        }
                    }
                    return true;
                }
                KeyCode::Esc => {
                    self.plugin_settings.list_mode = true;
                    self.plugin_settings.detail_cursor = 0;
                    self.settings.reduce(SettingsAction::CancelEdit);
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

impl TuiModel {
    pub(crate) fn set_raw_config_item(&mut self, path: &[&str], value: serde_json::Value) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/{}", path.join("/")),
            value_json: value.to_string(),
        });
        if let Some(raw) = self.config.agent_config_raw.as_mut() {
            let mut node = raw;
            for key in &path[..path.len() - 1] {
                if !node.get(*key).is_some_and(serde_json::Value::is_object) {
                    node[*key] = serde_json::json!({});
                }
                node = &mut node[*key];
            }
            node[path[path.len() - 1]] = value;
        }
    }
}
