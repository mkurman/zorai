use super::*;
use crate::widgets;
impl TuiModel {
    fn active_goal_sidebar_item(&self) -> Option<GoalSidebarCommandItem> {
        let goal_run_id = self.active_goal_sidebar_goal_run()?;
        let run = self.tasks.goal_run_by_id(goal_run_id)?;
        let selected_row = self.goal_sidebar.selected_row();

        match self.goal_sidebar.active_tab() {
            GoalSidebarTab::Steps => {
                let mut steps = run.steps.clone();
                steps.sort_by_key(|step| step.order);
                let step = steps.get(selected_row)?;
                Some(GoalSidebarCommandItem::Step {
                    step_id: step.id.clone(),
                })
            }
            GoalSidebarTab::Checkpoints => {
                let checkpoint = self
                    .tasks
                    .checkpoints_for_goal_run(goal_run_id)
                    .get(selected_row)?;
                let step_id = checkpoint.step_index.and_then(|step_index| {
                    run.steps
                        .iter()
                        .find(|step| step.order as usize == step_index)
                        .map(|step| step.id.clone())
                });
                Some(GoalSidebarCommandItem::Checkpoint { step_id })
            }
            GoalSidebarTab::Tasks => {
                let tasks: Vec<_> = if !run.child_task_ids.is_empty() {
                    run.child_task_ids
                        .iter()
                        .filter_map(|task_id| self.tasks.task_by_id(task_id))
                        .collect()
                } else {
                    self.tasks
                        .tasks()
                        .iter()
                        .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
                        .collect()
                };
                let task = *tasks.get(selected_row)?;
                Some(GoalSidebarCommandItem::Task {
                    target: sidebar::SidebarItemTarget::Task {
                        task_id: task.id.clone(),
                    },
                })
            }
            GoalSidebarTab::Files => {
                let thread_id = run.thread_id.clone()?;
                let context = self.tasks.work_context_for_thread(&thread_id)?;
                let entry = context
                    .entries
                    .iter()
                    .filter(|entry| match entry.goal_run_id.as_deref() {
                        Some(entry_goal_run_id) => entry_goal_run_id == goal_run_id,
                        None => true,
                    })
                    .nth(selected_row)?;
                Some(GoalSidebarCommandItem::File {
                    thread_id,
                    path: entry.path.clone(),
                })
            }
        }
    }

    pub(crate) fn handle_goal_sidebar_enter(&mut self) -> bool {
        let Some(item) = self.active_goal_sidebar_item() else {
            return false;
        };

        match item {
            GoalSidebarCommandItem::Step { step_id } => {
                if self.select_goal_step_in_active_run(step_id) {
                    self.focus = FocusArea::Chat;
                    return true;
                }
            }
            GoalSidebarCommandItem::Checkpoint { step_id } => {
                let Some(step_id) = step_id else {
                    self.status_line = "Checkpoint has no linked step".to_string();
                    return false;
                };
                if self.select_goal_step_in_active_run(step_id) {
                    self.focus = FocusArea::Chat;
                    return true;
                }
            }
            GoalSidebarCommandItem::Task { target } => {
                self.open_sidebar_target(target);
                self.focus = FocusArea::Chat;
                return true;
            }
            GoalSidebarCommandItem::File { thread_id, path } => {
                let status_line = path.clone();
                self.open_work_context_for_thread(
                    thread_id.clone(),
                    Some(path),
                    None,
                    self.current_goal_return_target(),
                    status_line,
                );
                return true;
            }
        }

        false
    }

    pub(crate) fn resolve_target_agent_id(&self, agent_alias: &str) -> Option<String> {
        match agent_alias.trim().to_ascii_lowercase().as_str() {
            "" => None,
            "svarog" | "swarog" | "main" => Some(zorai_protocol::AGENT_ID_SWAROG.to_string()),
            "rarog" | "concierge" => Some(zorai_protocol::AGENT_ID_RAROG.to_string()),
            "weles" => Some("weles".to_string()),
            "swarozyc" | "radogost" | "domowoj" | "swietowit" | "perun" | "mokosh" | "dazhbog"
            | "rod" => Some(agent_alias.trim().to_ascii_lowercase()),
            _ => self.subagents.entries.iter().find_map(|entry| {
                if entry.id.eq_ignore_ascii_case(agent_alias)
                    || entry.name.eq_ignore_ascii_case(agent_alias)
                    || entry
                        .id
                        .strip_suffix("_builtin")
                        .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_alias))
                {
                    Some(
                        entry
                            .id
                            .strip_suffix("_builtin")
                            .unwrap_or(entry.id.as_str())
                            .to_ascii_lowercase(),
                    )
                } else {
                    None
                }
            }),
        }
    }

    pub(crate) fn active_thread_owner_agent_id(&self) -> Option<String> {
        let Some(thread) = self.chat.active_thread() else {
            return self
                .pending_new_thread_target_agent
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
        };
        if thread.id == "concierge" {
            return Some(zorai_protocol::AGENT_ID_RAROG.to_string());
        }

        if let Some(handoff) = thread.thread_handoff_state.as_ref() {
            let active = handoff.active_agent_id.trim();
            if !active.is_empty() {
                if let Some(agent_id) = self.resolve_target_agent_id(active) {
                    return Some(agent_id);
                }
                if !active.eq_ignore_ascii_case("svarog") && !active.eq_ignore_ascii_case("swarog")
                {
                    return Some(active.to_ascii_lowercase());
                }
            }
        }

        if let Some(agent_name) = thread
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(agent_id) = self.resolve_target_agent_id(agent_name) {
                return Some(agent_id);
            }
            if !agent_name.eq_ignore_ascii_case("svarog")
                && !agent_name.eq_ignore_ascii_case("swarog")
            {
                return Some(agent_name.to_string());
            }
        }

        if widgets::thread_picker::is_rarog_thread(thread) {
            Some(zorai_protocol::AGENT_ID_RAROG.to_string())
        } else if widgets::thread_picker::is_weles_thread(thread) {
            Some("weles".to_string())
        } else {
            Some(zorai_protocol::AGENT_ID_SWAROG.to_string())
        }
    }

    pub(crate) fn active_thread_target_agent_config(&self) -> Option<PendingTargetAgentConfig> {
        let target_agent_id = self.active_thread_owner_agent_id()?;
        if target_agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_SWAROG) {
            return None;
        }
        let mut profile = self.current_conversation_agent_profile();
        if let Some(runtime) = self.chat.active_thread_runtime_metadata() {
            if let Some(provider) = runtime.provider {
                profile.provider = provider;
            }
            if let Some(model) = runtime.model {
                profile.model = model;
            }
            if let Some(reasoning_effort) = runtime.reasoning_effort {
                profile.reasoning_effort = Some(reasoning_effort);
            }
        }
        Some(PendingTargetAgentConfig {
            target_agent_id,
            target_agent_name: profile.agent_label,
            provider_id: profile.provider,
            model: profile.model,
            reasoning_effort: profile.reasoning_effort,
        })
    }

    fn effort_picker_index(reasoning_effort: Option<&str>) -> usize {
        match reasoning_effort
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "minimal" => 1,
            "low" => 2,
            "medium" => 3,
            "high" => 4,
            "xhigh" => 5,
            "max" => 6,
            _ => 0,
        }
    }

    fn normalized_effort_value(reasoning_effort: Option<&str>) -> Option<String> {
        reasoning_effort
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("none"))
            .map(ToOwned::to_owned)
    }

    pub(crate) fn effort_picker_current_value(&self) -> Option<String> {
        match self.settings_picker_target {
            Some(SettingsPickerTarget::SubAgentReasoningEffort) => {
                self.subagents.editor.as_ref().and_then(|editor| {
                    Self::normalized_effort_value(editor.reasoning_effort.as_deref())
                })
            }
            Some(SettingsPickerTarget::TargetAgentReasoningEffort) => self
                .pending_target_agent_config
                .as_ref()
                .and_then(|pending| {
                    Self::normalized_effort_value(pending.reasoning_effort.as_deref())
                }),
            Some(SettingsPickerTarget::ConciergeReasoningEffort) => {
                Self::normalized_effort_value(self.concierge.reasoning_effort.as_deref())
            }
            Some(SettingsPickerTarget::SvarogReasoningEffort) => {
                Self::normalized_effort_value(Some(&self.config.reasoning_effort))
            }
            Some(SettingsPickerTarget::CompactionWelesReasoningEffort) => {
                Self::normalized_effort_value(Some(&self.config.compaction_weles_reasoning_effort))
            }
            Some(SettingsPickerTarget::CompactionCustomReasoningEffort) => {
                Self::normalized_effort_value(Some(&self.config.compaction_custom_reasoning_effort))
            }
            _ => self
                .chat
                .active_thread_runtime_metadata()
                .and_then(|runtime| {
                    Self::normalized_effort_value(runtime.reasoning_effort.as_deref())
                })
                .or_else(|| {
                    let profile = self.current_conversation_agent_profile();
                    Self::normalized_effort_value(profile.reasoning_effort.as_deref())
                }),
        }
    }

    pub(crate) fn sync_effort_picker_cursor_to_current(&mut self) {
        self.modal.set_picker_item_count(7);
        let current = self.effort_picker_current_value();
        let cursor = Self::effort_picker_index(current.as_deref());
        self.modal
            .reduce(modal::ModalAction::Navigate(cursor as i32));
    }

    pub(crate) fn open_active_thread_target_provider_picker(&mut self) -> bool {
        let Some(pending) = self.active_thread_target_agent_config() else {
            return false;
        };
        self.pending_target_agent_config = Some(pending);
        self.open_provider_picker(SettingsPickerTarget::TargetAgentProvider);
        true
    }

    pub(crate) fn open_active_thread_target_model_picker(&mut self) -> bool {
        let Some(pending) = self.active_thread_target_agent_config() else {
            return false;
        };
        let provider_id = pending.provider_id.clone();
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        self.pending_target_agent_config = Some(pending);
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::TargetAgentModel,
            provider_id,
            base_url,
            api_key,
            auth_source,
        );
        true
    }

    pub(crate) fn open_active_thread_target_effort_picker(&mut self) -> bool {
        let Some(pending) = self.active_thread_target_agent_config() else {
            return false;
        };
        self.pending_target_agent_config = Some(pending);
        self.settings_picker_target = Some(SettingsPickerTarget::TargetAgentReasoningEffort);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
        self.sync_effort_picker_cursor_to_current();
        true
    }

    fn voice_lookup_string(raw: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
        raw.and_then(|value| {
            path.iter()
                .try_fold(value, |acc, key| acc.get(*key))
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    }

    fn voice_lookup_bool(raw: Option<&serde_json::Value>, path: &[&str]) -> Option<bool> {
        raw.and_then(|value| {
            path.iter()
                .try_fold(value, |acc, key| acc.get(*key))
                .and_then(|value| value.as_bool())
        })
    }

    fn voice_audio_string(
        raw: Option<&serde_json::Value>,
        flat_key: &str,
        nested_path: &[&str],
        fallback: &str,
    ) -> String {
        Self::voice_lookup_string(raw, nested_path)
            .or_else(|| Self::voice_lookup_string(raw, &[flat_key]))
            .or_else(|| {
                Self::voice_lookup_string(raw.and_then(|value| value.get("extra")), &[flat_key])
            })
            .unwrap_or_else(|| fallback.to_string())
    }

    fn voice_audio_bool(
        raw: Option<&serde_json::Value>,
        flat_key: &str,
        nested_path: &[&str],
        fallback: bool,
    ) -> bool {
        Self::voice_lookup_bool(raw, nested_path)
            .or_else(|| Self::voice_lookup_bool(raw, &[flat_key]))
            .or_else(|| {
                Self::voice_lookup_bool(raw.and_then(|value| value.get("extra")), &[flat_key])
            })
            .unwrap_or(fallback)
    }

    pub(crate) fn toggle_voice_capture(&mut self) {
        if self.voice_recording {
            if let Some(path) = self.stop_voice_capture() {
                let raw = self.config.agent_config_raw.as_ref();
                let provider = Self::voice_audio_string(
                    raw,
                    "audio_stt_provider",
                    &["audio", "stt", "provider"],
                    zorai_shared::providers::PROVIDER_ID_OPENAI,
                );
                let model = Self::voice_audio_string(
                    raw,
                    "audio_stt_model",
                    &["audio", "stt", "model"],
                    "whisper-1",
                );
                let language = Self::voice_lookup_string(raw, &["audio", "stt", "language"])
                    .or_else(|| Self::voice_lookup_string(raw, &["audio_stt_language"]))
                    .or_else(|| {
                        Self::voice_lookup_string(
                            raw.and_then(|value| value.get("extra")),
                            &["audio_stt_language"],
                        )
                    });
                let args_json = serde_json::json!({
                    "path": path,
                    "mime_type": "audio/wav",
                    "provider": provider,
                    "model": model,
                    "language": language,
                })
                .to_string();
                self.send_daemon_command(DaemonCommand::SpeechToText { args_json });
                self.status_line = "Transcribing voice capture...".to_string();
            }
            return;
        }

        let enabled = Self::voice_audio_bool(
            self.config.agent_config_raw.as_ref(),
            "audio_stt_enabled",
            &["audio", "stt", "enabled"],
            true,
        );
        if !enabled {
            self.status_line = "STT disabled in audio settings".to_string();
            return;
        }
        self.start_voice_capture();
    }

    pub(crate) fn speak_latest_assistant_message(&mut self) {
        let Some(thread) = self.chat.active_thread() else {
            self.status_line = "Open a thread first".to_string();
            return;
        };

        let selected_index = self.chat.selected_message();
        let selected_content = selected_index
            .and_then(|idx| thread.messages.get(idx))
            .filter(|message| {
                message.role == chat::MessageRole::Assistant && !message.content.trim().is_empty()
            })
            .map(|message| message.content.clone());

        let content_to_speak = if let Some(content) = selected_content {
            content
        } else if selected_index.is_some() {
            self.status_line = "Selected message is not speakable assistant text".to_string();
            self.show_input_notice(
                "Select an assistant message to speak",
                InputNoticeKind::Warning,
                60,
                true,
            );
            return;
        } else {
            let Some(message) = thread.messages.iter().rev().find(|message| {
                message.role == chat::MessageRole::Assistant && !message.content.trim().is_empty()
            }) else {
                self.status_line = "No assistant message available to speak".to_string();
                return;
            };
            message.content.clone()
        };

        let enabled = Self::voice_audio_bool(
            self.config.agent_config_raw.as_ref(),
            "audio_tts_enabled",
            &["audio", "tts", "enabled"],
            true,
        );
        if !enabled {
            self.status_line = "TTS disabled in audio settings".to_string();
            return;
        }

        if let Some(mut child) = self.voice_player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.clear_voice_player_ipc();

        let raw = self.config.agent_config_raw.as_ref();
        let provider = Self::voice_audio_string(
            raw,
            "audio_tts_provider",
            &["audio", "tts", "provider"],
            zorai_shared::providers::PROVIDER_ID_OPENAI,
        );
        let model = Self::voice_audio_string(
            raw,
            "audio_tts_model",
            &["audio", "tts", "model"],
            "gpt-4o-mini-tts",
        );
        let voice =
            Self::voice_audio_string(raw, "audio_tts_voice", &["audio", "tts", "voice"], "alloy");

        let cache_key = tts_cache_key(&provider, &model, &voice, &content_to_speak);
        if let Some(cached) = self.tts_audio_cache.get(&cache_key).cloned() {
            if cached.exists() {
                let path = cached.to_string_lossy().to_string();
                self.play_audio_path(&path);
                self.status_line = "Replaying cached speech (Ctrl+S stop)".to_string();
                return;
            }
            self.tts_audio_cache.remove(&cache_key);
        }

        let args_json = serde_json::json!({
            "input": content_to_speak,
            "provider": provider,
            "model": model,
            "voice": voice,
        })
        .to_string();
        self.pending_tts_cache_key = Some(cache_key);
        self.send_daemon_command(DaemonCommand::TextToSpeech { args_json });
        self.status_line = "Synthesizing speech...".to_string();
        self.set_active_thread_activity("preparing speech");
    }
}

pub(crate) fn tts_cache_key(provider: &str, model: &str, voice: &str, content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    provider.hash(&mut hasher);
    model.hash(&mut hasher);
    voice.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tts_cache_key_tests {
    use super::tts_cache_key;

    #[test]
    fn identical_inputs_produce_identical_keys() {
        let a = tts_cache_key(
            "openrouter",
            "x-ai/grok-voice-tts-1.0",
            "eve",
            "hello there",
        );
        let b = tts_cache_key(
            "openrouter",
            "x-ai/grok-voice-tts-1.0",
            "eve",
            "hello there",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn voice_or_content_change_changes_key() {
        let base = tts_cache_key(
            "openrouter",
            "x-ai/grok-voice-tts-1.0",
            "eve",
            "hello there",
        );
        assert_ne!(
            base,
            tts_cache_key(
                "openrouter",
                "x-ai/grok-voice-tts-1.0",
                "ara",
                "hello there"
            )
        );
        assert_ne!(
            base,
            tts_cache_key(
                "openrouter",
                "x-ai/grok-voice-tts-1.0",
                "eve",
                "different text"
            )
        );
        assert_ne!(
            base,
            tts_cache_key("openai", "x-ai/grok-voice-tts-1.0", "eve", "hello there")
        );
    }
}
