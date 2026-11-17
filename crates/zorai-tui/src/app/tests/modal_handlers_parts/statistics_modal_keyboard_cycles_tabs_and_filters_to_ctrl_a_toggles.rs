use super::whatsapp_modal_esc_sends_stop_and_closes_to_clicking_rendered_settings::*;
use crate::app::*;
#[test]
fn statistics_modal_keyboard_cycles_tabs_and_filters() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
    model.statistics_modal_snapshot = Some(zorai_protocol::AgentStatisticsSnapshot {
        window: zorai_protocol::AgentStatisticsWindow::All,
        generated_at: 1,
        has_incomplete_cost_history: false,
        totals: zorai_protocol::AgentStatisticsTotals {
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
            provider_count: 1,
            model_count: 1,
        },
        providers: vec![zorai_protocol::ProviderStatisticsRow {
            provider: "openai".to_string(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
        }],
        models: vec![zorai_protocol::ModelStatisticsRow {
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
        }],
        top_models_by_tokens: Vec::new(),
        top_models_by_cost: Vec::new(),
        daily: Vec::new(),
        sessions: Vec::new(),
        session_total: 0,
        session_limit: 25,
        session_offset: 0,
        content: Default::default(),
    });

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::Statistics,
    );
    assert!(!quit);
    assert_eq!(
        model.statistics_modal_tab,
        crate::state::statistics::StatisticsTab::Providers
    );

    let quit = model.handle_key_modal(
        KeyCode::Char(']'),
        KeyModifiers::NONE,
        modal::ModalKind::Statistics,
    );
    assert!(!quit);
    assert_eq!(
        model.statistics_modal_window,
        zorai_protocol::AgentStatisticsWindow::Today
    );
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatistics { window }) => {
            assert_eq!(window, zorai_protocol::AgentStatisticsWindow::Today);
        }
        other => panic!("expected statistics refetch, got {:?}", other),
    }
}

#[test]
fn command_palette_prompt_query_with_args_requests_target_agent() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "prompt weles".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected prompt inspection request from command palette query, got {:?}",
            other
        ),
    }
}

#[test]
fn command_palette_enter_prefers_highlighted_command_over_partial_query() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new");
    assert!(model.modal.command_palette_has_explicit_selection());

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!quit);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
}

#[test]
fn command_palette_typing_does_not_preview_first_match_before_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new-g".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new-g");
    assert!(!model.modal.command_palette_has_explicit_selection());
}

#[test]
fn slash_opened_command_palette_keeps_raw_filter_text() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.modal.command_display_query(), "new");

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.modal.command_display_query(), "new");
}

#[test]
fn command_palette_goa_filter_survives_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "goa".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("goal")
    );

    let quit = model.handle_key_modal(
        KeyCode::Up,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
}

#[test]
fn goal_composer_command_palette_typing_keeps_goal_draft_intact() {
    let (mut model, _daemon_rx) = make_model();
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Input;
    model.input.set_text("Ship release");
    model
        .goal_mission_control
        .set_prompt_text("Ship release".to_string());

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(!quit);

    assert_eq!(model.modal.command_display_query(), "n");
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");
}

#[test]
fn goal_composer_command_palette_reopens_fresh_after_close() {
    let (mut model, _daemon_rx) = make_model();
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Input;
    model.input.set_text("Ship release");
    model
        .goal_mission_control
        .set_prompt_text("Ship release".to_string());

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    assert_eq!(model.modal.command_display_query(), "new");

    let quit = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    assert_eq!(model.modal.command_display_query(), "");
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");
}

#[test]
fn chat_command_palette_typing_keeps_chat_draft_intact() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Input;
    model.input.set_text("hello chat");

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(!quit);

    assert_eq!(model.modal.command_display_query(), "n");
    assert_eq!(model.input.buffer(), "hello chat");
}

#[test]
fn command_palette_enter_runs_first_match_without_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new-g".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new-g");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!quit);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
}

#[test]
fn command_palette_mouse_selection_executes_selected_command_without_rewriting_query() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    model.modal_navigate_to(1);
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new");
    assert!(model.modal.command_palette_has_explicit_selection());

    model.handle_modal_enter(modal::ModalKind::CommandPalette);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
}

#[test]
fn slash_command_can_restart_from_prompt_viewer_modal() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));

    for ch in "/prompt weles".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected prompt inspection request after restarting slash command, got {:?}",
            other
        ),
    }
}

#[test]
fn ctrl_e_in_error_modal_clears_error() {
    let (mut model, _daemon_rx) = make_model();
    model.last_error = Some("boom".to_string());
    model.error_active = true;

    let handled = model.handle_key(KeyCode::Char('e'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ErrorViewer));
    assert_eq!(model.last_error.as_deref(), Some("boom"));

    let handled = model.handle_key(KeyCode::Char('e'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert!(model.modal.top().is_none());
    assert!(
        model.last_error.is_none(),
        "second Ctrl+E should clear the stored error"
    );
    assert!(
        !model.error_active,
        "clearing the error should also clear the active error badge"
    );
}

#[test]
fn openai_auth_modal_enter_uses_daemon_provided_url() {
    let (mut model, _daemon_rx) = make_model();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=daemon".to_string());
    model.openai_auth_status_text =
        Some("Open this URL in your browser to complete ChatGPT authentication.".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::OpenAIAuth,
    );

    assert!(!quit);
    assert_eq!(
        model.openai_auth_url.as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=daemon")
    );
}

#[test]
fn openai_auth_modal_copy_uses_shared_clipboard_helper() {
    let (mut model, _daemon_rx) = make_model();
    crate::app::conversion::reset_last_copied_text();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=daemon".to_string());
    model.openai_auth_status_text =
        Some("Open this URL in your browser to complete ChatGPT authentication.".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    let quit = model.handle_key_modal(
        KeyCode::Char('c'),
        KeyModifiers::NONE,
        modal::ModalKind::OpenAIAuth,
    );

    assert!(!quit);
    assert_eq!(
        crate::app::conversion::last_copied_text().as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=daemon")
    );
    assert_eq!(model.status_line, "Copied ChatGPT login URL to clipboard");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
}

#[test]
fn ctrl_a_toggles_approval_center_modal() {
    let (mut model, _daemon_rx) = make_model();

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalCenter));

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!handled);
    assert!(model.modal.top().is_none());
}
