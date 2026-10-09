use super::thread_picker_playgrounds_new_row_is_browse_only_to_slash_effort_updates::seed_active_weles_thread;
use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
use zorai_shared::providers::*;
#[test]
fn slash_effort_picker_starts_on_active_thread_owner_effort() {
    let (mut model, _daemon_rx) = make_model();
    seed_active_weles_thread(&mut model);
    model.config.reasoning_effort = "xhigh".to_string();

    assert!(model.execute_slash_command_line("/effort"));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
    assert_eq!(model.modal.picker_cursor(), 3);
}

#[test]
fn slash_effort_changes_only_the_active_thread_profile() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_DEEPSEEK.to_string();
    model.config.model = "deepseek-v4-pro".to_string();
    model.config.reasoning_effort = "high".to_string();
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-svarog".to_string(),
            agent_name: Some("Swarog".to_string()),
            profile_provider: Some(PROVIDER_ID_DEEPSEEK.to_string()),
            profile_model: Some("deepseek-v4-pro".to_string()),
            profile_reasoning_effort: Some("xhigh".to_string()),
            title: "Swarog thread".to_string(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-svarog".to_string()));

    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("xhigh")
    );
    assert!(model.execute_slash_command_line("/effort"));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
    assert_eq!(model.modal.picker_cursor(), 5);
    model.modal.reduce(modal::ModalAction::Navigate(-2));
    model.handle_modal_enter(modal::ModalKind::EffortPicker);

    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("medium")
    );
    let mut saved_paths = std::collections::BTreeMap::new();
    let mut thread_profiles = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } => {
                saved_paths.insert(key_path, value_json);
            }
            DaemonCommand::SetThreadExecutionProfile {
                thread_id,
                profile_json,
            } => thread_profiles.push((thread_id, profile_json)),
            _ => {}
        }
    }
    assert!(
        !saved_paths.keys().any(|key| key.ends_with("reasoning_effort")),
        "/effort is a thread command and must not rewrite the global Svarog effort"
    );
    assert_eq!(model.config.reasoning_effort, "high");
    let (thread_id, profile_json) = thread_profiles
        .last()
        .expect("/effort must persist the new effort on the active thread profile");
    assert_eq!(thread_id, "thread-svarog");
    let profile: serde_json::Value =
        serde_json::from_str(profile_json).expect("thread profile json");
    assert_eq!(profile["reasoning_effort"].as_str(), Some("medium"));
}

#[test]
fn settings_svarog_effort_updates_global_config_but_not_the_open_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_DEEPSEEK.to_string();
    model.config.model = "deepseek-v4-pro".to_string();
    model.config.reasoning_effort = "xhigh".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": PROVIDER_ID_DEEPSEEK,
        "model": "deepseek-v4-pro",
        "reasoning_effort": "xhigh",
        "providers": {
            PROVIDER_ID_DEEPSEEK: {
                "model": "deepseek-v4-pro",
                "reasoning_effort": "xhigh"
            }
        }
    }));
    model
        .chat
        .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
            id: "thread-svarog".to_string(),
            agent_name: Some("Swarog".to_string()),
            profile_provider: Some(PROVIDER_ID_DEEPSEEK.to_string()),
            profile_model: Some("deepseek-v4-pro".to_string()),
            profile_reasoning_effort: Some("xhigh".to_string()),
            title: "Swarog thread".to_string(),
            ..Default::default()
        }));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-svarog".to_string()));

    model.settings_picker_target = Some(SettingsPickerTarget::SvarogReasoningEffort);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
    model.sync_effort_picker_cursor_to_current();
    assert_eq!(model.modal.picker_cursor(), 5);
    model.modal.reduce(modal::ModalAction::Navigate(-1));
    model.handle_modal_enter(modal::ModalKind::EffortPicker);

    assert_eq!(model.config.reasoning_effort, "high");
    let raw = model
        .config
        .agent_config_raw
        .as_ref()
        .expect("raw config should stay loaded");
    assert_eq!(raw["reasoning_effort"].as_str(), Some("high"));
    assert_eq!(
        raw["providers"][PROVIDER_ID_DEEPSEEK]["reasoning_effort"].as_str(),
        Some("high")
    );
    assert_eq!(
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.profile_reasoning_effort.as_deref()),
        Some("xhigh"),
        "Settings change the global default only; the open thread keeps its pinned effort"
    );
    model.open_settings_tab(SettingsTab::Auth);
    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("xhigh")
    );

    model.handle_agent_config_event(crate::wire::AgentConfigSnapshot {
        provider: PROVIDER_ID_DEEPSEEK.to_string(),
        base_url: String::new(),
        model: "deepseek-v4-pro".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: "api_key".to_string(),
        api_transport: "responses".to_string(),
        reasoning_effort: "xhigh".to_string(),
        context_window_tokens: 1_000_000,
    });
    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_DEEPSEEK,
        "model": "deepseek-v4-pro",
        "reasoning_effort": "xhigh",
        "providers": {
            PROVIDER_ID_DEEPSEEK: {
                "model": "deepseek-v4-pro",
                "reasoning_effort": "xhigh"
            }
        }
    }));
    assert_eq!(
        model
            .current_header_agent_profile()
            .reasoning_effort
            .as_deref(),
        Some("xhigh")
    );
    assert_eq!(model.config.reasoning_effort, "high");
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("reasoning_effort"))
            .and_then(|value| value.as_str()),
        Some("high")
    );

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": PROVIDER_ID_DEEPSEEK,
        "model": "deepseek-v4-pro",
        "reasoning_effort": "high",
        "providers": {
            PROVIDER_ID_DEEPSEEK: {
                "model": "deepseek-v4-pro",
                "reasoning_effort": "high"
            }
        }
    }));
    assert!(
        model.pending_svarog_reasoning_effort.is_none(),
        "matching persisted config should clear the optimistic override"
    );

    let mut saved_paths = std::collections::BTreeMap::new();
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        } = command
        {
            saved_paths.insert(key_path, value_json);
        }
    }
    assert_eq!(
        saved_paths.get("/reasoning_effort").map(String::as_str),
        Some("\"high\"")
    );
    assert_eq!(
        saved_paths
            .get(&format!(
                "/providers/{}/reasoning_effort",
                PROVIDER_ID_DEEPSEEK
            ))
            .map(String::as_str),
        Some("\"high\"")
    );
}

#[test]
fn thread_picker_mouse_click_switches_to_rarog_tab() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("thread picker modal should be visible");

    let rarog_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::thread_picker::hit_test(
                    overlay_area,
                    &model.chat,
                    &model.modal,
                    &model.subagents,
                    pos,
                ) == Some(widgets::thread_picker::ThreadPickerHitTarget::Tab(
                    modal::ThreadPickerTab::Rarog,
                )) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("thread picker should expose a clickable Rarog tab");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rarog_pos.x,
        row: rarog_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );
}

#[test]
fn ctrl_q_opens_queued_prompts_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("steer prompt"));

    let quit = model.handle_key(KeyCode::Char('q'), KeyModifiers::CONTROL);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::QueuedPrompts));
    assert_eq!(model.modal.picker_cursor(), 0);
}

#[test]
fn queued_prompts_modal_send_now_stops_stream_and_sends_selected_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_tool_call_event(
        "thread-1".to_string(),
        "call-1".to_string(),
        "bash_command".to_string(),
        "{\"command\":\"pwd\"}".to_string(),
        None,
        None,
    );
    model
        .queued_prompts
        .push(QueuedPrompt::new("send this now"));
    model.open_queued_prompts_modal();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::StopStream { thread_id }) => assert_eq!(thread_id, "thread-1"),
        other => panic!("expected stop-stream before send-now, got {:?}", other),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "send this now");
        }
        other => panic!("expected send-now prompt dispatch, got {:?}", other),
    }
    assert!(model.queued_prompts.is_empty());
}

#[test]
fn queued_prompts_modal_copy_marks_item_as_copied_for_five_seconds() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("copy me"));
    model.open_queued_prompts_modal();

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );
    assert!(!quit);

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );
    assert!(!quit);
    assert!(model.queued_prompts[0].is_copied(model.tick_counter));

    for _ in 0..100 {
        model.on_tick();
    }
    assert!(!model.queued_prompts[0].is_copied(model.tick_counter));
}

#[test]
fn queued_prompts_modal_delete_action_removes_clicked_item() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("delete me"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let delete_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::queued_prompts::hit_test(
                    overlay_area,
                    &model.queued_prompts,
                    model.modal.picker_cursor(),
                    model.tick_counter,
                    pos,
                ) == Some(widgets::queued_prompts::QueuedPromptsHitTarget::Action {
                    message_index: 0,
                    action: QueuedPromptAction::Delete,
                }) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("delete action should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: delete_pos.x,
        row: delete_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(model.queued_prompts.is_empty());
    assert!(model.modal.top().is_none());
}

#[test]
fn queued_prompts_modal_clicking_row_opens_prompt_viewer_with_full_message() {
    let (mut model, _daemon_rx) = make_model();
    model
        .queued_prompts
        .push(QueuedPrompt::new("preview line\nfull queued message body"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let row_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::queued_prompts::hit_test(
                    overlay_area,
                    &model.queued_prompts,
                    model.modal.picker_cursor(),
                    model.tick_counter,
                    pos,
                ) == Some(widgets::queued_prompts::QueuedPromptsHitTarget::Row(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("queued prompt row should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: row_pos.x,
        row: row_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(
        model
            .prompt_modal_body()
            .contains("full queued message body"),
        "prompt viewer should show the full queued message body"
    );
}

#[test]
fn queued_prompts_modal_expand_action_opens_prompt_viewer_with_full_message() {
    let (mut model, _daemon_rx) = make_model();
    model
        .queued_prompts
        .push(QueuedPrompt::new("preview line\nexpanded via action"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let expand_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::queued_prompts::hit_test(
                    overlay_area,
                    &model.queued_prompts,
                    model.modal.picker_cursor(),
                    model.tick_counter,
                    pos,
                ) == Some(widgets::queued_prompts::QueuedPromptsHitTarget::Action {
                    message_index: 0,
                    action: QueuedPromptAction::Expand,
                }) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("expand action should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: expand_pos.x,
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(
        model.prompt_modal_body().contains("expanded via action"),
        "expand action should open the full queued message"
    );
}
