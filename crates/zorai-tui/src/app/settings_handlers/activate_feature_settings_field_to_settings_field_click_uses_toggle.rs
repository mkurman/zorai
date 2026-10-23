use super::*;
impl TuiModel {
    pub(crate) fn activate_feature_settings_field(&mut self, field: &str) -> bool {
        match field {
            "feat_tier_override" => {
                let tiers = ["newcomer", "familiar", "power_user", "expert"];
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("tier"))
                    .and_then(|t| t.get("user_override"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&self.tier.current_tier);
                let current_idx = tiers.iter().position(|t| *t == current).unwrap_or(0);
                let next = tiers[(current_idx + 1) % tiers.len()];
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/tier/user_override".to_string(),
                    value_json: format!("\"{}\"", next),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("tier").is_none() {
                        raw["tier"] = serde_json::json!({});
                    }
                    raw["tier"]["user_override"] = serde_json::Value::String(next.to_string());
                }
                self.tier.on_tier_changed(next);
                true
            }
            "feat_audio_stt_language" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("audio"))
                    .and_then(|a| a.get("stt"))
                    .and_then(|s| s.get("language"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.settings
                    .start_editing("feat_audio_stt_language", &current);
                true
            }
            "code_review_agent" => {
                let options = std::iter::once("thread".to_string())
                    .chain(
                        self.subagents
                            .entries
                            .iter()
                            .filter(|entry| entry.enabled)
                            .map(|entry| entry.id.clone()),
                    )
                    .collect::<Vec<_>>();
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("code_review_agent"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("thread");
                let next = options
                    .iter()
                    .position(|option| option == current)
                    .map(|index| options[(index + 1) % options.len()].clone())
                    .unwrap_or_else(|| "thread".to_string());
                if options.len() < 2 {
                    self.status_line =
                        "Enable a sub-agent to use it as the code reviewer".to_string();
                }
                self.set_raw_config_item(&["code_review_agent"], serde_json::Value::String(next));
                true
            }
            "code_review_commit_window" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("code_review_commit_window"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5)
                    .to_string();
                self.settings
                    .start_editing("code_review_commit_window", &current);
                true
            }
            "feat_heartbeat_interval" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat_interval_mins"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30)
                    .to_string();
                self.settings
                    .start_editing("feat_heartbeat_interval", &current);
                true
            }
            "feat_heartbeat_cron" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat_cron"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.settings.start_editing("feat_heartbeat_cron", &current);
                true
            }
            "feat_heartbeat_quiet_start" | "feat_heartbeat_quiet_end" => {
                let key = if field == "feat_heartbeat_quiet_start" {
                    "quiet_hours_start"
                } else {
                    "quiet_hours_end"
                };
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get(key))
                    .and_then(|v| v.as_u64())
                    .map(|hour| hour.to_string())
                    .unwrap_or_default();
                self.settings.start_editing(field, &current);
                true
            }
            "feat_decay_half_life_hours" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("memory_decay_half_life_hours"))
                    .and_then(|v| v.as_f64())
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "69".to_string());
                self.settings
                    .start_editing("feat_decay_half_life_hours", &current);
                true
            }
            "feat_heuristic_promotion_threshold" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("heuristic_promotion_threshold"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "5".to_string());
                self.settings
                    .start_editing("feat_heuristic_promotion_threshold", &current);
                true
            }
            "feat_skill_community_preapprove_timeout_secs" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("skill_recommendation"))
                    .and_then(|s| s.get("community_preapprove_timeout_secs"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "30".to_string());
                self.settings
                    .start_editing("feat_skill_community_preapprove_timeout_secs", &current);
                true
            }
            "feat_skill_suggest_global_enable_after_approvals" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("skill_recommendation"))
                    .and_then(|s| s.get("suggest_global_enable_after_approvals"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "3".to_string());
                self.settings
                    .start_editing("feat_skill_suggest_global_enable_after_approvals", &current);
                true
            }
            "feat_audio_stt_provider" => {
                self.open_provider_picker(SettingsPickerTarget::AudioSttProvider);
                true
            }
            "feat_audio_stt_model" => {
                self.open_audio_model_picker("stt");
                true
            }
            "feat_audio_tts_provider" => {
                self.open_provider_picker(SettingsPickerTarget::AudioTtsProvider);
                true
            }
            "feat_audio_tts_model" => {
                self.open_audio_model_picker("tts");
                true
            }
            "feat_audio_tts_voice" => {
                let current = self.config.audio_tts_voice();
                let current = if current.is_empty() {
                    "alloy".to_string()
                } else {
                    current
                };
                self.settings
                    .start_editing("feat_audio_tts_voice", &current);
                true
            }
            "feat_image_generation_provider" => {
                self.open_provider_picker(SettingsPickerTarget::ImageGenerationProvider);
                true
            }
            "feat_image_generation_model" => {
                self.open_image_generation_model_picker();
                true
            }
            "feat_embedding_provider" => {
                self.open_provider_picker(SettingsPickerTarget::EmbeddingProvider);
                true
            }
            "feat_embedding_model" => {
                self.open_embedding_model_picker();
                true
            }
            "feat_embedding_dimensions" => {
                self.settings.start_editing(
                    "feat_embedding_dimensions",
                    &self.config.semantic_embedding_dimensions().to_string(),
                );
                true
            }
            "feat_auto_thread_title" => {
                let modes = ["off", "rarog", "weles"];
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("auto_thread_title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("off");
                let current_idx = modes.iter().position(|mode| *mode == current).unwrap_or(0);
                let next = modes[(current_idx + 1) % modes.len()];
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/auto_thread_title".to_string(),
                    value_json: format!("\"{next}\""),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    raw["auto_thread_title"] = serde_json::Value::String(next.to_string());
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn settings_field_click_uses_toggle(&self) -> bool {
        matches!(
            self.current_settings_field_name(),
            "managed_sandbox_enabled"
                | "managed_security_level"
                | "gateway_enabled"
                | "web_search_enabled"
                | "enable_streaming"
                | "auto_retry"
                | "enable_conversation_memory"
                | "enable_honcho_memory"
                | "anticipatory_enabled"
                | "anticipatory_morning_brief"
                | "anticipatory_predictive_hydration"
                | "anticipatory_stuck_detection"
                | "operator_model_enabled"
                | "operator_model_allow_message_statistics"
                | "operator_model_allow_approval_learning"
                | "operator_model_allow_attention_tracking"
                | "operator_model_allow_implicit_feedback"
                | "collaboration_enabled"
                | "compliance_sign_all_events"
                | "tool_synthesis_enabled"
                | "tool_synthesis_require_activation"
                | "auto_compact_context"
                | "compaction_strategy"
                | "compaction_weles_provider"
                | "compaction_weles_reasoning_effort"
                | "compaction_weles_api_transport"
                | "compaction_custom_provider"
                | "compaction_custom_auth_source"
                | "compaction_custom_api_transport"
                | "compaction_custom_reasoning_effort"
                | "snapshot_auto_cleanup"
                | "feat_tier_override"
                | "feat_check_stale_todos"
                | "feat_check_stuck_goals"
                | "feat_check_unreplied_messages"
                | "feat_check_repo_changes"
                | "feat_check_plugin_auth"
                | "feat_heartbeat_enabled"
                | "desktop_notifications"
                | "feat_audio_tts_auto_speak"
                | "feat_consolidation_enabled"
                | "feat_skill_recommendation_enabled"
                | "feat_skill_background_community_search"
                | "feat_audio_stt_enabled"
                | "feat_audio_tts_enabled"
                | "feat_embedding_enabled"
                | "feat_auto_thread_title"
                | "whatsapp_link_device"
                | "whatsapp_relink_device"
                | "mlflow_enabled"
                | "mlflow_visible_operator"
                | "mlflow_gateway"
                | "mlflow_goal_task"
                | "mlflow_subagent"
                | "mlflow_concierge"
                | "mlflow_heartbeat_autonomous"
                | "mlflow_capture_mode"
                | "mlflow_test_connection"
                | "mlflow_send_test_trace"
        ) || crate::state::settings::is_tool_toggle_field(self.current_settings_field_name())
    }
}
