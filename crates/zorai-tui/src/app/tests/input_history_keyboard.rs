use super::*;

fn submit_input_for_history(model: &mut TuiModel, text: &str) {
    for ch in text.chars() {
        model.input.reduce(input::InputAction::InsertChar(ch));
    }
    model.input.reduce(input::InputAction::Submit);
    let _ = model.input.take_submitted();
}

#[test]
fn focused_empty_input_uses_up_down_for_sent_prompt_history() {
    let mut model = build_model();
    submit_input_for_history(&mut model, "first prompt");
    submit_input_for_history(&mut model, "second prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "second prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "first prompt");

    model.handle_key(KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "second prompt");

    model.handle_key(KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "");
}

#[test]
fn left_arrow_commits_selected_history_to_normal_composer_editing() {
    let mut model = build_model();
    submit_input_for_history(&mut model, "sent prompt");

    model.handle_key(KeyCode::Up, KeyModifiers::NONE);
    model.handle_key(KeyCode::Left, KeyModifiers::NONE);
    model.handle_key(KeyCode::Char('!'), KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "sent promp!t");
}

#[test]
fn conversation_typing_d_goes_to_composer_even_when_chat_focused() {
    let mut model = build_model();
    model.main_pane_view = MainPaneView::Conversation;
    model.focus = FocusArea::Chat;

    model.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);

    assert_eq!(model.input.buffer(), "d");
    assert_eq!(model.focus, FocusArea::Input);
    assert!(
        model.input_notice.is_none(),
        "typing d must not trigger single-letter shortcut notices"
    );
}

#[test]
fn tab_accepts_the_next_reply_suggestion_only_for_the_open_thread_and_an_empty_input() {
    let mut model = build_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-open".to_string(),
        title: "Open".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-open".to_string()));
    model.focus = FocusArea::Input;

    model.handle_client_event(ClientEvent::NextReplySuggestion {
        thread_id: "thread-other".to_string(),
        content: "Wrong thread".to_string(),
    });
    assert_eq!(model.next_reply_offer(), None);

    model.handle_client_event(ClientEvent::NextReplySuggestion {
        thread_id: "thread-open".to_string(),
        content: "Run the tests".to_string(),
    });
    model.input.reduce(input::InputAction::InsertChar('x'));
    assert_eq!(model.next_reply_offer(), None);
    model.input.reduce(input::InputAction::Backspace);

    model.handle_key(KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(model.input.buffer(), "Run the tests");
    assert_eq!(model.focus, FocusArea::Input);
}
