use super::super::{auth_env_lock, make_model, unique_test_db_path};
use super::*;
use crate::state::settings::SettingsTab;
#[test]
fn activating_about_tab_does_not_enter_edit_mode() {
    let (mut model, _daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::About));

    model.activate_settings_field();

    assert!(!model.settings.is_editing());
}

#[test]
fn whatsapp_link_device_requires_allowed_contacts() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.settings.reduce(SettingsAction::NavigateField(17));

    model.activate_settings_field();

    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(
        model.status_line,
        "Set at least one allowed WhatsApp phone number before linking"
    );
    assert_eq!(model.modal.top(), None);
}

#[test]
fn whatsapp_relink_device_requires_allowed_contacts() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.settings.reduce(SettingsAction::NavigateField(18));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(
        model.status_line,
        "Set at least one allowed WhatsApp phone number before linking"
    );
    assert_eq!(model.modal.top(), None);
}

#[test]
fn whatsapp_link_device_starts_when_allowlist_is_present() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535\ninvalid".to_string();
    model.settings.reduce(SettingsAction::NavigateField(17));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status probe"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.status_line, "Starting WhatsApp link workflow");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
}

#[test]
fn github_copilot_logout_clears_db_auth_and_refreshes_entries() {
    let _lock = auth_env_lock();
    let _guard = EnvGuard::new(&["ZORAI_PROVIDER_AUTH_DB_PATH"]);
    let db_path = unique_test_db_path("copilot-logout");
    std::env::set_var("ZORAI_PROVIDER_AUTH_DB_PATH", &db_path);
    write_provider_auth_row(&db_path, PROVIDER_ID_GITHUB_COPILOT, "github_copilot");

    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_GITHUB_COPILOT.to_string(),
        provider_name: "GitHub Copilot".to_string(),
        authenticated: true,
        auth_source: "github_copilot".to_string(),
        model: "openai/gpt-4.1".to_string(),
    }];
    model.auth.action_cursor = 0;
    model.auth.selected = 0;

    model.run_auth_tab_action();

    assert!(!has_provider_auth_row(
        &db_path,
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot"
    ));
    assert_eq!(model.status_line, "GitHub Copilot auth cleared");
    assert!(matches!(
        daemon_rx.try_recv().expect("expected auth refresh"),
        DaemonCommand::GetProviderAuthStates
    ));
    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn openai_chatgpt_login_requests_daemon_flow_without_local_modal_state() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 0;
    model.auth.selected = 0;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected daemon login command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_ne!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
}

#[test]
fn openai_chatgpt_logout_requests_daemon_flow() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 0;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected daemon logout command"),
        DaemonCommand::LogoutOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn api_key_login_marks_provider_authenticated_immediately() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_DEEPSEEK.to_string(),
        provider_name: "DeepSeek".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "deepseek-v4-pro".to_string(),
    }];
    model.auth.action_cursor = 0;
    model.auth.selected = 0;

    model.run_auth_tab_action();
    model.auth.login_buffer = "deepseek-key".to_string();
    model.auth.login_cursor = model.auth.login_buffer.chars().count();
    model.confirm_auth_login();

    assert!(model.auth.entries[0].authenticated);
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected provider login command"),
        DaemonCommand::LoginProvider {
            provider_id,
            api_key,
            ..
        } if provider_id == PROVIDER_ID_DEEPSEEK && api_key == "deepseek-key"
    ));
}

#[test]
fn openai_chatgpt_second_action_requests_login_when_auth_not_available() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 1;
    model.auth.selected = 0;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.auth_source = "chatgpt_subscription".to_string();
    model.config.chatgpt_auth_available = false;
    model.status_line = "before refresh".to_string();

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected daemon login command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.status_line, "before refresh");
}

#[test]
fn openai_chatgpt_second_action_restarts_login_after_logout() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 1;
    model.auth.selected = 0;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.chatgpt_auth_available = false;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected daemon auth status command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn api_transport_cycles_for_github_copilot() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings.reduce(SettingsAction::NavigateField(4));
    assert_eq!(model.settings.current_field_name(), "api_transport");

    model.activate_settings_field();

    assert_eq!(model.config.api_transport, "chat_completions");

    model.activate_settings_field();

    assert_eq!(model.config.api_transport, "anthropic_messages");

    model.activate_settings_field();

    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn github_copilot_preserves_responses_transport_when_loaded_from_saved_config() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn github_copilot_preserves_explicit_anthropic_transport_when_loaded_from_saved_config() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "claude-sonnet-4.6",
                "api_transport": "anthropic_messages",
                "auth_source": "github_copilot"
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.model, "claude-sonnet-4.6");
    assert_eq!(model.config.api_transport, "anthropic_messages");
}

#[test]
fn github_copilot_gemini_31_forces_chat_completions_when_loaded_from_saved_config() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gemini-3.1-pro-preview",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.model, "gemini-3.1-pro-preview");
    assert_eq!(model.config.api_transport, "chat_completions");
}

#[test]
fn commit_subagent_editor_persists_existing_provider_model_and_effort_changes() {
    let (mut model, mut daemon_rx) = make_model();
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        claude_permission_mode: None,
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("code_review".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Protected builtin".to_string()),
        reasoning_effort: Some("medium".to_string()),
        api_transport: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "WELES",
            "provider": PROVIDER_ID_OPENAI,
            "model": "gpt-5.4-mini",
            "role": "code_review",
            "enabled": true,
            "builtin": true,
            "immutable_identity": true,
            "disable_allowed": false,
            "delete_allowed": false,
            "protected_reason": "Protected builtin",
            "reasoning_effort": "medium",
            "created_at": 1
        })),
    }];

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_ANTHROPIC.to_string(),
        "claude-sonnet-4-5".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.role = "code_review".to_string();
    editor.enabled = true;
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.disable_allowed = false;
    editor.delete_allowed = false;
    editor.protected_reason = Some("Protected builtin".to_string());
    editor.reasoning_effort = Some("high".to_string());
    editor.context_window_tokens = Some(333_000);
    editor.raw_json = Some(serde_json::json!({
        "id": "weles_builtin",
        "name": "WELES",
        "provider": PROVIDER_ID_OPENAI,
        "model": "gpt-5.4-mini",
        "role": "code_review",
        "enabled": true,
        "builtin": true,
        "immutable_identity": true,
        "disable_allowed": false,
        "delete_allowed": false,
        "protected_reason": "Protected builtin",
        "reasoning_effort": "medium",
        "created_at": 1
    }));
    model.subagents.editor = Some(editor);

    model.commit_subagent_editor();

    let command = daemon_rx
        .try_recv()
        .expect("expected sub-agent update command");
    let DaemonCommand::SetSubAgent(payload) = command else {
        panic!("expected SetSubAgent command");
    };
    let saved: serde_json::Value =
        serde_json::from_str(&payload).expect("payload should be valid json");
    assert_eq!(
        saved.get("provider").and_then(|value| value.as_str()),
        Some(PROVIDER_ID_ANTHROPIC)
    );
    assert_eq!(
        saved.get("model").and_then(|value| value.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(
        saved
            .get("reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("high")
    );
    assert_eq!(
        saved
            .get("context_window_tokens")
            .and_then(|value| value.as_u64()),
        Some(333_000)
    );

    let entry = model
        .subagents
        .entries
        .iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("updated entry should remain present");
    assert_eq!(entry.provider, "anthropic");
    assert_eq!(entry.model, "claude-sonnet-4-5");
    assert_eq!(entry.reasoning_effort.as_deref(), Some("high"));
}
