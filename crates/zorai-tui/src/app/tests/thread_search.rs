use super::*;
use crate::client::ThreadSearchResultVm;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::unbounded_channel;

fn make_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

fn drain(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

fn open_picker_and_type(model: &mut TuiModel, text: &str) {
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    for ch in text.chars() {
        model.handle_key_modal(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
            modal::ModalKind::ThreadPicker,
        );
    }
}

fn hit(thread_id: &str, title: &str, snippet: Option<&str>) -> ThreadSearchResultVm {
    ThreadSearchResultVm {
        thread_id: thread_id.to_string(),
        title: title.to_string(),
        updated_at: 1,
        snippet: snippet.map(str::to_string),
    }
}

fn results(query: &str, hits: Vec<ThreadSearchResultVm>) -> ThreadSearchResultsVm {
    ThreadSearchResultsVm {
        query: query.to_string(),
        semantic: false,
        semantic_error: None,
        results: hits,
    }
}

fn search_commands(commands: &[DaemonCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            DaemonCommand::SearchThreads { query } => Some(query.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn typing_sends_one_daemon_search_for_the_settled_query_not_one_per_keystroke() {
    let (mut model, mut daemon_rx) = make_model();
    open_picker_and_type(&mut model, "lora");

    assert!(!model.dispatch_due_thread_search(Instant::now()));
    assert!(search_commands(&drain(&mut daemon_rx)).is_empty());

    assert!(model.dispatch_due_thread_search(Instant::now() + THREAD_SEARCH_DEBOUNCE * 2));
    assert_eq!(
        search_commands(&drain(&mut daemon_rx)),
        vec!["lora".to_string()]
    );
    assert!(model.pending_thread_search_deadline().is_none());
}

#[test]
fn a_single_character_keeps_the_local_filter_and_never_hits_the_daemon() {
    let (mut model, mut daemon_rx) = make_model();
    open_picker_and_type(&mut model, "l");

    assert!(model.pending_thread_search_deadline().is_none());
    assert!(!model.dispatch_due_thread_search(Instant::now() + THREAD_SEARCH_DEBOUNCE * 2));
    assert!(search_commands(&drain(&mut daemon_rx)).is_empty());
}

#[test]
fn enter_opens_a_search_hit_even_when_the_tui_never_loaded_that_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-loaded".into(),
            agent_name: Some("Svarog".into()),
            title: "Weekly sync".into(),
            ..Default::default()
        },
    ]));
    open_picker_and_type(&mut model, "lora");
    model.handle_thread_search_results(results(
        "lora",
        vec![
            hit(
                "thread-archived",
                "LoRA sweep plan",
                Some("start the lora sweep"),
            ),
            hit(
                "thread-loaded",
                "Weekly sync",
                Some("we discussed the lora sweep"),
            ),
        ],
    ));
    drain(&mut daemon_rx);

    let listed = widgets::thread_picker::filtered_threads_for_workspace(
        &model.chat,
        &model.modal,
        &model.subagents,
        &model.tasks,
        &model.workspace,
    )
    .iter()
    .map(|thread| (thread.id.clone(), thread.agent_name.clone()))
    .collect::<Vec<_>>();
    assert_eq!(
        listed,
        vec![
            ("thread-archived".to_string(), None),
            ("thread-loaded".to_string(), Some("Svarog".to_string())),
        ],
        "daemon ranking decides the order, and loaded threads keep their live state"
    );

    model.modal.reduce(modal::ModalAction::Navigate(1));
    model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert_eq!(model.chat.active_thread_id(), Some("thread-archived"));
    assert!(drain(&mut daemon_rx).iter().any(|command| matches!(
        command,
        DaemonCommand::RequestThread { thread_id, .. } if thread_id == "thread-archived"
    )));
}

#[test]
fn results_for_text_the_user_already_changed_are_dropped() {
    let (mut model, _daemon_rx) = make_model();
    open_picker_and_type(&mut model, "lora");

    model.handle_thread_search_results(results("lor", vec![hit("thread-a", "A", None)]));

    assert!(model.modal.thread_search().is_none());
}

#[test]
fn shortening_the_query_below_two_characters_returns_to_the_local_list() {
    let (mut model, _daemon_rx) = make_model();
    open_picker_and_type(&mut model, "lo");
    model.handle_thread_search_results(results("lo", vec![hit("thread-a", "A", None)]));
    assert!(model.modal.thread_search().is_some());

    model.handle_key_modal(
        KeyCode::Backspace,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(model.modal.thread_search().is_none());
    assert!(model.pending_thread_search_deadline().is_none());
}

#[test]
fn the_picker_shows_why_the_selected_thread_matched() {
    let (mut model, _daemon_rx) = make_model();
    open_picker_and_type(&mut model, "lora");
    model.handle_thread_search_results(results(
        "lora",
        vec![hit(
            "thread-archived",
            "LoRA sweep plan",
            Some("start the\n lora   sweep"),
        )],
    ));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal.draw(|frame| model.render(frame)).expect("render");
    let buffer = terminal.backend().buffer();
    let screen = (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(screen.contains("LoRA sweep plan"), "{screen}");
    assert!(
        screen.contains("keyword │ start the lora sweep"),
        "{screen}"
    );
}

#[test]
fn the_daemon_search_payload_parses_including_fields_the_tui_does_not_use() {
    let payload = r#"{"query":"lora","semantic":false,"semantic_error":null,"results":[{"thread_id":"thread-a","title":"LoRA","updated_at":1792746406289,"snippet":"lora sweep","matched":["title","content"],"score":0.03}]}"#;

    let parsed: ThreadSearchResultsVm = serde_json::from_str(payload).expect("parse");

    assert_eq!(parsed.query, "lora");
    assert_eq!(parsed.results[0].thread_id, "thread-a");
    assert_eq!(parsed.results[0].updated_at, 1792746406289);
}
