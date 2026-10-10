use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
use zorai_shared::providers::*;
#[test]
fn thread_picker_playgrounds_new_row_is_browse_only() {
    let (mut model, _daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "playground:domowoj:thread-user".into(),
            title: "Participant Playground · Domowoj @ thread-user".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Playgrounds);
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ThreadPicker));
    assert_eq!(model.chat.active_thread_id(), None);
    assert_eq!(model.status_line, "Playgrounds are created automatically");
}

#[test]
fn thread_picker_new_conversation_uses_dynamic_agent_tab_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Agent("domowoj".to_string()));
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);

    model.submit_prompt("look around".to_string());

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::SendMessage {
                thread_id,
                target_agent_id,
                content,
                ..
            }) => {
                assert_eq!(thread_id, None);
                assert_eq!(target_agent_id.as_deref(), Some("domowoj"));
                assert_eq!(content, "look around");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn new_weles_conversation_uses_weles_profile_before_first_prompt() {
    let (mut model, _daemon_rx) = make_model();
    model.config.provider = "openai".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.custom_model_name.clear();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-5".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "WELES",
            "provider": "anthropic",
            "model": "claude-sonnet-4-5",
            "reasoning_effort": "medium"
        })),
    });

    model.start_new_thread_view_for_agent(Some("weles"));

    let profile = model.current_conversation_agent_profile();
    assert_eq!(profile.agent_label, "Weles");
    assert_eq!(profile.provider, "anthropic");
    assert_eq!(profile.model, "claude-sonnet-4-5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("medium"));
}

#[test]
fn new_weles_conversation_keeps_weles_profile_after_first_prompt_locally() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.config.provider = "openai".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.custom_model_name.clear();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-5".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("weles"));
    model.submit_prompt("review this diff".to_string());

    let profile = model.current_conversation_agent_profile();
    assert_eq!(profile.agent_label, "Weles");
    assert_eq!(profile.provider, "anthropic");
    assert_eq!(profile.model, "claude-sonnet-4-5");
}

pub(super) fn seed_active_weles_thread(model: &mut TuiModel) {
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.model = "gpt-5.4".to_string();
    model.auth.loaded = true;
    model
        .auth
        .entries
        .push(crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        });
    model.subagents.entries.push(crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4".to_string(),
        role: Some("review".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    });
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-weles".to_string(),
            agent_name: Some("Weles".to_string()),
            profile_provider: Some(PROVIDER_ID_OPENAI.to_string()),
            profile_model: Some("gpt-5.4".to_string()),
            profile_reasoning_effort: Some("medium".to_string()),
            title: "Weles thread".to_string(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-weles".to_string()));
}

pub(super) fn seed_active_svarog_thread(model: &mut TuiModel) {
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.model = "gpt-5.4".to_string();
    model.auth.loaded = true;
    model
        .auth
        .entries
        .push(crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        });
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-svarog".to_string(),
            agent_name: Some("Swarog".to_string()),
            profile_provider: Some(PROVIDER_ID_OPENAI.to_string()),
            profile_model: Some("gpt-5.4".to_string()),
            profile_reasoning_effort: Some("high".to_string()),
            title: "Svarog thread".to_string(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-svarog".to_string()));
}

fn seed_dola_subagent(model: &mut TuiModel) {
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.model = "gpt-5.4".to_string();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "dola".to_string(),
        name: "Dola".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4".to_string(),
        role: Some("specialist".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("high".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    });
}

#[test]
fn slash_new_without_args_uses_active_thread_owner() {
    let (mut model, _daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);

    assert!(model.execute_slash_command_line("/new"));

    let profile = model.current_conversation_agent_profile();
    assert_eq!(profile.agent_label, "Weles");
}

#[test]
fn slash_effort_on_pending_subagent_thread_updates_that_subagent() {
    let (mut model, mut daemon_rx) = make_model();
    seed_dola_subagent(&mut model);
    model.start_new_thread_view_for_agent(Some("dola"));

    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("high")
    );
    assert!(model.execute_slash_command_line("/effort"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
    assert_eq!(model.modal.picker_cursor(), 4);
    model.modal.reduce(modal::ModalAction::Navigate(-2));
    model.handle_modal_enter(modal::ModalKind::EffortPicker);

    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("low")
    );
    assert_eq!(
        model
            .subagents
            .entries
            .iter()
            .find(|entry| entry.id == "dola")
            .and_then(|entry| entry.reasoning_effort.as_deref()),
        Some("low")
    );
    let mut saw_dola_update = false;
    let mut saw_svarog_update = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetTargetAgentReasoningEffort {
                target_agent_id,
                reasoning_effort,
            } if target_agent_id == "dola" && reasoning_effort == "low" => {
                saw_dola_update = true;
            }
            DaemonCommand::SetTargetAgentReasoningEffort {
                target_agent_id, ..
            } if target_agent_id == zorai_protocol::AGENT_ID_SWAROG => {
                saw_svarog_update = true;
            }
            _ => {}
        }
    }
    assert!(saw_dola_update);
    assert!(!saw_svarog_update);
}

#[test]
fn slash_model_updates_active_thread_owner_model() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);

    assert!(model.execute_slash_command_line("/model"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let target_index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == "gpt-5.4-mini")
        .expect("expected OpenAI mini model");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(target_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    let mut saw_target_update = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SetTargetAgentProviderModel {
                target_agent_id,
                provider_id,
                model,
            } if target_agent_id == "weles"
                && provider_id == PROVIDER_ID_OPENAI
                && model == "gpt-5.4-mini"
        ) {
            saw_target_update = true;
        }
    }
    assert!(saw_target_update);
    assert!(daemon_rx.try_recv().is_err());
    let thread = model
        .chat
        .active_thread()
        .expect("weles thread should stay active");
    assert_eq!(thread.runtime_model.as_deref(), Some("gpt-5.4-mini"));
    assert_eq!(thread.profile_model.as_deref(), Some("gpt-5.4-mini"));
}

#[test]
fn slash_model_custom_entry_updates_active_thread_owner_model() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);

    assert!(model.execute_slash_command_line("/model"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let custom_index = model.filtered_model_picker_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    assert_eq!(model.modal.top(), Some(modal::ModalKind::CustomModelEditor));
    assert_ne!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(
        model.settings.editing_field(),
        Some("thread_owner_custom_model")
    );
    assert!(model.custom_model_editor_body().contains("Pinned to Weles"));
    while !model.settings.edit_buffer().is_empty() {
        model.handle_key_modal(
            KeyCode::Backspace,
            KeyModifiers::NONE,
            modal::ModalKind::CustomModelEditor,
        );
    }
    for ch in "vendor/custom-model".chars() {
        model.handle_key_modal(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
            modal::ModalKind::CustomModelEditor,
        );
    }
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::CustomModelEditor,
    );

    let mut saw_target_update = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SetTargetAgentProviderModel {
                target_agent_id,
                provider_id,
                model,
            } if target_agent_id == "weles"
                && provider_id == PROVIDER_ID_OPENAI
                && model == "vendor/custom-model"
        ) {
            saw_target_update = true;
        }
    }
    assert!(saw_target_update);
    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.runtime_model.as_deref()),
        Some("vendor/custom-model")
    );
}

#[test]
fn slash_provider_updates_active_thread_owner_provider_after_model_pick() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model
        .auth
        .entries
        .push(crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        });

    assert!(model.execute_slash_command_line("/provider"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    let provider_index = model
        .filtered_provider_picker_defs()
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_XAI)
        .expect("expected xAI provider");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(provider_index as i32));
    model.handle_modal_enter(modal::ModalKind::ProviderPicker);

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let model_index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == "grok-4")
        .expect("expected xAI model");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(model_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    let mut saw_target_update = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SetTargetAgentProviderModel {
                target_agent_id,
                provider_id,
                model,
            } if target_agent_id == "weles"
                && provider_id == PROVIDER_ID_XAI
                && model == "grok-4"
        ) {
            saw_target_update = true;
        }
    }
    assert!(saw_target_update);
}

#[test]
fn slash_provider_updates_active_svarog_thread_header_after_model_pick() {
    let (mut model, _daemon_rx) = make_model();
    seed_active_svarog_thread(&mut model);
    model
        .auth
        .entries
        .push(crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        });

    assert_eq!(
        model.current_header_agent_profile().provider,
        PROVIDER_ID_OPENAI
    );
    assert_eq!(model.current_header_agent_profile().model, "gpt-5.4");

    assert!(model.execute_slash_command_line("/provider"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    let provider_index = model
        .filtered_provider_picker_defs()
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_XAI)
        .expect("expected xAI provider");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(provider_index as i32));
    model.handle_modal_enter(modal::ModalKind::ProviderPicker);

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let model_index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == "grok-4")
        .expect("expected xAI model");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(model_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.provider, PROVIDER_ID_XAI);
    assert_eq!(profile.model, "grok-4");
}

#[test]
fn slash_provider_model_on_new_subagent_thread_queues_execution_profile_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.auth.loaded = true;
    model
        .auth
        .entries
        .push(crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        });
    model.subagents.entries.push(crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "dola".to_string(),
        name: "Dola".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4".to_string(),
        role: Some("helper".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("dola"));
    assert!(model.execute_slash_command_line("/provider"));
    let provider_index = model
        .filtered_provider_picker_defs()
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_XAI)
        .expect("expected xAI provider");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(provider_index as i32));
    model.handle_modal_enter(modal::ModalKind::ProviderPicker);

    let model_index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == "grok-4")
        .expect("expected xAI model");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(model_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.provider, PROVIDER_ID_XAI);
    assert_eq!(profile.model, "grok-4");

    let mut saw_target_update = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            DaemonCommand::SetTargetAgentProviderModel {
                target_agent_id,
                provider_id,
                model,
            } if target_agent_id == "dola"
                && provider_id == PROVIDER_ID_XAI
                && model == "grok-4"
        ) {
            saw_target_update = true;
        }
    }
    assert!(saw_target_update);

    model.submit_prompt("hello from dola".to_string());

    let mut saw_execution_profile = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetThreadExecutionProfile { profile_json, .. }
                if profile_json.contains(PROVIDER_ID_XAI) && profile_json.contains("grok-4") =>
            {
                saw_execution_profile = true;
            }
            DaemonCommand::SendMessage { .. } | DaemonCommand::DismissConciergeWelcome => {}
            other => panic!("unexpected daemon command: {:?}", other),
        }
    }
    assert!(saw_execution_profile);
    let thread = model
        .chat
        .active_thread()
        .expect("local thread should exist after first prompt");
    assert_eq!(thread.profile_provider.as_deref(), Some(PROVIDER_ID_XAI));
    assert_eq!(thread.profile_model.as_deref(), Some("grok-4"));
}

#[test]
fn target_agent_model_picker_excludes_stale_models_from_other_providers() {
    let (mut model, _daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model.start_new_thread_view_for_agent(Some("weles"));
    model
        .config
        .reduce(crate::state::config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "grok-4".to_string(),
                name: Some("Grok 4".to_string()),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));
    assert!(model.execute_slash_command_line("/model"));
    assert!(
        !model
            .available_model_picker_models()
            .iter()
            .any(|entry| entry.id == "grok-4"),
        "stale xAI models must be cleared when the OpenAI picker opens"
    );
}

#[test]
fn target_agent_model_picker_includes_remote_models_for_active_provider() {
    let (mut model, _daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model.start_new_thread_view_for_agent(Some("weles"));
    assert!(model.execute_slash_command_line("/model"));
    model
        .config
        .reduce(crate::state::config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    assert!(
        model
            .available_model_picker_models()
            .iter()
            .any(|entry| entry.id == "gpt-5.4-mini"),
        "remote OpenAI models should appear when picker provider matches"
    );
}

#[test]
fn slash_effort_updates_active_thread_owner_effort() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);

    assert!(model.execute_slash_command_line("/effort"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
    model.modal.reduce(modal::ModalAction::Navigate(1));
    model.handle_modal_enter(modal::ModalKind::EffortPicker);

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected target agent effort update"),
        DaemonCommand::SetTargetAgentReasoningEffort {
            target_agent_id,
            reasoning_effort,
        } if target_agent_id == "weles" && reasoning_effort == "high"
    ));
    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("high")
    );
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn settings_svarog_model_change_does_not_retarget_active_non_svarog_thread() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.open_provider_backed_model_picker(
        SettingsPickerTarget::Model,
        model.config.provider.clone(),
        model.config.base_url.clone(),
        model.config.api_key.clone(),
        model.config.auth_source.clone(),
    );
    let (model_index, next_model) = model
        .available_model_picker_models()
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.id != "gpt-5.4")
        .map(|(index, entry)| (index, entry.id.clone()))
        .expect("expected an alternative OpenAI model");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(model_index as i32));
    model.handle_modal_enter(modal::ModalKind::ModelPicker);

    assert_eq!(model.config.model, next_model);
    let thread = model.chat.active_thread().expect("weles thread stays active");
    assert_eq!(thread.profile_model.as_deref(), Some("gpt-5.4"));
    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(
                command,
                DaemonCommand::SetTargetAgentProviderModel { .. }
                    | DaemonCommand::SetThreadExecutionProfile { .. }
            ),
            "Svarog settings must not rewrite another agent or its thread: {command:?}"
        );
    }
}

#[test]
fn settings_rarog_provider_edits_rarog_not_active_thread_owner() {
    let (mut model, mut daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let field_index = (0..64)
        .find(|index| {
            model.settings_navigate_to(*index);
            model.current_settings_field_name() == "concierge_provider"
        })
        .expect("Rarog tab exposes a provider field");
    model.settings_navigate_to(field_index);

    model.activate_settings_field();

    assert_eq!(
        model.settings_picker_target,
        Some(SettingsPickerTarget::ConciergeProvider)
    );
    assert!(model.pending_target_agent_config.is_none());
    model.handle_modal_enter(modal::ModalKind::ProviderPicker);
    assert_eq!(model.concierge.provider.as_deref(), Some(PROVIDER_ID_OPENAI));
    while let Ok(command) = daemon_rx.try_recv() {
        assert!(
            !matches!(
                command,
                DaemonCommand::SetTargetAgentProviderModel { .. }
                    | DaemonCommand::SetThreadExecutionProfile { .. }
            ),
            "Rarog settings must not edit the active thread's agent: {command:?}"
        );
    }
}
