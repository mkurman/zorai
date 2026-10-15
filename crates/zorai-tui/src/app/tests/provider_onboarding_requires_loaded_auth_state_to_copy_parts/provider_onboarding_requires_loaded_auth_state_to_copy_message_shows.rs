use super::super::{build_model, unauthenticated_entry};
use super::*;
use ratatui::backend::TestBackend;

#[test]
fn provider_onboarding_requires_loaded_auth_state() {
    let mut model = build_model();
    model.connected = true;
    model.auth.entries = vec![unauthenticated_entry()];

    assert!(!model.should_show_provider_onboarding());
}

#[test]
fn provider_onboarding_shows_when_no_provider_is_configured() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    model.auth.entries = vec![unauthenticated_entry()];

    assert!(model.should_show_provider_onboarding());
}

#[test]
fn provider_onboarding_hides_when_provider_is_configured() {
    let mut model = build_model();
    model.connected = true;
    model.auth.loaded = true;
    let mut entry = unauthenticated_entry();
    entry.authenticated = true;
    model.auth.entries = vec![entry];

    assert!(!model.should_show_provider_onboarding());
}

#[test]
fn local_landing_shows_only_for_empty_conversation_state() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;

    assert!(model.should_show_local_landing());

    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    assert!(!model.should_show_local_landing());

    model
        .chat
        .reduce(chat::ChatAction::SelectThread(String::new()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "stream".to_string(),
        content: "hello".to_string(),
    });
    assert!(!model.should_show_local_landing());

    model.chat.reduce(chat::ChatAction::ResetStreaming);
    model
        .chat
        .reduce(chat::ChatAction::ThreadListReceived(Vec::new()));
    model.connected = true;
    model.auth.loaded = true;
    model.auth.entries = vec![unauthenticated_entry()];
    assert!(!model.should_show_local_landing());
}

#[test]
fn daemon_connection_loading_shows_for_disconnected_empty_conversation_state() {
    let model = build_model();

    assert!(model.should_show_daemon_connection_loading());
    assert!(!model.should_show_local_landing());
}

#[test]
fn daemon_connection_loading_persists_until_config_is_loaded() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Recovered Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(model.should_show_daemon_connection_loading());
    assert!(!model.should_show_local_landing());
}

#[test]
fn local_landing_yields_to_concierge_loading() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.concierge.loading = true;

    assert!(model.should_show_concierge_hero_loading());
    assert!(
        !model.should_show_local_landing(),
        "local landing should not hide concierge loading animation"
    );
}

#[test]
fn header_shows_rarog_runtime_while_concierge_welcome_is_loading() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = "openai".to_string();
    model.config.model = "gpt-6.1-sol".to_string();
    model.concierge.provider = Some("max-coding-plan".to_string());
    model.concierge.model = Some("MiniMax-M3.1-Flash-Preview".to_string());
    model.concierge.reasoning_effort = Some("low".to_string());
    model.concierge.loading = true;

    let profile = model.current_header_agent_profile();
    assert_eq!(
        profile.provider, "max-coding-plan",
        "Rarog writes the welcome, so the header must not show Svarog's runtime while it loads"
    );
    assert_eq!(profile.model, "MiniMax-M3.1-Flash-Preview");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn thread_loading_placeholder_shows_for_selected_empty_thread() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.thread_loading_id = Some("thread-1".to_string());

    assert!(model.should_show_thread_loading());
    assert!(!model.should_show_local_landing());
}

#[test]
fn thread_loading_renders_dedicated_rarog_animation() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.width = 100;
    model.height = 40;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Release planning".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.thread_loading_id = Some("thread-1".to_string());

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("thread loading render should succeed");

    let buffer = terminal.backend().buffer();
    let rendered = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered.contains("Loading thread: Release planning"),
        "thread loading should show thread-specific copy in the dedicated animation"
    );
    assert!(
        rendered.contains("Replaying recent turns"),
        "thread loading should use thread-specific loading stages"
    );
    assert!(
        !rendered.contains("Rarog is threading a welcome"),
        "thread loading should stop reusing concierge welcome copy"
    );
}

#[test]
fn daemon_connection_loading_renders_svarog_landing_with_waiting_copy() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.focus = FocusArea::Chat;

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("daemon connection loading render should succeed");

    let buffer = terminal.backend().buffer();
    let rendered = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        rendered.contains("Waiting for daemon connection"),
        "disconnected landing should show connection waiting copy"
    );
    assert!(
        rendered.contains("Warming the forge"),
        "disconnected landing should render animated connection-stage copy"
    );
    assert!(
        !rendered.contains("Type to begin."),
        "disconnected landing should not present the ready-to-type prompt"
    );
}

#[test]
fn anticipatory_banner_is_suppressed_while_concierge_welcome_is_active() {
    let mut model = build_model();
    model
        .anticipatory
        .reduce(crate::state::AnticipatoryAction::Replace(vec![
            crate::wire::AnticipatoryItem {
                id: "digest-1".to_string(),
                ..Default::default()
            },
        ]));
    model
        .concierge
        .reduce(crate::state::ConciergeAction::WelcomeReceived {
            content: "Welcome back".to_string(),
            actions: vec![crate::state::ConciergeActionVm {
                label: "Continue".to_string(),
                action_type: "continue_session".to_string(),
                thread_id: Some("thread-1".to_string()),
            }],
        });

    assert_eq!(model.anticipatory_banner_height(), 0);
}

#[test]
fn local_landing_full_render_does_not_panic_at_width_100() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.width = 100;
    model.height = 40;
    model.focus = FocusArea::Input;

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("local landing render should not panic at width 100");
}

#[test]
fn local_landing_full_render_does_not_panic_at_width_80() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.width = 80;
    model.height = 24;
    model.focus = FocusArea::Input;

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("local landing render should not panic at width 80");
}

#[test]
fn full_render_does_not_panic_when_terminal_collapses_to_single_cell() {
    let mut model = build_model();
    model.width = 1;
    model.height = 1;
    model.focus = FocusArea::Input;

    let backend = TestBackend::new(1, 1);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should tolerate a single-cell terminal");
}

#[test]
fn conversation_render_with_sidebar_does_not_panic_on_tiny_frame() {
    let mut model = build_model();
    model.width = 8;
    model.height = 6;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::ThreadTodosReceived {
        thread_id: "thread-1".to_string(),
        goal_run_id: None,
        step_index: None,
        items: vec![task::TodoItem {
            id: "todo-1".to_string(),
            content: "Investigate crash".to_string(),
            status: Some(task::TodoStatus::Pending),
            position: 0,
            step_index: None,
            created_at: 0,
            updated_at: 0,
        }],
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("conversation render should tolerate a tiny frame with sidebar");
}

#[test]
fn render_uses_frame_area_even_when_model_size_is_stale() {
    let mut model = build_model();
    model.width = 120;
    model.height = 40;
    model.focus = FocusArea::Input;

    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should honor the live frame size, not stale model dimensions");
}

#[test]
fn concierge_loading_uses_frame_area_even_when_model_size_is_stale() {
    let mut model = build_model();
    model.width = 120;
    model.height = 40;
    model.concierge.loading = true;
    model.focus = FocusArea::Chat;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("concierge loading should render within the live frame size");
}

#[test]
fn render_syncs_model_dimensions_to_live_frame_area() {
    let mut model = build_model();
    model.width = 120;
    model.height = 40;

    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should succeed against the live frame area");

    assert_eq!(model.width, 100);
    assert_eq!(model.height, 24);
}

#[test]
fn copy_message_formats_reasoning_and_content_with_separator() {
    let mut model = build_model();
    conversion::reset_last_copied_text();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            reasoning: Some("Private chain".to_string()),
            content: "Public answer".to_string(),
            ..Default::default()
        },
    });

    model.copy_message(0);

    assert_eq!(
        conversion::last_copied_text().as_deref(),
        Some("Reasoning:\nPrivate chain\n\n-------\n\nContent:\nPublic answer")
    );
}

#[test]
fn copy_message_shows_copied_label_until_timeout() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Public answer".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));

    model.copy_message(0);

    let copied_actions = widgets::chat::message_action_targets(
        &model.chat,
        0,
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.first())
            .expect("message should exist"),
        model.tick_counter,
    );
    assert_eq!(copied_actions[0].0, "[Copied]");

    for _ in 0..100 {
        model.on_tick();
    }

    let reverted_actions = widgets::chat::message_action_targets(
        &model.chat,
        0,
        model
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.first())
            .expect("message should exist"),
        model.tick_counter,
    );
    assert_eq!(reverted_actions[0].0, "[Copy]");
}
