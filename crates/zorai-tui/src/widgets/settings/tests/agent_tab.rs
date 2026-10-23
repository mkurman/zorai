use super::*;
use crate::state::settings::{SettingsAction, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;

fn assert_rows_match_fields(provider: &str) {
    let mut config = ConfigState::new();
    config.provider = provider.to_string();
    let subagents = SubAgentsState::new();
    let mut settings = SettingsState::default();
    settings.reduce(SettingsAction::Open);
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));

    let count = settings.field_count_with_config(&config);
    for field_idx in 0..count {
        let field = settings.current_field_name_with_config(&config).to_string();
        assert!(
            !field.is_empty(),
            "{provider}: row {field_idx} has no field"
        );
        let row = render_agent_tab(&settings, &config, &ThemeTokens::default())
            .iter()
            .position(|line| {
                line.spans
                    .first()
                    .is_some_and(|span| span.content.trim() == ">")
            })
            .unwrap_or_else(|| panic!("{provider}: {field} is not rendered as selected"));
        assert_eq!(
            settings_row_hit(&settings, &config, &subagents, row),
            Some((field_idx, None)),
            "{provider}: clicking the row of {field} must select it"
        );
        if let Some((edit_row, _)) = single_line_edit_layout(&settings, &config, &field) {
            assert_eq!(edit_row, row, "{provider}: edit cursor row for {field}");
        }
        settings.navigate_field(1, count);
    }
}

#[test]
fn openrouter_routing_rows_are_reachable_and_do_not_shadow_the_identity_rows() {
    assert_rows_match_fields(zorai_shared::providers::PROVIDER_ID_OPENROUTER);
}

#[test]
fn huggingface_route_row_is_reachable_on_the_agent_tab() {
    assert_rows_match_fields(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE);
}

#[test]
fn plain_providers_keep_alias_prompt_and_claude_permissions_reachable() {
    assert_rows_match_fields(zorai_shared::providers::PROVIDER_ID_OPENAI);
}
