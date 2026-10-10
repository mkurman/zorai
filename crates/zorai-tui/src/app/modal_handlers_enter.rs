use super::*;
use crate::app::commands::{
    apply_active_thread_provider_model_to_daemon, apply_target_agent_custom_model_locally,
    push_active_thread_execution_profile_to_daemon, GoalActionPickerItem,
};
use crate::state::config::embedding_dimensions_from_fetched_model;
use zorai_shared::providers::PROVIDER_ID_CUSTOM;

fn split_openrouter_provider_csv(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for item in value.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() || out.iter().any(|existing| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn join_openrouter_provider_csv(values: &[String]) -> String {
    values.join(", ")
}

fn toggle_openrouter_provider_slug(value: &str, slug: &str) -> (String, bool) {
    let mut values = split_openrouter_provider_csv(value);
    if let Some(index) = values.iter().position(|item| item == slug) {
        values.remove(index);
        return (join_openrouter_provider_csv(&values), false);
    }
    values.push(slug.to_string());
    (join_openrouter_provider_csv(&values), true)
}

fn remove_openrouter_provider_slug(value: &str, slug: &str) -> String {
    let values = split_openrouter_provider_csv(value)
        .into_iter()
        .filter(|item| item != slug)
        .collect::<Vec<_>>();
    join_openrouter_provider_csv(&values)
}

fn apply_target_agent_reasoning_effort_locally(
    model: &mut TuiModel,
    target_agent_id: &str,
    target_agent_name: &str,
    reasoning_effort: Option<String>,
) {
    if target_agent_id.eq_ignore_ascii_case(zorai_protocol::AGENT_ID_RAROG) {
        model.concierge.reasoning_effort = reasoning_effort.clone();
    }

    for entry in &mut model.subagents.entries {
        let id_matches = entry.id.eq_ignore_ascii_case(target_agent_id)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(target_agent_id));
        let name_matches = entry.name.eq_ignore_ascii_case(target_agent_name)
            || entry.name.eq_ignore_ascii_case(target_agent_id);
        if id_matches || name_matches {
            entry.reasoning_effort = reasoning_effort.clone();
        }
    }

    if let Some(thread) = model.chat.active_thread_mut() {
        thread.runtime_reasoning_effort = reasoning_effort;
    }
}

fn apply_active_svarog_provider_model_locally(
    model: &mut TuiModel,
    provider_id: &str,
    model_id: &str,
    context_window_tokens: Option<u32>,
) {
    let Some(thread) = model.chat.active_thread_mut() else {
        return;
    };

    thread.profile_provider = Some(provider_id.to_string());
    thread.profile_model = Some(model_id.to_string());
    thread.profile_context_window_tokens = context_window_tokens;
    thread.runtime_provider = Some(provider_id.to_string());
    thread.runtime_model = Some(model_id.to_string());
}

pub(super) fn begin_custom_model_edit(model: &mut TuiModel) {
    let current = if model.config.custom_model_name.trim().is_empty()
        || model.config.custom_model_name == model.config.model
    {
        model.config.model.clone()
    } else {
        format!(
            "{} | {}",
            model.config.custom_model_name, model.config.model
        )
    };
    if model.modal.top() != Some(modal::ModalKind::Settings) {
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    }
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    model.settings_navigate_to(3);
    model.settings.start_editing("custom_model_entry", &current);
    model.status_line = "Enter custom model as `Name | ID` or just `ID`".to_string();
}

fn json_array_contains_modality(value: Option<&serde_json::Value>, modality: &str) -> bool {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .map(str::trim)
                    .map(|value| value.eq_ignore_ascii_case(modality))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn json_string_has_modality(value: Option<&serde_json::Value>, modality: &str) -> bool {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase().contains(modality))
        .unwrap_or(false)
}

fn modality_side_has_term(value: Option<&serde_json::Value>, side: &str, modality: &str) -> bool {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            let Some((input, output)) = lowered.split_once("->") else {
                return false;
            };
            let directional = match side {
                "input" => input,
                "output" => output,
                _ => return false,
            };
            directional
                .split(|ch: char| matches!(ch, '+' | ',' | '|' | '/' | ' '))
                .any(|token| token.trim() == modality)
        })
        .unwrap_or(false)
}

fn selected_model_supports_image_input(model: &crate::state::config::FetchedModel) -> bool {
    let metadata = model.metadata.as_ref();
    json_array_contains_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/input_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        "image",
    ) || json_string_has_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/modality"))
            .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        "image",
    )
}

fn selected_model_supports_audio(model: &crate::state::config::FetchedModel) -> bool {
    let metadata = model.metadata.as_ref();
    json_array_contains_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/input_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities"))),
        "audio",
    ) || modality_side_has_term(
        metadata
            .and_then(|value| value.pointer("/architecture/modality"))
            .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        "input",
        "audio",
    )
}

fn enable_vision_tool(model: &mut TuiModel) {
    model.config.tool_vision = true;
    if let Some(raw) = model.config.agent_config_raw.as_mut() {
        if raw.get("tools").is_none() {
            raw["tools"] = serde_json::json!({});
        }
        raw["tools"]["vision"] = serde_json::Value::Bool(true);
    }
    model.send_daemon_command(DaemonCommand::SetConfigItem {
        key_path: "/tools/vision".to_string(),
        value_json: "true".to_string(),
    });
}

pub(super) fn handle_modal_enter(model: &mut TuiModel, kind: modal::ModalKind) {
    tracing::info!("handle_modal_enter: {:?}", kind);
    match kind {
        modal::ModalKind::CommandPalette => {
            let cmd_name = model
                .modal
                .selected_command()
                .map(|cmd| cmd.command.clone());
            let selection_active = model.modal.command_palette_has_explicit_selection();
            let query = model.modal.command_query().trim().to_string();
            tracing::info!(
                "selected_command: {:?}, cursor: {}, filtered: {:?}",
                cmd_name,
                model.modal.picker_cursor(),
                model.modal.filtered_items()
            );
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            let query_head = query
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if selection_active {
                if let Some(command) = cmd_name {
                    model.execute_command(&command);
                }
            } else if !query.is_empty()
                && !query_head.is_empty()
                && (cmd_name.as_deref() == Some(query_head) || model.is_builtin_command(query_head))
            {
                let command_line = if query.starts_with('/') {
                    query
                } else {
                    format!("/{query}")
                };
                model.execute_slash_command_line(&command_line);
            } else if let Some(command) = cmd_name {
                model.execute_command(&command);
            }
        }
        modal::ModalKind::OperatorProfileOnboarding => {
            model.sync_operator_profile_onboarding_item_count();
            let view = model.operator_profile_onboarding_view();
            if let Some(target) = widgets::operator_profile_onboarding::target_at_index(
                &view,
                model.modal.picker_cursor(),
            ) {
                let _ = model.execute_operator_profile_onboarding_target(target);
            }
        }
        modal::ModalKind::ThreadPicker => {
            let cursor = model.modal.picker_cursor();
            let thread_picker_tab = model.modal.thread_picker_tab();
            if cursor == 0 {
                if thread_picker_tab == modal::ThreadPickerTab::Goals {
                    model.status_line = "Goal threads are created automatically".to_string();
                    return;
                }
                if thread_picker_tab == modal::ThreadPickerTab::Workspace {
                    model.status_line =
                        "Workspace threads are created from workspace tasks".to_string();
                    return;
                }
                if thread_picker_tab == modal::ThreadPickerTab::Playgrounds {
                    model.status_line = "Playgrounds are created automatically".to_string();
                    return;
                }
                if thread_picker_tab == modal::ThreadPickerTab::Gateway {
                    model.status_line = "Gateway threads are created automatically".to_string();
                    return;
                }
                model.close_top_modal();
                model.input.reduce(input::InputAction::Clear);
                let target_agent_id = TuiModel::thread_picker_target_agent_id(thread_picker_tab);
                model.start_new_thread_view_for_agent(target_agent_id.as_deref());
                model.status_line = "New conversation".to_string();
            } else if let Some((tid, title)) =
                widgets::thread_picker::filtered_threads_for_workspace(
                    &model.chat,
                    &model.modal,
                    &model.subagents,
                    &model.tasks,
                    &model.workspace,
                )
                .get(cursor - 1)
                .map(|thread| {
                    (
                        thread.id.clone(),
                        widgets::thread_picker::thread_display_title_for_workspace(
                            thread,
                            &model.tasks,
                            &model.workspace,
                        ),
                    )
                })
            {
                model.close_top_modal();
                model.input.reduce(input::InputAction::Clear);
                model.open_thread_conversation(tid);
                model.status_line = format!("Thread: {}", title);
            }
        }
        modal::ModalKind::GoalPicker => {
            let cursor = model.modal.picker_cursor();
            let selected = if cursor == 0 {
                None
            } else {
                model.filtered_goal_runs().get(cursor - 1).map(|run| {
                    sidebar::SidebarItemTarget::GoalRun {
                        goal_run_id: run.id.clone(),
                        step_id: None,
                    }
                })
            };
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            if cursor == 0 {
                model.open_new_goal_view();
            } else if let Some(target) = selected {
                model.open_sidebar_target(target);
            } else {
                model.status_line = "No goals available".to_string();
            }
        }
        modal::ModalKind::WorkspacePicker => {
            model.submit_workspace_picker();
        }
        modal::ModalKind::WorkspaceCreate => {
            model.submit_workspace_create_workspace_modal();
        }
        modal::ModalKind::WorkspaceCreateTask => {
            model.submit_workspace_create_modal();
        }
        modal::ModalKind::WorkspaceReviewTask => {
            model.submit_workspace_review_modal();
        }
        modal::ModalKind::WorkspaceEditTask => {
            model.submit_workspace_edit_modal();
        }
        modal::ModalKind::WorkspaceTaskDetail => {
            model.close_top_modal();
        }
        modal::ModalKind::WorkspaceTaskHistory => {
            model.submit_workspace_history_modal();
        }
        modal::ModalKind::WorkspaceActorPicker => {
            model.submit_workspace_actor_picker();
        }
        modal::ModalKind::ThreadParticipants => {
            model.handle_thread_participants_modal_enter();
        }
        modal::ModalKind::ThreadHandoff => {
            model.submit_thread_handoff_modal_action();
        }
        modal::ModalKind::ThreadParticipantAgentPicker => {
            model.submit_thread_participant_agent_picker();
        }
        modal::ModalKind::ThreadParticipantActions => {
            model.submit_thread_participant_actions();
        }
        modal::ModalKind::GatewayDefaultAgentPicker => {
            model.submit_gateway_default_agent_picker();
        }
        modal::ModalKind::GoalStepActionPicker => {
            let cursor = model.modal.picker_cursor();
            let items = model.goal_action_picker_items();
            model.close_top_modal();
            match items.get(cursor).copied() {
                Some(GoalActionPickerItem::PauseGoal) => {
                    if !model.request_selected_goal_run_toggle_confirmation() {
                        model.status_line = "Goal action is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::ResumeGoal) => {
                    if !model.request_selected_goal_run_toggle_confirmation() {
                        model.status_line = "Goal action is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::StopGoal) => {
                    if !model.request_selected_goal_run_stop_confirmation() {
                        model.status_line = "Goal action is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::DeleteGoal) => {
                    if let Some(run) = model.selected_goal_run() {
                        model.open_pending_action_confirm(PendingConfirmAction::DeleteGoalRun {
                            goal_run_id: run.id.clone(),
                            title: run.title.clone(),
                        });
                    } else {
                        model.status_line = "Goal action is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::AcceptReview) => {
                    if !model.request_goal_review_verdict_confirmation("accept") {
                        model.status_line = "Goal is not awaiting review".to_string();
                    }
                }
                Some(GoalActionPickerItem::SoftReject) => {
                    if !model.request_goal_review_verdict_confirmation("soft_reject") {
                        model.status_line = "Goal is not awaiting review".to_string();
                    }
                }
                Some(GoalActionPickerItem::HardReject) => {
                    if !model.request_goal_review_verdict_confirmation("hard_reject") {
                        model.status_line = "Goal is not awaiting review".to_string();
                    }
                }
                Some(GoalActionPickerItem::CycleRuntimeAssignment) => {
                    if !model.cycle_selected_runtime_assignment() {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::EditRuntimeProvider) => {
                    if !model.stage_mission_control_assignment_modal_edit(
                        goal_mission_control::RuntimeAssignmentEditField::Provider,
                    ) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::EditRuntimeModel) => {
                    if !model.stage_mission_control_assignment_modal_edit(
                        goal_mission_control::RuntimeAssignmentEditField::Model,
                    ) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::EditRuntimeReasoning) => {
                    if !model.stage_mission_control_assignment_modal_edit(
                        goal_mission_control::RuntimeAssignmentEditField::ReasoningEffort,
                    ) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::EditRuntimeRole) => {
                    if !model.stage_mission_control_assignment_modal_edit(
                        goal_mission_control::RuntimeAssignmentEditField::Role,
                    ) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::ToggleRuntimeEnabled) => {
                    if !model.update_selected_runtime_assignment(|assignment| {
                        assignment.enabled = !assignment.enabled;
                    }) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::ToggleRuntimeInherit) => {
                    if !model.update_selected_runtime_assignment(|assignment| {
                        assignment.inherit_from_main = !assignment.inherit_from_main;
                    }) {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                }
                Some(GoalActionPickerItem::ApplyRuntimeNextTurn) => {
                    model.open_pending_action_confirm(PendingConfirmAction::ReuseModelAsStt {
                        model_id: "__mission_control__:next_turn".to_string(),
                    });
                }
                Some(GoalActionPickerItem::ApplyRuntimeReassignActiveStep) => {
                    model.open_pending_action_confirm(PendingConfirmAction::ReuseModelAsStt {
                        model_id: "__mission_control__:reassign_active_step".to_string(),
                    });
                }
                Some(GoalActionPickerItem::ApplyRuntimeRestartActiveStep) => {
                    model.open_pending_action_confirm(PendingConfirmAction::ReuseModelAsStt {
                        model_id: "__mission_control__:restart_active_step".to_string(),
                    });
                }
                None => {
                    model.status_line = "Goal action is unavailable".to_string();
                }
            }
        }
        modal::ModalKind::QueuedPrompts => {
            model.execute_selected_queued_prompt_action();
        }
        modal::ModalKind::PinnedBudgetExceeded => {
            model.close_pinned_budget_exceeded_modal();
        }
        modal::ModalKind::ProviderPicker => {
            if let Some(edit) = model.goal_mission_control.pending_runtime_edit.clone() {
                let cursor = model.modal.picker_cursor();
                let provider_defs = model.filtered_provider_picker_defs();
                if let Some(def) = provider_defs.get(cursor) {
                    let next_provider = def.id.to_string();
                    let (_, _, auth_source) = model.provider_auth_snapshot(def.id);
                    let next_model =
                        providers::default_model_for_provider_auth(def.id, &auth_source);
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    let updated = model.update_selected_runtime_assignment(|assignment| {
                        if edit.field == goal_mission_control::RuntimeAssignmentEditField::Provider
                        {
                            assignment.provider = next_provider.clone();
                            if !next_model.trim().is_empty() {
                                assignment.model = next_model.clone();
                            }
                        }
                    });
                    if !updated {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                } else {
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                }
                return;
            }
            let cursor = model.modal.picker_cursor();
            let provider_defs = model.filtered_provider_picker_defs();
            if let Some(def) = provider_defs.get(cursor) {
                match model
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Provider)
                {
                    SettingsPickerTarget::Provider => {
                        model.apply_provider_selection(def.id);
                        model.settings_picker_target = None;
                        model.close_top_modal();
                        if model.config.provider == PROVIDER_ID_CUSTOM {
                            model.settings_navigate_to(3);
                        } else {
                            model.open_provider_backed_model_picker(
                                SettingsPickerTarget::Model,
                                model.config.provider.clone(),
                                model.config.base_url.clone(),
                                model.config.api_key.clone(),
                                model.config.auth_source.clone(),
                            );
                        }
                        return;
                    }
                    SettingsPickerTarget::AudioSttProvider => {
                        let current_model = model.config.audio_stt_model();
                        let known_models = TuiModel::audio_catalog_models("stt", def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_audio_model_for("stt", def.id)
                        } else {
                            current_model
                        };
                        model.set_audio_config_string("stt", "provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_audio_config_string("stt", "model", next_model);
                        }
                        model.close_top_modal();
                        model.open_audio_model_picker("stt");
                        model.status_line = format!("STT provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::AudioTtsProvider => {
                        let current_model = model.config.audio_tts_model();
                        let known_models = TuiModel::audio_catalog_models("tts", def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_audio_model_for("tts", def.id)
                        } else {
                            current_model
                        };
                        model.set_audio_config_string("tts", "provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_audio_config_string("tts", "model", next_model);
                        }
                        model.close_top_modal();
                        model.open_audio_model_picker("tts");
                        model.status_line = format!("TTS provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::ImageGenerationProvider => {
                        let current_model = model.config.image_generation_model();
                        let known_models = TuiModel::image_generation_catalog_models(def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_image_generation_model_for(def.id)
                        } else {
                            current_model
                        };
                        model.set_image_generation_config_string("provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_image_generation_config_string("model", next_model);
                        }
                        model.close_top_modal();
                        model.open_image_generation_model_picker();
                        model.status_line = format!("Image provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::EmbeddingProvider => {
                        let current_model = model.config.semantic_embedding_model();
                        let known_models = TuiModel::embedding_catalog_models(def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_embedding_model_for(def.id)
                        } else {
                            current_model
                        };
                        model.set_embedding_config_string("provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_embedding_config_string("model", next_model);
                        }
                        model.close_top_modal();
                        model.open_embedding_model_picker();
                        model.status_line = format!("Embedding provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::BuiltinPersonaProvider => {
                        model.apply_provider_selection_without_sync(def.id);
                        model.close_top_modal();
                        if model.config.provider == PROVIDER_ID_CUSTOM {
                            model.status_line =
                                "Custom provider setup for builtin personas is not supported here"
                                    .to_string();
                            model.restore_builtin_persona_setup_config_snapshot();
                            model.pending_builtin_persona_setup = None;
                            model.settings_picker_target = None;
                            return;
                        }
                        model.open_provider_backed_model_picker(
                            SettingsPickerTarget::BuiltinPersonaModel,
                            model.config.provider.clone(),
                            model.config.base_url.clone(),
                            model.config.api_key.clone(),
                            model.config.auth_source.clone(),
                        );
                        if let Some(setup) = model.pending_builtin_persona_setup.as_ref() {
                            model.status_line =
                                format!("Configure {} model", setup.target_agent_name);
                        }
                        return;
                    }
                    SettingsPickerTarget::CompactionWelesProvider => {
                        let (_, _, auth_source) = model.provider_auth_snapshot(def.id);
                        model.config.compaction_weles_provider = def.id.to_string();
                        if model.config.compaction_weles_model.trim().is_empty()
                            || !providers::known_models_for_provider_auth(def.id, &auth_source)
                                .iter()
                                .any(|model_entry| {
                                    model_entry.id == model.config.compaction_weles_model
                                })
                        {
                            model.config.compaction_weles_model =
                                providers::default_model_for_provider_auth(def.id, &auth_source);
                        }
                        model.close_top_modal();
                        model.open_compaction_weles_model_picker();
                        model.sync_config_to_daemon();
                        return;
                    }
                    SettingsPickerTarget::CompactionCustomProvider => {
                        let (base_url, api_key, auth_source) = model.provider_auth_snapshot(def.id);
                        model.apply_compaction_custom_provider(def.id);
                        model.config.compaction_custom_base_url = base_url;
                        model.config.compaction_custom_api_key = api_key;
                        model.config.compaction_custom_auth_source = auth_source.clone();
                        if model.config.compaction_custom_model.trim().is_empty()
                            || !providers::known_models_for_provider_auth(def.id, &auth_source)
                                .iter()
                                .any(|model_entry| {
                                    model_entry.id == model.config.compaction_custom_model
                                })
                        {
                            model.config.compaction_custom_model =
                                providers::default_model_for_provider_auth(def.id, &auth_source);
                        }
                        model.normalize_compaction_custom_transport();
                        model.close_top_modal();
                        model.open_compaction_custom_model_picker();
                        model.sync_config_to_daemon();
                        return;
                    }
                    SettingsPickerTarget::SubAgentProvider => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.provider = def.id.to_string();
                            if editor.provider != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
                                editor.openrouter_provider_order.clear();
                                editor.openrouter_provider_ignore.clear();
                                editor.openrouter_allow_fallbacks = true;
                                editor.field = editor.field.next_for_provider(&editor.provider);
                            }
                            if editor.provider != zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
                                editor.huggingface_provider.clear();
                            }
                            let default_model =
                                providers::default_model_for_provider_auth(def.id, "api_key");
                            if editor.model.trim().is_empty()
                                || !providers::known_models_for_provider_auth(def.id, "api_key")
                                    .iter()
                                    .any(|model_entry| model_entry.id == editor.model)
                            {
                                editor.model = default_model;
                            }
                            editor.context_window_tokens =
                                providers::known_context_window_for(def.id, &editor.model);
                        }
                        model.status_line = format!("Sub-agent provider: {}", def.name);
                    }
                    SettingsPickerTarget::TargetAgentProvider => {
                        let target_agent_name = model
                            .pending_target_agent_config
                            .as_ref()
                            .map(|pending| pending.target_agent_name.clone())
                            .unwrap_or_else(|| "agent".to_string());
                        let (base_url, api_key, auth_source) = model.provider_auth_snapshot(def.id);
                        if let Some(pending) = model.pending_target_agent_config.as_mut() {
                            pending.provider_id = def.id.to_string();
                            let default_model =
                                providers::default_model_for_provider_auth(def.id, &auth_source);
                            if !default_model.trim().is_empty() {
                                pending.model = default_model;
                            }
                        }
                        model.close_top_modal();
                        model.open_provider_backed_model_picker(
                            SettingsPickerTarget::TargetAgentModel,
                            def.id.to_string(),
                            base_url,
                            api_key,
                            auth_source,
                        );
                        model.status_line =
                            format!("Configure {target_agent_name} model for {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::ConciergeProvider => {
                        model.concierge.provider = Some(def.id.to_string());
                        if def.id != zorai_shared::providers::PROVIDER_ID_OPENROUTER {
                            model.concierge.openrouter_provider_order.clear();
                            model.concierge.openrouter_provider_ignore.clear();
                            model.concierge.openrouter_allow_fallbacks = true;
                            model.clamp_settings_cursor();
                        }
                        if def.id != zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
                            model.concierge.huggingface_provider.clear();
                        }
                        let default_model =
                            providers::default_model_for_provider_auth(def.id, "api_key");
                        if model.concierge.model.as_deref().unwrap_or("").is_empty() {
                            model.concierge.model = Some(default_model);
                        }
                        model.send_concierge_config();
                        model.status_line = format!("Rarog provider: {}", def.name);
                    }
                    SettingsPickerTarget::Model
                    | SettingsPickerTarget::AudioSttModel
                    | SettingsPickerTarget::AudioTtsModel
                    | SettingsPickerTarget::ImageGenerationModel
                    | SettingsPickerTarget::EmbeddingModel
                    | SettingsPickerTarget::BuiltinPersonaModel
                    | SettingsPickerTarget::CompactionWelesModel
                    | SettingsPickerTarget::CompactionCustomModel
                    | SettingsPickerTarget::SubAgentModel
                    | SettingsPickerTarget::SubAgentRole
                    | SettingsPickerTarget::SubAgentReasoningEffort
                    | SettingsPickerTarget::TargetAgentModel
                    | SettingsPickerTarget::TargetAgentReasoningEffort
                    | SettingsPickerTarget::ConciergeModel
                    | SettingsPickerTarget::ConciergeReasoningEffort
                    | SettingsPickerTarget::SvarogReasoningEffort
                    | SettingsPickerTarget::CompactionWelesReasoningEffort
                    | SettingsPickerTarget::CompactionCustomReasoningEffort
                    | SettingsPickerTarget::OpenRouterPreferredProviders
                    | SettingsPickerTarget::OpenRouterExcludedProviders
                    | SettingsPickerTarget::SubAgentOpenRouterPreferredProviders
                    | SettingsPickerTarget::SubAgentOpenRouterExcludedProviders
                    | SettingsPickerTarget::ConciergeOpenRouterPreferredProviders
                    | SettingsPickerTarget::ConciergeOpenRouterExcludedProviders => {}
                }
            } else {
                model.status_line = "No authenticated providers available".to_string();
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::ModelPicker => {
            if let Some(edit) = model.goal_mission_control.pending_runtime_edit.clone() {
                let models = widgets::model_picker::filtered_models_for_selection(
                    &model.available_runtime_assignment_models(),
                    model.modal.command_query(),
                );
                let cursor = model.modal.picker_cursor();
                if cursor == models.len() {
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    model.begin_mission_control_custom_model_edit();
                    return;
                }
                if let Some(model_entry) = models.get(cursor) {
                    let next_model = model_entry.id.clone();
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    let updated = model.update_selected_runtime_assignment(|assignment| {
                        if edit.field == goal_mission_control::RuntimeAssignmentEditField::Model {
                            assignment.model = next_model.clone();
                        }
                    });
                    if !updated {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                } else {
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                }
                return;
            }
            let models = model.filtered_model_picker_models();
            let cursor = model.modal.picker_cursor();
            if cursor == models.len() {
                let picker_target = model.settings_picker_target;
                model.settings_picker_target = None;
                model.close_top_modal();
                model.begin_targeted_custom_model_edit(picker_target);
                return;
            }
            if models.is_empty() {
                model.status_line = "No models available. Set model in /settings".to_string();
            } else if let Some(model_entry) = models.get(cursor) {
                let model_id = model_entry.id.clone();
                let model_context_window = model_entry.context_window;
                let mut follow_up_confirm = None;
                match model
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::Model => {
                        model
                            .config
                            .reduce(config::ConfigAction::SetModel(model_id.clone()));
                        if providers::model_uses_context_window_override(
                            &model.config.provider,
                            &model.config.auth_source,
                            &model.config.model,
                            &model.config.custom_model_name,
                        ) {
                            model.config.custom_model_name =
                                model_entry.name.clone().unwrap_or_else(|| model_id.clone());
                            let next_context = model_context_window.unwrap_or(
                                model
                                    .config
                                    .custom_context_window_tokens
                                    .unwrap_or(providers::default_custom_model_context_window()),
                            );
                            model.config.custom_context_window_tokens = Some(next_context);
                            model.config.context_window_tokens = next_context;
                        } else {
                            model.config.custom_model_name.clear();
                            model.config.custom_context_window_tokens = None;
                            model.config.context_window_tokens =
                                model_context_window.unwrap_or(128_000);
                        }
                        let previous_transport = model.config.api_transport.clone();
                        model.config.api_transport = model.provider_transport_snapshot(
                            &model.config.provider,
                            &model.config.auth_source,
                            &model.config.model,
                            &model.config.api_transport,
                        );
                        let selected_provider = model.config.provider.clone();
                        let selected_context_window = model
                            .config
                            .custom_context_window_tokens
                            .or(model_context_window);
                        if !model.modal.contains(modal::ModalKind::Settings) {
                            apply_active_svarog_provider_model_locally(
                                model,
                                &selected_provider,
                                &model_id,
                                selected_context_window,
                            );
                            let reasoning_effort = model
                                .chat
                                .active_thread_runtime_metadata()
                                .and_then(|runtime| runtime.reasoning_effort.clone());
                            apply_active_thread_provider_model_to_daemon(
                                model,
                                &selected_provider,
                                &model_id,
                                reasoning_effort.as_deref(),
                            );
                        }
                        model.status_line = format!("Model: {}", model_id);
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(model_id.clone()))
                        {
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: "/model".to_string(),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/providers/{}/model", model.config.provider),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/model", model.config.provider),
                                value_json,
                            });
                        }
                        if previous_transport != model.config.api_transport {
                            let transport_json = serde_json::json!(model.config.api_transport);
                            if let Ok(value_json) = serde_json::to_string(&transport_json) {
                                model.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/api_transport".to_string(),
                                    value_json: value_json.clone(),
                                });
                                model.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: format!(
                                        "/providers/{}/api_transport",
                                        model.config.provider
                                    ),
                                    value_json: value_json.clone(),
                                });
                                model.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: format!("/{}/api_transport", model.config.provider),
                                    value_json,
                                });
                            }
                        }
                        if selected_model_supports_image_input(model_entry)
                            && !model.config.tool_vision
                        {
                            enable_vision_tool(model);
                        }
                        if selected_model_supports_audio(model_entry)
                            && model.config.audio_stt_model() != model_id
                        {
                            follow_up_confirm = Some(PendingConfirmAction::ReuseModelAsStt {
                                model_id: model_id.clone(),
                            });
                        }
                        model.save_settings();
                    }
                    SettingsPickerTarget::AudioSttModel => {
                        model.set_audio_config_string("stt", "model", model_id.clone());
                        model.status_line = format!("STT model: {}", model_id);
                    }
                    SettingsPickerTarget::AudioTtsModel => {
                        model.set_audio_config_string("tts", "model", model_id.clone());
                        model.status_line = format!("TTS model: {}", model_id);
                    }
                    SettingsPickerTarget::ImageGenerationModel => {
                        model.set_image_generation_config_string("model", model_id.clone());
                        model.status_line = format!("Image model: {}", model_id);
                    }
                    SettingsPickerTarget::EmbeddingModel => {
                        let embedding_dimensions =
                            embedding_dimensions_from_fetched_model(model_entry);
                        model.set_embedding_config_string("model", model_id.clone());
                        if let Some(dimensions) = embedding_dimensions {
                            model.set_embedding_dimensions_config(dimensions);
                        }
                        model.status_line = format!("Embedding model: {}", model_id);
                    }
                    SettingsPickerTarget::BuiltinPersonaModel => {
                        let Some(setup) = model.pending_builtin_persona_setup.clone() else {
                            model.status_line = "No builtin persona setup is active".to_string();
                            model.settings_picker_target = None;
                            model.close_top_modal();
                            return;
                        };
                        let selected_provider = model.config.provider.clone();
                        model.send_daemon_command(DaemonCommand::SetTargetAgentProviderModel {
                            target_agent_id: setup.target_agent_id.clone(),
                            provider_id: selected_provider.clone(),
                            model: model_id.clone(),
                        });
                        let mut raw = model
                            .config
                            .agent_config_raw
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({}));
                        if raw.get("builtin_sub_agents").is_none() {
                            raw["builtin_sub_agents"] = serde_json::json!({});
                        }
                        raw["builtin_sub_agents"][setup.target_agent_id.as_str()]["provider"] =
                            serde_json::Value::String(selected_provider.clone());
                        raw["builtin_sub_agents"][setup.target_agent_id.as_str()]["model"] =
                            serde_json::Value::String(model_id.clone());
                        model.config.agent_config_raw = Some(raw);
                        model.restore_builtin_persona_setup_config_snapshot();
                        let continuation = setup.continuation.clone();
                        model.pending_builtin_persona_setup = None;
                        model.status_line = format!(
                            "{} configured with {} / {}",
                            setup.target_agent_name, selected_provider, model_id
                        );
                        model.settings_picker_target = None;
                        model.close_top_modal();
                        match continuation {
                            PendingBuiltinPersonaSetupContinuation::SubmitPrompt(prompt) => {
                                model.submit_prompt(prompt);
                            }
                            PendingBuiltinPersonaSetupContinuation::SelectWorkspaceActor {
                                pending,
                                actor,
                            } => {
                                model.apply_workspace_actor_selection(pending, Some(actor));
                            }
                            PendingBuiltinPersonaSetupContinuation::AddThreadParticipant {
                                thread_id,
                                agent_id,
                                agent_name,
                            } => {
                                model.open_participant_instruction_editor(
                                    thread_id,
                                    agent_id,
                                    agent_name,
                                    true,
                                    String::new(),
                                );
                            }
                        }
                        return;
                    }
                    SettingsPickerTarget::CompactionWelesModel => {
                        model.config.compaction_weles_model = model_id.clone();
                        model.status_line = format!("Compaction WELES model: {}", model_id);
                        model.sync_config_to_daemon();
                    }
                    SettingsPickerTarget::CompactionCustomModel => {
                        model.config.compaction_custom_model = model_id.clone();
                        if let Some(context_window) = model_context_window {
                            model.config.compaction_custom_context_window_tokens = context_window;
                        }
                        model.normalize_compaction_custom_transport();
                        model.status_line = format!("Compaction custom model: {}", model_id);
                        model.sync_config_to_daemon();
                    }
                    SettingsPickerTarget::SubAgentModel => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.model = model_id.clone();
                            editor.context_window_tokens = model_context_window;
                        }
                        model.status_line = format!("Sub-agent model: {}", model_id);
                    }
                    SettingsPickerTarget::TargetAgentModel => {
                        let Some(pending) = model.pending_target_agent_config.clone() else {
                            model.status_line =
                                "No thread-owned agent target is active".to_string();
                            model.settings_picker_target = None;
                            model.close_top_modal();
                            return;
                        };
                        apply_target_agent_custom_model_locally(
                            model,
                            &pending.target_agent_id,
                            &pending.target_agent_name,
                            &pending.provider_id,
                            &model_id,
                        );
                        apply_active_thread_provider_model_to_daemon(
                            model,
                            &pending.provider_id,
                            &model_id,
                            pending.reasoning_effort.as_deref(),
                        );
                        if let Some(context_window) = model_context_window {
                            if let Some(thread) = model.chat.active_thread_mut() {
                                thread.profile_context_window_tokens = Some(context_window);
                            }
                        }
                        model.status_line =
                            format!("{} model: {}", pending.target_agent_name, model_id);
                        model.pending_target_agent_config = None;
                    }
                    SettingsPickerTarget::ConciergeModel => {
                        model.concierge.model = Some(model_id.clone());
                        model.send_concierge_config();
                        model.status_line = format!("Rarog model: {}", model_id);
                    }
                    SettingsPickerTarget::Provider
                    | SettingsPickerTarget::AudioSttProvider
                    | SettingsPickerTarget::AudioTtsProvider
                    | SettingsPickerTarget::ImageGenerationProvider
                    | SettingsPickerTarget::EmbeddingProvider
                    | SettingsPickerTarget::BuiltinPersonaProvider
                    | SettingsPickerTarget::CompactionWelesProvider
                    | SettingsPickerTarget::CompactionCustomProvider
                    | SettingsPickerTarget::SubAgentProvider
                    | SettingsPickerTarget::SubAgentRole
                    | SettingsPickerTarget::SubAgentReasoningEffort
                    | SettingsPickerTarget::TargetAgentProvider
                    | SettingsPickerTarget::TargetAgentReasoningEffort
                    | SettingsPickerTarget::ConciergeProvider
                    | SettingsPickerTarget::ConciergeReasoningEffort
                    | SettingsPickerTarget::SvarogReasoningEffort
                    | SettingsPickerTarget::CompactionWelesReasoningEffort
                    | SettingsPickerTarget::CompactionCustomReasoningEffort
                    | SettingsPickerTarget::OpenRouterPreferredProviders
                    | SettingsPickerTarget::OpenRouterExcludedProviders
                    | SettingsPickerTarget::SubAgentOpenRouterPreferredProviders
                    | SettingsPickerTarget::SubAgentOpenRouterExcludedProviders
                    | SettingsPickerTarget::ConciergeOpenRouterPreferredProviders
                    | SettingsPickerTarget::ConciergeOpenRouterExcludedProviders => {}
                }
                model.settings_picker_target = None;
                model.close_top_modal();
                if let Some(action) = follow_up_confirm {
                    model.open_pending_action_confirm(action);
                }
                return;
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::OpenRouterProviderPicker => {
            let cursor = model.modal.picker_cursor();
            let Some(slug) = model
                .filtered_openrouter_endpoint_providers()
                .get(cursor)
                .map(|slug| (*slug).to_string())
            else {
                model.status_line = "No OpenRouter endpoint provider selected".to_string();
                return;
            };
            match model.settings_picker_target {
                Some(SettingsPickerTarget::OpenRouterPreferredProviders) => {
                    let (next_order, added) = toggle_openrouter_provider_slug(
                        &model.config.openrouter_provider_order,
                        &slug,
                    );
                    model.config.openrouter_provider_order = next_order;
                    if added {
                        model.config.openrouter_provider_ignore = remove_openrouter_provider_slug(
                            &model.config.openrouter_provider_ignore,
                            &slug,
                        );
                    }
                    model.status_line = format!("OpenRouter preferred provider toggled: {slug}");
                    model.sync_config_to_daemon();
                }
                Some(SettingsPickerTarget::OpenRouterExcludedProviders) => {
                    let (next_ignore, added) = toggle_openrouter_provider_slug(
                        &model.config.openrouter_provider_ignore,
                        &slug,
                    );
                    model.config.openrouter_provider_ignore = next_ignore;
                    if added {
                        model.config.openrouter_provider_order = remove_openrouter_provider_slug(
                            &model.config.openrouter_provider_order,
                            &slug,
                        );
                    }
                    model.status_line = format!("OpenRouter excluded provider toggled: {slug}");
                    model.sync_config_to_daemon();
                }
                Some(SettingsPickerTarget::SubAgentOpenRouterPreferredProviders) => {
                    if let Some(editor) = model.subagents.editor.as_mut() {
                        let (next_order, added) = toggle_openrouter_provider_slug(
                            &editor.openrouter_provider_order,
                            &slug,
                        );
                        editor.openrouter_provider_order = next_order;
                        if added {
                            editor.openrouter_provider_ignore = remove_openrouter_provider_slug(
                                &editor.openrouter_provider_ignore,
                                &slug,
                            );
                        }
                        model.status_line =
                            format!("OpenRouter preferred provider toggled: {slug}");
                    }
                }
                Some(SettingsPickerTarget::SubAgentOpenRouterExcludedProviders) => {
                    if let Some(editor) = model.subagents.editor.as_mut() {
                        let (next_ignore, added) = toggle_openrouter_provider_slug(
                            &editor.openrouter_provider_ignore,
                            &slug,
                        );
                        editor.openrouter_provider_ignore = next_ignore;
                        if added {
                            editor.openrouter_provider_order = remove_openrouter_provider_slug(
                                &editor.openrouter_provider_order,
                                &slug,
                            );
                        }
                        model.status_line = format!("OpenRouter excluded provider toggled: {slug}");
                    }
                }
                Some(SettingsPickerTarget::ConciergeOpenRouterPreferredProviders) => {
                    let (next_order, added) = toggle_openrouter_provider_slug(
                        &model.concierge.openrouter_provider_order,
                        &slug,
                    );
                    model.concierge.openrouter_provider_order = next_order;
                    if added {
                        model.concierge.openrouter_provider_ignore =
                            remove_openrouter_provider_slug(
                                &model.concierge.openrouter_provider_ignore,
                                &slug,
                            );
                    }
                    model.status_line = format!("OpenRouter preferred provider toggled: {slug}");
                    model.send_concierge_config();
                }
                Some(SettingsPickerTarget::ConciergeOpenRouterExcludedProviders) => {
                    let (next_ignore, added) = toggle_openrouter_provider_slug(
                        &model.concierge.openrouter_provider_ignore,
                        &slug,
                    );
                    model.concierge.openrouter_provider_ignore = next_ignore;
                    if added {
                        model.concierge.openrouter_provider_order = remove_openrouter_provider_slug(
                            &model.concierge.openrouter_provider_order,
                            &slug,
                        );
                    }
                    model.status_line = format!("OpenRouter excluded provider toggled: {slug}");
                    model.send_concierge_config();
                }
                _ => {
                    model.status_line =
                        "OpenRouter provider picker target is unavailable".to_string();
                }
            }
            model
                .modal
                .set_picker_item_count(model.filtered_openrouter_endpoint_providers().len());
        }
        modal::ModalKind::OpenAIAuth => {
            if let Some(url) = model.openai_auth_url.clone() {
                if crate::auth::open_external_url(&url).is_ok() {
                    model.status_line = "Opened ChatGPT login in browser".to_string();
                } else if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(url);
                    model.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                }
            }
        }
        modal::ModalKind::RolePicker => {
            if let Some(edit) = model.goal_mission_control.pending_runtime_edit.clone() {
                let cursor = model.modal.picker_cursor();
                if cursor == crate::state::subagents::role_picker_custom_index() {
                    let current = model
                        .goal_mission_control
                        .display_role_assignments()
                        .get(edit.row_index)
                        .map(|assignment| assignment.role_id.clone())
                        .unwrap_or_default();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    if model.modal.top() != Some(modal::ModalKind::Settings) {
                        model
                            .modal
                            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                    }
                    model
                        .settings
                        .start_editing("mission_control_assignment_role", &current);
                    model.status_line = "Enter Mission Control role ID".to_string();
                    return;
                }
                if let Some(choice) = crate::state::subagents::role_picker_choice(cursor) {
                    let role_id = choice.id.to_string();
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    let updated = model.update_selected_runtime_assignment(|assignment| {
                        if edit.field == goal_mission_control::RuntimeAssignmentEditField::Role {
                            assignment.role_id = role_id.clone();
                        }
                    });
                    if !updated {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                } else {
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                }
                return;
            }
            let cursor = model.modal.picker_cursor();
            if cursor == crate::state::subagents::role_picker_custom_index() {
                let current = model
                    .subagents
                    .editor
                    .as_ref()
                    .map(|editor| editor.role.clone())
                    .unwrap_or_default();
                model.settings_picker_target = None;
                model.close_top_modal();
                model.settings.start_editing("subagent_role", &current);
                model.status_line = "Enter sub-agent role ID".to_string();
                return;
            }

            if let Some(choice) = crate::state::subagents::role_picker_choice(cursor) {
                if let Some(editor) = model.subagents.editor.as_mut() {
                    match choice.kind {
                        crate::state::subagents::RolePickerChoiceKind::Preset => {
                            editor.apply_role_preset_by_index(cursor);
                        }
                        crate::state::subagents::RolePickerChoiceKind::Persona => {
                            editor.role = choice.id.to_string();
                            editor.previous_role_preset = None;
                        }
                    }
                }
                model.status_line = format!("Sub-agent role: {}", choice.label);
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::EffortPicker => {
            if let Some(edit) = model.goal_mission_control.pending_runtime_edit.clone() {
                let efforts = ["", "minimal", "low", "medium", "high", "xhigh", "max"];
                let cursor = model.modal.picker_cursor();
                if let Some(&effort) = efforts.get(cursor) {
                    let next_effort = (!effort.is_empty()).then(|| effort.to_string());
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                    let updated = model.update_selected_runtime_assignment(|assignment| {
                        if edit.field
                            == goal_mission_control::RuntimeAssignmentEditField::ReasoningEffort
                        {
                            assignment.reasoning_effort = next_effort.clone();
                        }
                    });
                    if !updated {
                        model.status_line = "Mission Control roster is unavailable".to_string();
                    }
                } else {
                    model.goal_mission_control.clear_runtime_edit();
                    model.settings_picker_target = None;
                    model.close_top_modal();
                }
                return;
            }
            let efforts = ["", "minimal", "low", "medium", "high", "xhigh", "max"];
            let cursor = model.modal.picker_cursor();
            if let Some(&effort) = efforts.get(cursor) {
                match model.settings_picker_target {
                    Some(SettingsPickerTarget::SubAgentReasoningEffort) => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.reasoning_effort = if effort.is_empty() {
                                Some("none".to_string())
                            } else {
                                Some(effort.to_string())
                            };
                        }
                        model.status_line = if effort.is_empty() {
                            "Sub-agent effort: none".to_string()
                        } else {
                            format!("Sub-agent effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::TargetAgentReasoningEffort) => {
                        let Some(pending) = model.pending_target_agent_config.clone() else {
                            model.status_line =
                                "No thread-owned agent target is active".to_string();
                            model.settings_picker_target = None;
                            model.close_top_modal();
                            return;
                        };
                        let reasoning_effort = effort.to_string();
                        model.send_daemon_command(DaemonCommand::SetTargetAgentReasoningEffort {
                            target_agent_id: pending.target_agent_id.clone(),
                            reasoning_effort: reasoning_effort.clone(),
                        });
                        apply_target_agent_reasoning_effort_locally(
                            model,
                            &pending.target_agent_id,
                            &pending.target_agent_name,
                            (!reasoning_effort.is_empty()).then_some(reasoning_effort.clone()),
                        );
                        model.status_line = if effort.is_empty() {
                            format!("{} effort: none", pending.target_agent_name)
                        } else {
                            format!("{} effort: {}", pending.target_agent_name, effort)
                        };
                        model.pending_target_agent_config = None;
                    }
                    Some(SettingsPickerTarget::ConciergeReasoningEffort) => {
                        model.concierge.reasoning_effort = if effort.is_empty() {
                            None
                        } else {
                            Some(effort.to_string())
                        };
                        model.send_concierge_config();
                        model.status_line = if effort.is_empty() {
                            "Rarog effort: none".to_string()
                        } else {
                            format!("Rarog effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::CompactionWelesReasoningEffort) => {
                        model.config.compaction_weles_reasoning_effort = if effort.is_empty() {
                            "none".to_string()
                        } else {
                            effort.to_string()
                        };
                        model.sync_config_to_daemon();
                        model.status_line = if effort.is_empty() {
                            "Compaction WELES effort: none".to_string()
                        } else {
                            format!("Compaction WELES effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::CompactionCustomReasoningEffort) => {
                        model.config.compaction_custom_reasoning_effort = if effort.is_empty() {
                            "none".to_string()
                        } else {
                            effort.to_string()
                        };
                        model.sync_config_to_daemon();
                        model.status_line = if effort.is_empty() {
                            "Compaction custom effort: none".to_string()
                        } else {
                            format!("Compaction custom effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::SvarogReasoningEffort) => {
                        model.set_pending_svarog_reasoning_effort(effort.to_string());
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(effort.to_string()))
                        {
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: "/reasoning_effort".to_string(),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!(
                                    "/providers/{}/reasoning_effort",
                                    model.config.provider
                                ),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/reasoning_effort", model.config.provider),
                                value_json,
                            });
                        }
                        model.status_line = if effort.is_empty() {
                            "Svarog effort: none".to_string()
                        } else {
                            format!("Svarog effort: {}", effort)
                        };
                        model.save_settings();
                    }
                    _ => {
                        let profile = model.current_conversation_agent_profile();
                        let next_effort = (!effort.is_empty()).then(|| effort.to_string());
                        if let Some(thread) = model.chat.active_thread_mut() {
                            thread.profile_reasoning_effort = next_effort.clone();
                            thread.runtime_reasoning_effort = next_effort.clone();
                            push_active_thread_execution_profile_to_daemon(
                                model,
                                &profile.provider,
                                &profile.model,
                                next_effort.as_deref(),
                            );
                            model.status_line = if effort.is_empty() {
                                "Thread effort: none".to_string()
                            } else {
                                format!("Thread effort: {}", effort)
                            };
                        } else {
                            model.status_line =
                                "No active thread; change the default in Settings > Svarog"
                                    .to_string();
                        }
                    }
                }
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::ContextWindowEditor => {
            model.commit_active_thread_context_window_editor();
        }
        modal::ModalKind::CustomModelEditor => {
            model.commit_active_thread_custom_model_editor();
        }
        modal::ModalKind::WhatsAppLink => {}
        _ => {
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
        }
    }
}
