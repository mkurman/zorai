use super::*;
use crate::state::concierge::ConciergeState;
use crate::state::settings::{SettingsAction, SettingsState, SettingsTab};
use crate::theme::ThemeTokens;

const LABELS: &[(&str, &str)] = &[
    ("concierge_api_transport", "Transport"),
    ("concierge_claude_permission_mode", "Claude perms"),
    ("concierge_huggingface_provider", "HF Route"),
    ("concierge_openrouter_provider_order", "OR Prefer"),
    ("concierge_openrouter_provider_ignore", "OR Exclude"),
    ("concierge_openrouter_allow_fallbacks", "OR Fallback"),
];

fn assert_each_field_highlights_its_own_row(provider: Option<&str>) {
    let mut concierge = ConciergeState::new();
    concierge.provider = provider.map(str::to_string);
    let fields = concierge.settings_fields();
    let mut settings = SettingsState::default();
    settings.reduce(SettingsAction::Open);
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));

    for (index, field) in fields.iter().enumerate() {
        let selected = render_concierge_tab(&settings, &concierge, &ThemeTokens::default())
            .iter()
            .map(|line| line.to_string())
            .filter(|line| line.starts_with("> "))
            .collect::<Vec<_>>();
        assert_eq!(
            selected.len(),
            1,
            "{provider:?}: cursor {index} ({field}) highlights {selected:?}"
        );
        if let Some((_, label)) = LABELS.iter().find(|(name, _)| name == field) {
            assert!(
                selected[0].contains(label),
                "{provider:?}: {field} highlights {:?}",
                selected[0]
            );
        }
        settings.navigate_field(1, fields.len());
    }
}

#[test]
fn transport_is_reachable_and_each_rarog_row_highlights_alone_for_every_provider_shape() {
    assert_each_field_highlights_its_own_row(None);
    assert_each_field_highlights_its_own_row(Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER));
    assert_each_field_highlights_its_own_row(Some(
        zorai_shared::providers::PROVIDER_ID_HUGGINGFACE,
    ));
}
