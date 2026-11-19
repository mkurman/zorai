use super::*;

#[test]
fn goal_composer_add_agent_hotkey_creates_another_role_assignment() {
    let mut model = build_model();
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            }],
            "Main agent inheritance",
        );
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.goal_mission_control.role_assignments.len(), 2);
    assert_eq!(
        model.goal_mission_control.selected_runtime_assignment_index,
        1
    );
}

#[test]
fn goal_composer_launch_sends_preflight_role_assignments() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "planning".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Previous goal snapshot",
        );
    model
        .goal_mission_control
        .set_prompt_text("Ship the next release");

    model.start_goal_run_from_mission_control();

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::DismissConciergeWelcome) => {}
        other => panic!("expected concierge dismissal before launch, got {other:?}"),
    }
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::StartGoalRun {
            goal,
            launch_assignments,
            ..
        }) => {
            assert_eq!(goal, "Ship the next release");
            assert_eq!(launch_assignments.len(), 2);
            assert_eq!(launch_assignments[1].role_id, "planning");
        }
        other => panic!("expected start-goal-run command, got {other:?}"),
    }
}

#[test]
fn goal_composer_role_picker_allows_custom_researcher_assignment() {
    let mut model = build_model();
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "research".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Main agent inheritance",
        );
    model
        .goal_mission_control
        .set_selected_runtime_assignment_index(1);
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::RolePicker));

    let custom_row = crate::state::subagents::role_picker_item_count() - 1;
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );
    assert!(!handled);
    assert_eq!(
        model.settings.editing_field(),
        Some("mission_control_assignment_role")
    );

    model.settings.reduce(SettingsAction::InsertChar('e'));
    model.settings.reduce(SettingsAction::InsertChar('r'));
    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!handled);
    assert_eq!(
        model.goal_mission_control.role_assignments[1].role_id,
        "researcher"
    );
}

#[test]
fn goal_composer_custom_role_edit_keeps_typing_in_inline_editor() {
    let mut model = build_model();
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "research".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Main agent inheritance",
        );
    model
        .goal_mission_control
        .set_selected_runtime_assignment_index(1);
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Input;

    let opened = model.stage_mission_control_assignment_modal_edit(
        goal_mission_control::RuntimeAssignmentEditField::Role,
    );
    assert!(opened);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::RolePicker));

    let custom_row = crate::state::subagents::role_picker_custom_index();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(
        model.settings.editing_field(),
        Some("mission_control_assignment_role")
    );

    let handled = model.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
    assert!(!handled);
    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(!handled);
    assert!(model.input.buffer().is_empty());
    assert_eq!(model.settings.edit_buffer(), "researcher");

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.settings.editing_field(), None);
    assert_eq!(
        model.goal_mission_control.role_assignments[1].role_id,
        "researcher"
    );
}

#[test]
fn goal_composer_role_picker_allows_builtin_persona_assignment() {
    let mut model = build_model();
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "research".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Main agent inheritance",
        );
    model
        .goal_mission_control
        .set_selected_runtime_assignment_index(1);
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::RolePicker));

    let mokosh_row = crate::state::subagents::role_picker_index_for_id("mokosh")
        .expect("mokosh should be available as a Mission Control role");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(mokosh_row as i32));

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );

    assert!(!handled);
    assert_eq!(
        model.goal_mission_control.role_assignments[1].role_id,
        "mokosh"
    );
}

#[test]
fn goal_composer_launch_includes_attached_text_files_in_goal_prompt() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Chat;
    model
        .goal_mission_control
        .set_prompt_text("analyze".to_string());
    model.attachments.push(Attachment {
        filename: "plan.md".to_string(),
        size_bytes: 18,
        payload: AttachmentPayload::Text("Implementation plan".to_string()),
    });

    model.start_goal_run_from_mission_control();

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::DismissConciergeWelcome) => {}
        other => panic!("expected concierge dismissal before launch, got {other:?}"),
    }
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::StartGoalRun { goal, .. }) => {
            assert!(goal.contains("<attached_file name=\"plan.md\">"), "{goal}");
            assert!(goal.contains("Implementation plan"), "{goal}");
            assert!(goal.ends_with("analyze"), "{goal}");
        }
        other => panic!("expected start-goal-run command, got {other:?}"),
    }
    assert!(
        model.attachments.is_empty(),
        "goal launch should consume pending attachments"
    );
}

#[test]
fn mission_control_model_picker_fetches_models_for_selected_assignment_provider() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "researcher".to_string(),
                    enabled: true,
                    provider: zorai_shared::providers::PROVIDER_ID_CHUTES.to_string(),
                    model: "deepseek-ai/DeepSeek-R1".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Main agent inheritance",
        );
    model
        .goal_mission_control
        .set_selected_runtime_assignment_index(1);
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            zorai_shared::providers::PROVIDER_ID_CHUTES: {
                "base_url": "https://llm.chutes.ai/v1",
                "api_key": "chutes-key",
                "auth_source": "api_key"
            }
        }
    }));

    let opened = model.stage_mission_control_assignment_modal_edit(
        goal_mission_control::RuntimeAssignmentEditField::Model,
    );

    assert!(opened);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, zorai_shared::providers::PROVIDER_ID_CHUTES);
            assert_eq!(base_url, "https://llm.chutes.ai/v1");
            assert_eq!(api_key, "chutes-key");
            assert_eq!(output_modalities, None);
        }
        other => panic!(
            "expected FetchModels for mission control assignment model picker, got {other:?}"
        ),
    }
}

#[test]
fn mission_control_custom_model_entry_updates_selected_assignment_not_global_model() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.main_pane_view = MainPaneView::GoalComposer;
    model.config.model = "gpt-5.4".to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.goal_mission_control =
        goal_mission_control::GoalMissionControlState::from_main_assignment(
            task::GoalAgentAssignment {
                role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                enabled: true,
                provider: "openai".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: Some("medium".to_string()),
                inherit_from_main: false,
            },
            vec![
                task::GoalAgentAssignment {
                    role_id: zorai_protocol::AGENT_ID_SWAROG.to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning_effort: Some("medium".to_string()),
                    inherit_from_main: false,
                },
                task::GoalAgentAssignment {
                    role_id: "researcher".to_string(),
                    enabled: true,
                    provider: "openai".to_string(),
                    model: "gpt-5.4-mini".to_string(),
                    reasoning_effort: Some("high".to_string()),
                    inherit_from_main: false,
                },
            ],
            "Main agent inheritance",
        );
    model
        .goal_mission_control
        .set_selected_runtime_assignment_index(1);

    let opened = model.stage_mission_control_assignment_modal_edit(
        goal_mission_control::RuntimeAssignmentEditField::Model,
    );
    assert!(opened);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));

    let custom_row = model.available_runtime_assignment_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!handled);
    assert_eq!(
        model.settings.editing_field(),
        Some("mission_control_assignment_model")
    );
    model.settings.reduce(SettingsAction::InsertChar('x'));

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!handled);
    assert_eq!(model.config.model, "gpt-5.4");
    assert_eq!(
        model.goal_mission_control.role_assignments[1].model,
        "gpt-5.4-minix"
    );
}

#[test]
fn custom_model_modalities_reach_the_daemon_under_the_shared_provider_model_key() {
    let (_daemon_tx, daemon_rx) = std::sync::mpsc::channel();
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.config.provider = "openrouter".to_string();
    assert_eq!(
        TuiModel::custom_model_modalities("Video, image bogus"),
        vec!["text", "image", "video"]
    );
    model.set_custom_model_modalities("qwen/qwen3-vl", "video");
    let mut sent = None;
    while let Ok(command) = cmd_rx.try_recv() {
        if let DaemonCommand::SetConfigItem {
            key_path,
            value_json,
        } = command
        {
            sent = Some((key_path, value_json));
        }
    }
    assert_eq!(
        sent,
        Some((
            "/model_modalities/openrouter::qwen~1qwen3-vl".to_string(),
            "[\"text\",\"video\"]".to_string()
        ))
    );
}
