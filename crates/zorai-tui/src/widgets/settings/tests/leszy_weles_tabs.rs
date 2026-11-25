use super::*;
use crate::state::settings::{SettingsAction, SettingsState, SettingsTab};
use crate::state::settings_registry::settings_page_spec;
use crate::state::subagents::{SubAgentEntry, SubAgentsState};
use crate::theme::ThemeTokens;

fn entry(id: &str, name: &str) -> SubAgentEntry {
    SubAgentEntry {
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: None,
        enabled: true,
        builtin: id.ends_with("_builtin"),
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        api_transport: None,
        claude_permission_mode: None,
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        huggingface_provider: String::new(),
        raw_json: Some(serde_json::json!({ "id": id })),
    }
}

fn render(tab: SettingsTab, config: &ConfigState, subagents: &SubAgentsState) -> String {
    let mut settings = SettingsState::new();
    settings.reduce(SettingsAction::SwitchTab(tab));
    render_tab_content(
        100,
        &settings,
        config,
        &crate::state::modal::ModalState::new(),
        &crate::state::auth::AuthState::new(),
        subagents,
        &crate::state::concierge::ConciergeState::new(),
        &crate::state::settings::PluginSettingsState::new(),
        &ThemeTokens::default(),
    )
    .iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n")
}

fn page_field_names(tab: SettingsTab) -> Vec<&'static str> {
    settings_page_spec(tab)
        .map(|page| page.all_fields().map(|spec| spec.name).collect())
        .unwrap_or_default()
}

#[test]
fn weles_is_edited_from_weles_tab_not_subagents_list() {
    let mut subagents = SubAgentsState::new();
    subagents.entries = vec![entry("weles_builtin", "WELES"), entry("qa", "QA")];
    subagents.selected = 0;

    let text = render(SettingsTab::SubAgents, &ConfigState::new(), &subagents);

    assert!(
        text.contains("QA"),
        "regular sub-agents stay listed: {text}"
    );
    assert!(
        !text.contains("WELES"),
        "WELES lives in its own tab, so the sub-agent list must not offer it: {text}"
    );
    assert_eq!(
        subagents.selected_entry().map(|e| e.id.as_str()),
        Some("qa")
    );
}

#[test]
fn weles_settings_are_moved_not_duplicated() {
    let weles = page_field_names(SettingsTab::Weles);
    for field in [
        "managed_security_level",
        "weles_max_concurrent_reviews",
        "compaction_weles_provider",
        "compaction_weles_model",
        "compaction_weles_reasoning_effort",
        "compaction_weles_api_transport",
    ] {
        assert!(weles.contains(&field), "{field} belongs to the Weles tab");
        for tab in [
            SettingsTab::Safety,
            SettingsTab::Execution,
            SettingsTab::Compaction,
        ] {
            assert!(
                !page_field_names(tab).contains(&field),
                "{field} must not stay duplicated in {tab:?}"
            );
        }
    }
    assert!(page_field_names(SettingsTab::Compaction).contains(&"compaction_strategy"));
}

#[test]
fn leszy_empty_values_inherit_from_svarog() {
    let mut config = ConfigState::new();
    config.agent_config_raw = Some(serde_json::json!({ "coder": { "model": "gpt-5.4" } }));

    let text = render(SettingsTab::Leszy, &config, &SubAgentsState::new());
    let provider_line = text
        .lines()
        .find(|line| line.contains("Provider"))
        .unwrap_or_default();
    let model_line = text
        .lines()
        .find(|line| line.contains("Model") && !line.trim().eq("Model"))
        .unwrap_or_default();

    assert!(
        provider_line.contains("(use Svarog)"),
        "unset Leszy provider falls back to Svarog: {text}"
    );
    assert!(
        model_line.contains("gpt-5.4") && !model_line.contains("(use Svarog)"),
        "an explicit Leszy model overrides Svarog: {text}"
    );
}
