use super::super::*;

#[test]
fn open_resets_to_provider_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    state.reduce(SettingsAction::NavigateField(3));
    state.reduce(SettingsAction::EditField);
    assert_eq!(state.active_tab(), SettingsTab::Agent);
    assert!(state.is_dirty());

    state.reduce(SettingsAction::Open);
    assert_eq!(state.active_tab(), SettingsTab::Auth);
    assert_eq!(state.field_cursor(), 0);
    assert!(state.editing_field().is_none());
    assert!(!state.is_dirty());
}

#[test]
fn switch_tab_resets_cursor_and_editing() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::NavigateField(4));
    state.reduce(SettingsAction::EditField);
    assert!(state.editing_field().is_some());

    state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
    assert_eq!(state.active_tab(), SettingsTab::Tools);
    assert_eq!(state.field_cursor(), 0);
    assert!(state.editing_field().is_none());
}

#[test]
fn navigate_field_increases_cursor() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    state.reduce(SettingsAction::NavigateField(2));
    assert_eq!(state.field_cursor(), 2);
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.field_cursor(), 3);
}

#[test]
fn navigate_field_clamps_at_zero() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::NavigateField(-10));
    assert_eq!(state.field_cursor(), 0);
}

#[test]
fn navigate_field_clamps_at_max() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::NavigateField(100));
    assert_eq!(state.field_cursor(), 0);
}

#[test]
fn navigate_field_method_clamps_at_max() {
    let mut state = SettingsState::new();
    state.navigate_field(100, 5);
    assert_eq!(state.field_cursor(), 4);
}

#[test]
fn edit_field_sets_dirty() {
    let mut state = SettingsState::new();
    assert!(!state.is_dirty());
    state.reduce(SettingsAction::EditField);
    assert!(state.is_dirty());
    assert!(state.editing_field().is_some());
}

#[test]
fn confirm_edit_clears_editing_field() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::EditField);
    assert!(state.editing_field().is_some());
    state.reduce(SettingsAction::ConfirmEdit);
    assert!(state.editing_field().is_none());
    assert!(state.is_dirty());
}

#[test]
fn cancel_edit_clears_editing_field() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::EditField);
    state.reduce(SettingsAction::CancelEdit);
    assert!(state.editing_field().is_none());
}

#[test]
fn save_clears_dirty_flag() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::EditField);
    assert!(state.is_dirty());
    state.reduce(SettingsAction::Save);
    assert!(!state.is_dirty());
}

#[test]
fn dropdown_open_and_navigate() {
    let mut state = SettingsState::new();
    assert!(!state.is_dropdown_open());
    state.reduce(SettingsAction::OpenDropdown);
    assert!(state.is_dropdown_open());
    assert_eq!(state.dropdown_cursor(), 0);

    state.reduce(SettingsAction::NavigateDropdown(2));
    assert_eq!(state.dropdown_cursor(), 2);
    state.reduce(SettingsAction::NavigateDropdown(-1));
    assert_eq!(state.dropdown_cursor(), 1);
}

#[test]
fn select_dropdown_closes_and_sets_dirty() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::OpenDropdown);
    state.reduce(SettingsAction::SelectDropdown);
    assert!(!state.is_dropdown_open());
    assert!(state.is_dirty());
}

#[test]
fn close_clears_editing_and_dropdown() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::EditField);
    state.reduce(SettingsAction::OpenDropdown);
    state.reduce(SettingsAction::Close);
    assert!(state.editing_field().is_none());
    assert!(!state.is_dropdown_open());
}

#[test]
fn all_tabs_hide_provider_variant() {
    assert_eq!(SettingsTab::all().len(), 13);
    assert!(!SettingsTab::all().contains(&SettingsTab::Provider));
    assert_eq!(SettingsTab::all()[0], SettingsTab::Auth);
    assert_eq!(SettingsTab::all()[1], SettingsTab::Agent);
    assert_eq!(SettingsTab::all()[2], SettingsTab::Concierge);
    assert_eq!(SettingsTab::all().last(), Some(&SettingsTab::About));
}

#[test]
fn about_tab_has_zero_edit_fields() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::About));

    assert_eq!(state.field_count(), 0);
    assert_eq!(state.current_field_name(), "");
}

#[test]
fn tab_cycling_through_all() {
    let mut state = SettingsState::new();
    for &tab in SettingsTab::all() {
        state.reduce(SettingsAction::SwitchTab(tab));
        assert_eq!(state.active_tab(), tab);
    }
}

#[test]
fn insert_char_appends_to_edit_buffer() {
    let mut state = SettingsState::new();
    state.start_editing("base_url", "https://");
    assert!(state.is_editing());
    assert_eq!(state.edit_buffer(), "https://");

    state.reduce(SettingsAction::InsertChar('a'));
    state.reduce(SettingsAction::InsertChar('p'));
    state.reduce(SettingsAction::InsertChar('i'));
    assert_eq!(state.edit_buffer(), "https://api");
}

#[test]
fn backspace_removes_last_char() {
    let mut state = SettingsState::new();
    state.start_editing("api_key", "sk-abc");
    state.reduce(SettingsAction::Backspace);
    assert_eq!(state.edit_buffer(), "sk-ab");
    state.reduce(SettingsAction::Backspace);
    assert_eq!(state.edit_buffer(), "sk-a");
}

#[test]
fn backspace_on_empty_buffer_is_noop() {
    let mut state = SettingsState::new();
    state.start_editing("api_key", "");
    state.reduce(SettingsAction::Backspace);
    assert_eq!(state.edit_buffer(), "");
}

#[test]
fn cancel_edit_clears_buffer() {
    let mut state = SettingsState::new();
    state.start_editing("base_url", "https://example.com");
    state.reduce(SettingsAction::InsertChar('!'));
    state.reduce(SettingsAction::CancelEdit);
    assert!(!state.is_editing());
    assert_eq!(state.edit_buffer(), "");
}

#[test]
fn confirm_edit_keeps_buffer_value() {
    let mut state = SettingsState::new();
    state.start_editing("base_url", "https://");
    state.reduce(SettingsAction::InsertChar('x'));
    state.reduce(SettingsAction::ConfirmEdit);
    assert!(!state.is_editing());
    assert_eq!(state.edit_buffer(), "https://x");
}

#[test]
fn whatsapp_allowed_contacts_uses_textarea_mode() {
    let mut state = SettingsState::new();
    state.start_editing("whatsapp_allowed_contacts", "+15551234567");

    assert!(state.is_editing());
    assert!(state.is_textarea());
}

#[test]
fn whatsapp_allowed_contacts_preserves_newlines_in_textarea_mode() {
    let mut state = SettingsState::new();
    state.start_editing("whatsapp_allowed_contacts", "+15551234567");

    state.reduce(SettingsAction::InsertChar('\n'));
    state.reduce(SettingsAction::InsertChar('+'));

    assert_eq!(state.edit_buffer(), "+15551234567\n+");
}

#[test]
fn current_field_name_provider_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    assert_eq!(state.current_field_name(), "provider");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "base_url");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "auth_source");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "model");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "api_transport");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "assistant_id");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "reasoning_effort");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "context_window_tokens");
}

#[test]
fn current_field_name_tools_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
    assert_eq!(state.current_field_name(), "tool_bash");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_file_ops");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_web_search");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_web_browse");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_vision");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_system_info");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_gateway");
}

#[test]
fn current_field_name_gateway_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    assert_eq!(state.current_field_name(), "gateway_enabled");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "gateway_default_agent");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "gateway_prefix");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "slack_token");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "slack_channel_filter");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "slack_default_agent");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "telegram_token");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "telegram_allowed_chats");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "telegram_default_agent");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "discord_token");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "discord_channel_filter");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "discord_allowed_users");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "discord_default_agent");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_allowed_contacts");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_token");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_phone_id");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_default_agent");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_link_device");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "whatsapp_relink_device");
}

#[test]
fn current_field_name_agent_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    assert_eq!(state.current_field_name(), "provider");
    state.reduce(SettingsAction::NavigateField(7));
    assert_eq!(state.current_field_name(), "context_window_tokens");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "swarog_alias");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "system_prompt");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "backend");
}

#[test]
fn current_field_name_chat_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    assert_eq!(state.current_field_name(), "enable_streaming");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "enable_conversation_memory");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "enable_honcho_memory");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "anticipatory_enabled");
    state.reduce(SettingsAction::NavigateField(4));
    assert_eq!(state.current_field_name(), "operator_model_enabled");
    state.reduce(SettingsAction::NavigateField(11));
    assert_eq!(
        state.current_field_name(),
        "tool_synthesis_max_generated_tools"
    );
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tui_chat_history_page_size");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(
        state.current_field_name(),
        "participant_observer_restore_window_hours"
    );
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "operator_model_inspect");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "operator_model_reset");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "collaboration_sessions_inspect");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "generated_tools_inspect");
    state.reduce(SettingsAction::NavigateField(4));
    assert_eq!(state.current_field_name(), "generated_tools_inspect");
    assert_eq!(state.field_cursor(), 24);
}

#[test]
fn current_field_name_advanced_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
    assert_eq!(state.current_field_name(), "managed_sandbox_enabled");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "managed_security_level");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "auto_compact_context");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "compaction_strategy");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "max_context_messages");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "max_tool_loops");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "max_retries");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "retry_delay_ms");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "message_loop_delay_ms");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tool_call_delay_ms");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "llm_stream_chunk_timeout_secs");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "auto_retry");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "context_window_tokens");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "compact_threshold_pct");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "keep_recent_on_compact");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "bash_timeout_secs");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "weles_max_concurrent_reviews");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "snapshot_auto_cleanup");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "snapshot_max_count");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "snapshot_max_size_mb");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "snapshot_stats");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "auto_refresh_interval_secs");
    state.reduce(SettingsAction::NavigateField(5));
    assert_eq!(state.current_field_name(), "auto_refresh_interval_secs");
    assert_eq!(state.field_cursor(), 21);
}

#[test]
fn field_count_per_tab() {
    let mut state = SettingsState::new();
    assert_eq!(state.field_count(), 1);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    assert_eq!(state.field_count(), 8);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
    assert_eq!(state.field_count(), 7);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::WebSearch));
    assert_eq!(state.field_count(), 10);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    assert_eq!(state.field_count(), 25);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    assert_eq!(state.field_count(), 19);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Auth));
    assert_eq!(state.field_count(), 1);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    assert_eq!(state.field_count(), 12);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    assert_eq!(state.field_count(), 1);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
    assert_eq!(state.field_count(), 9);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Features));
    assert_eq!(state.field_count(), 30);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
    assert_eq!(state.field_count(), 25);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Plugins));
    assert_eq!(state.field_count(), 1);
    state.reduce(SettingsAction::SwitchTab(SettingsTab::About));
    assert_eq!(state.field_count(), 0);
}
