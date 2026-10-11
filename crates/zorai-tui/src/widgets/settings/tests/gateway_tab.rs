use super::*;
use crate::state::modal::ModalState;
use crate::state::settings::{SettingsAction, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;

#[test]
fn gateway_click_and_edit_rows_match_the_rendered_row_of_every_field() {
    let config = ConfigState::new();
    let modal = ModalState::new();
    let subagents = SubAgentsState::new();
    let mut settings = SettingsState::default();
    settings.reduce(SettingsAction::Open);
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));

    for field_idx in 0..settings.field_count() {
        let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
        let row = lines
            .iter()
            .position(|line| line.spans.first().is_some_and(|span| span.content == "> "))
            .expect("selected gateway field is rendered");
        let field = settings.current_field_name().to_string();
        assert_eq!(
            settings_row_hit(&settings, &config, &subagents, row),
            Some((field_idx, None)),
            "clicking the row of {field} must select it"
        );
        if let Some((edit_row, _)) = single_line_edit_layout(&settings, &config, &field) {
            assert_eq!(edit_row, row, "edit cursor row for {field}");
        }
        settings.reduce(SettingsAction::NavigateField(1));
    }
}

#[test]
fn platform_agent_rows_explain_an_empty_value_falls_back_to_the_gateway_default() {
    let mut config = ConfigState::new();
    config.discord_default_agent = "weles".to_string();
    let settings = SettingsState::default();
    let text: String = render_gateway_tab(
        &settings,
        &config,
        &ModalState::new(),
        &ThemeTokens::default(),
    )
    .iter()
    .map(|line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
            + "\n"
    })
    .collect();
    assert_eq!(text.matches("(gateway default)").count(), 3);
    assert!(text.contains("weles"));
}
