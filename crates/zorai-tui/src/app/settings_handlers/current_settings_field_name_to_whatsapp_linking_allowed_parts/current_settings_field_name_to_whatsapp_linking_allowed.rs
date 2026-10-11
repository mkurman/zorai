use super::*;
use zorai_shared::providers::{
    AudioToolKind, PROVIDER_ID_CUSTOM, PROVIDER_ID_HUGGINGFACE, PROVIDER_ID_OPENROUTER,
};
impl TuiModel {
    pub(crate) fn current_settings_field_name(&self) -> &str {
        if self.settings.active_tab() == crate::state::SettingsTab::Concierge {
            return match self.settings.field_cursor() {
                0 => "concierge_enabled",
                1 => "concierge_detail_level",
                2 => "concierge_provider",
                3 => "concierge_model",
                4 => "concierge_reasoning_effort",
                5 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) => {
                    "concierge_openrouter_provider_order"
                }
                6 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) => {
                    "concierge_openrouter_provider_ignore"
                }
                7 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) => {
                    "concierge_openrouter_allow_fallbacks"
                }
                7 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_HUGGINGFACE) => {
                    "concierge_huggingface_provider"
                }
                8 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER)
                    || self.concierge.provider.as_deref() == Some(PROVIDER_ID_HUGGINGFACE) =>
                {
                    "concierge_base_url"
                }
                9 if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER)
                    || self.concierge.provider.as_deref() == Some(PROVIDER_ID_HUGGINGFACE) =>
                {
                    "rarog_alias"
                }
                7 => "concierge_base_url",
                8 => "rarog_alias",
                6 => "concierge_claude_permission_mode",
                _ => "",
            };
        }
        self.settings.current_field_name_with_config(&self.config)
    }

    pub(crate) fn settings_field_count(&self) -> usize {
        if self.settings.active_tab() == crate::state::SettingsTab::Concierge {
            return if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) {
                10
            } else if self.concierge.provider.as_deref() == Some(PROVIDER_ID_HUGGINGFACE) {
                10
            } else {
                9
            };
        }
        self.settings.field_count_with_config(&self.config)
    }

    pub(crate) fn clamp_settings_cursor(&mut self) {
        self.settings
            .clamp_field_cursor(self.settings_field_count());
    }

    pub(crate) fn open_honcho_editor(&mut self) {
        let editor = crate::state::config::HonchoEditorState::from_config(&self.config);
        self.config.honcho_editor = Some(editor);
        self.settings.reduce(SettingsAction::CancelEdit);
        self.status_line = "Editing Honcho memory settings".to_string();
    }

    pub(crate) fn close_honcho_editor(&mut self) {
        self.config.honcho_editor = None;
        self.settings.reduce(SettingsAction::CancelEdit);
        self.status_line = "Closed Honcho memory editor".to_string();
    }

    pub(crate) fn commit_honcho_editor(&mut self) {
        let Some(editor) = self.config.honcho_editor.take() else {
            return;
        };
        self.config.enable_honcho_memory = editor.enabled;
        self.config.honcho_api_key = editor.api_key;
        self.config.honcho_base_url = editor.base_url;
        self.config.honcho_workspace_id = if editor.workspace_id.trim().is_empty() {
            "zorai".to_string()
        } else {
            editor.workspace_id
        };
        self.settings.reduce(SettingsAction::CancelEdit);
        self.sync_config_to_daemon();
        self.status_line = "Updated Honcho memory settings".to_string();
    }

    pub(crate) fn cycle_compaction_strategy(&mut self) {
        self.config.compaction_strategy = match self.config.compaction_strategy.as_str() {
            "heuristic" => "weles".to_string(),
            "weles" => "custom_model".to_string(),
            _ => "heuristic".to_string(),
        };
        self.clamp_settings_cursor();
        self.sync_config_to_daemon();
    }

    pub(crate) fn apply_compaction_custom_provider(&mut self, provider_id: &str) {
        self.config.compaction_custom_provider = provider_id.to_string();
        self.config.compaction_custom_base_url = providers::find_by_id(provider_id)
            .map(|provider| provider.default_base_url.to_string())
            .unwrap_or_default();
        self.config.compaction_custom_auth_source =
            providers::default_auth_source_for(provider_id).to_string();
        self.config.compaction_custom_api_transport =
            providers::default_transport_for(provider_id).to_string();
        self.config.compaction_custom_model =
            providers::default_model_for_provider_auth(provider_id, "api_key");
        self.config.compaction_custom_context_window_tokens = if provider_id == PROVIDER_ID_CUSTOM {
            128_000
        } else {
            providers::known_context_window_for(provider_id, &self.config.compaction_custom_model)
                .unwrap_or(128_000)
        };
        self.config.compaction_custom_api_key.clear();
        self.config.compaction_custom_assistant_id.clear();
    }

    pub(crate) fn normalize_compaction_custom_transport(&mut self) {
        self.config.compaction_custom_api_transport = self.provider_transport_snapshot(
            &self.config.compaction_custom_provider,
            &self.config.compaction_custom_auth_source,
            &self.config.compaction_custom_model,
            &self.config.compaction_custom_api_transport,
        );
    }

    pub(crate) fn open_provider_picker(&mut self, target: SettingsPickerTarget) {
        self.settings_picker_target = Some(target);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.sync_provider_picker_item_count();
    }

    pub(crate) fn open_compaction_weles_model_picker(&mut self) {
        let provider_id = self.config.compaction_weles_provider.clone();
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::CompactionWelesModel,
            provider_id,
            base_url,
            api_key,
            auth_source,
        );
    }

    pub(crate) fn open_compaction_custom_model_picker(&mut self) {
        let provider_id = self.config.compaction_custom_provider.clone();
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::CompactionCustomModel,
            provider_id,
            self.config.compaction_custom_base_url.clone(),
            self.config.compaction_custom_api_key.clone(),
            self.config.compaction_custom_auth_source.clone(),
        );
    }

    pub(crate) fn model_picker_current_selection(&self) -> (String, Option<String>) {
        match self
            .settings_picker_target
            .unwrap_or(SettingsPickerTarget::Model)
        {
            SettingsPickerTarget::AudioSttModel => (self.config.audio_stt_model(), None),
            SettingsPickerTarget::AudioTtsModel => (self.config.audio_tts_model(), None),
            SettingsPickerTarget::ImageGenerationModel => {
                (self.config.image_generation_model(), None)
            }
            SettingsPickerTarget::EmbeddingModel => (self.config.semantic_embedding_model(), None),
            SettingsPickerTarget::CompactionWelesModel => {
                (self.config.compaction_weles_model.clone(), None)
            }
            SettingsPickerTarget::CompactionCustomModel => {
                (self.config.compaction_custom_model.clone(), None)
            }
            SettingsPickerTarget::SubAgentModel => self
                .subagents
                .editor
                .as_ref()
                .map(|editor| (editor.model.clone(), None))
                .unwrap_or_else(|| (String::new(), None)),
            SettingsPickerTarget::ConciergeModel => {
                (self.concierge.model.clone().unwrap_or_default(), None)
            }
            SettingsPickerTarget::TargetAgentModel => self
                .pending_target_agent_config
                .as_ref()
                .map(|pending| (pending.model.clone(), None))
                .unwrap_or_else(|| (String::new(), None)),
            _ => (
                self.config.model.clone(),
                Some(self.config.custom_model_name.clone()),
            ),
        }
    }

    fn target_agent_chat_picker_models(
        &self,
        provider_id: &str,
        auth_source: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let mut models = providers::known_models_for_provider_auth(provider_id, auth_source);
        let allows_unlisted = matches!(
            provider_id,
            zorai_shared::providers::PROVIDER_ID_OLLAMA
                | zorai_shared::providers::PROVIDER_ID_LMSTUDIO
                | zorai_shared::providers::PROVIDER_ID_HUGGINGFACE
                | zorai_shared::providers::PROVIDER_ID_CUSTOM
        );
        let fetched_for_active_picker = self
            .model_picker_provider_id
            .as_deref()
            .is_some_and(|active_provider| active_provider == provider_id);
        let merge_fetched = fetched_for_active_picker
            && (allows_unlisted || self.should_fetch_remote_models(provider_id, auth_source));
        if merge_fetched {
            for model in self.config.fetched_models() {
                if !models.iter().any(|existing| existing.id == model.id) {
                    models.push(model.clone());
                }
            }
        }
        models
    }

    pub(crate) fn available_model_picker_models(&self) -> Vec<crate::state::config::FetchedModel> {
        let (current_model, custom_model_name) = self.model_picker_current_selection();
        match self
            .settings_picker_target
            .unwrap_or(SettingsPickerTarget::Model)
        {
            SettingsPickerTarget::TargetAgentModel => {
                let provider_id = self
                    .pending_target_agent_config
                    .as_ref()
                    .map(|pending| pending.provider_id.as_str())
                    .unwrap_or(self.config.provider.as_str());
                let auth_source = self.provider_auth_snapshot(provider_id).2;
                widgets::model_picker::merge_models_for_selection(
                    &self.target_agent_chat_picker_models(provider_id, &auth_source),
                    &current_model,
                    custom_model_name.as_deref(),
                )
            }
            SettingsPickerTarget::AudioSttModel
            | SettingsPickerTarget::AudioTtsModel
            | SettingsPickerTarget::ImageGenerationModel
            | SettingsPickerTarget::EmbeddingModel => {
                let (endpoint, provider_id) = match self
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::AudioSttModel => {
                        ("stt", self.config.audio_stt_provider())
                    }
                    SettingsPickerTarget::AudioTtsModel => {
                        ("tts", self.config.audio_tts_provider())
                    }
                    SettingsPickerTarget::ImageGenerationModel => {
                        ("image_generation", self.config.image_generation_provider())
                    }
                    SettingsPickerTarget::EmbeddingModel => {
                        ("embedding", self.config.semantic_embedding_provider())
                    }
                    _ => return Vec::new(),
                };
                let mut models = match endpoint {
                    "image_generation" => Self::image_generation_catalog_models(&provider_id),
                    "embedding" => Self::embedding_catalog_models(&provider_id),
                    _ => Self::audio_catalog_models(endpoint, &provider_id),
                };
                for model in self.config.fetched_models() {
                    let include = match endpoint {
                        "image_generation" => {
                            let pricing_image = model
                                .pricing
                                .as_ref()
                                .and_then(|pricing| pricing.image.as_deref())
                                .is_some();
                            zorai_shared::providers::derive_model_feature_capabilities(
                                &provider_id,
                                &model.id,
                                model.metadata.as_ref(),
                                pricing_image,
                            )
                            .image_generation
                        }
                        "embedding" => {
                            model.id.to_ascii_lowercase().contains("embedding")
                                || model.id.to_ascii_lowercase().contains("embed")
                                || model
                                    .metadata
                                    .as_ref()
                                    .map(|metadata| {
                                        metadata
                                            .to_string()
                                            .to_ascii_lowercase()
                                            .contains("embedding")
                                    })
                                    .unwrap_or(false)
                        }
                        _ => Self::fetched_model_supports_audio_endpoint(model, endpoint),
                    };
                    if include && !models.iter().any(|existing| existing.id == model.id) {
                        models.push(model.clone());
                    }
                }
                widgets::model_picker::merge_models_for_selection(
                    &models,
                    &current_model,
                    custom_model_name.as_deref(),
                )
            }
            _ => widgets::model_picker::available_models_for(
                &self.config,
                &current_model,
                custom_model_name.as_deref(),
            ),
        }
    }

    pub(crate) fn filtered_model_picker_models(&self) -> Vec<crate::state::config::FetchedModel> {
        widgets::model_picker::filtered_models_for_selection(
            &self.available_model_picker_models(),
            self.modal.command_query(),
        )
    }

    pub(super) fn available_provider_picker_defs(&self) -> Vec<&'static providers::ProviderDef> {
        match self.settings_picker_target {
            Some(SettingsPickerTarget::AudioSttProvider) => {
                widgets::provider_picker::available_audio_provider_defs(
                    &self.auth,
                    AudioToolKind::SpeechToText,
                )
            }
            Some(SettingsPickerTarget::AudioTtsProvider) => {
                widgets::provider_picker::available_audio_provider_defs(
                    &self.auth,
                    AudioToolKind::TextToSpeech,
                )
            }
            Some(SettingsPickerTarget::EmbeddingProvider) => {
                widgets::provider_picker::available_embedding_provider_defs(&self.auth)
            }
            _ => widgets::provider_picker::available_provider_defs(&self.auth),
        }
    }

    pub(crate) fn filtered_provider_picker_defs(&self) -> Vec<&'static providers::ProviderDef> {
        widgets::provider_picker::filtered_provider_defs(
            self.available_provider_picker_defs(),
            self.modal.command_query(),
        )
    }

    pub(crate) fn sync_model_picker_item_count(&mut self) {
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
        let count = widgets::model_picker::filtered_model_picker_item_count(
            &models,
            self.modal.command_query(),
        );
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn sync_provider_picker_item_count(&mut self) {
        let count = self.filtered_provider_picker_defs().len();
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn filtered_openrouter_endpoint_providers(&self) -> Vec<&str> {
        let query = self.modal.command_query().trim().to_ascii_lowercase();
        if query.is_empty() {
            return self
                .config
                .openrouter_endpoint_providers
                .iter()
                .map(String::as_str)
                .collect();
        }
        let terms = query.split_whitespace().collect::<Vec<_>>();
        self.config
            .openrouter_endpoint_providers
            .iter()
            .map(String::as_str)
            .filter(|slug| {
                let slug = slug.to_ascii_lowercase();
                terms.iter().all(|term| slug.contains(term))
            })
            .collect()
    }

    pub(crate) fn sync_openrouter_provider_picker_item_count(&mut self) {
        let count = self.filtered_openrouter_endpoint_providers().len();
        self.modal.set_picker_item_count(count);
    }

    pub(crate) fn begin_targeted_custom_model_edit(
        &mut self,
        target: Option<SettingsPickerTarget>,
    ) {
        match target.unwrap_or(SettingsPickerTarget::Model) {
            SettingsPickerTarget::AudioSttModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(18);
                self.settings
                    .start_editing("feat_audio_stt_model", &self.config.audio_stt_model());
                self.status_line = "Enter STT model ID".to_string();
            }
            SettingsPickerTarget::AudioTtsModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(21);
                self.settings
                    .start_editing("feat_audio_tts_model", &self.config.audio_tts_model());
                self.status_line = "Enter TTS model ID".to_string();
            }
            SettingsPickerTarget::ImageGenerationModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(24);
                self.settings.start_editing(
                    "feat_image_generation_model",
                    &self.config.image_generation_model(),
                );
                self.status_line = "Enter image generation model ID".to_string();
            }
            SettingsPickerTarget::EmbeddingModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(27);
                self.settings.start_editing(
                    "feat_embedding_model",
                    &self.config.semantic_embedding_model(),
                );
                self.status_line = "Enter embedding model ID".to_string();
            }
            SettingsPickerTarget::BuiltinPersonaModel => {
                self.status_line =
                    "Custom model entry is not available for builtin persona setup".to_string();
            }
            SettingsPickerTarget::CompactionWelesModel => self.settings.start_editing(
                "compaction_weles_model",
                &self.config.compaction_weles_model,
            ),
            SettingsPickerTarget::CompactionCustomModel => self.settings.start_editing(
                "compaction_custom_model",
                &self.config.compaction_custom_model,
            ),
            SettingsPickerTarget::SubAgentModel => {
                let Some(editor) = self.subagents.editor.as_ref() else {
                    self.status_line = "No sub-agent editor is active".to_string();
                    return;
                };
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
                self.settings.start_editing("subagent_model", &editor.model);
                self.status_line = "Enter sub-agent model ID".to_string();
            }
            SettingsPickerTarget::ConciergeModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
                self.settings.start_editing(
                    "concierge_model",
                    self.concierge.model.as_deref().unwrap_or(""),
                );
                self.status_line = "Enter Rarog model ID".to_string();
            }
            SettingsPickerTarget::Model if self.modal.top() == Some(modal::ModalKind::Settings) => {
                self.begin_custom_model_edit();
            }
            SettingsPickerTarget::Model | SettingsPickerTarget::TargetAgentModel => {
                self.open_active_thread_custom_model_editor();
            }
            _ => self.begin_custom_model_edit(),
        }
    }

    pub(crate) fn whatsapp_linking_allowed(&self) -> bool {
        zorai_protocol::has_whatsapp_allowed_contacts(&self.config.whatsapp_allowed_contacts)
    }
}
