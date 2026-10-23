use super::*;
use crate::state::settings::{SettingsAction, SettingsState, SettingsTab};
use crate::state::settings_registry::{settings_page_spec, SETTINGS_PAGE_VALUE_COL};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;

fn page_tabs() -> Vec<SettingsTab> {
    SettingsTab::all()
        .iter()
        .copied()
        .filter(|tab| settings_page_spec(*tab).is_some())
        .collect()
}

fn configs() -> Vec<ConfigState> {
    ["none", "duckduckgo", "firecrawl", "exa", "tavily"]
        .into_iter()
        .map(|provider| {
            let mut config = ConfigState::new();
            config.search_provider = provider.to_string();
            config
        })
        .collect()
}

#[test]
fn every_page_field_is_drawn_clicked_and_edited_on_the_same_row() {
    let subagents = SubAgentsState::new();
    for tab in page_tabs() {
        let page = settings_page_spec(tab).expect("page");
        for config in configs() {
            let mut settings = SettingsState::default();
            settings.reduce(SettingsAction::Open);
            settings.reduce(SettingsAction::SwitchTab(tab));
            let fields = page.visible_fields(&config);
            assert_eq!(settings.field_count_with_config(&config), fields.len());
            for (index, spec) in fields.iter().enumerate() {
                assert_eq!(settings.current_field_name_with_config(&config), spec.name);
                let lines = render_settings_page(page, &settings, &config, &ThemeTokens::default());
                let selected = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.to_string().starts_with("> "))
                    .map(|(row, _)| row)
                    .collect::<Vec<_>>();
                assert_eq!(selected.len(), 1, "{tab:?}/{}: {selected:?}", spec.name);
                let row = selected[0];
                assert!(
                    lines[row].to_string().contains(spec.label),
                    "{tab:?}/{} highlighted the wrong row",
                    spec.name
                );
                assert_eq!(
                    settings_row_hit(&settings, &config, &subagents, row),
                    Some((index, None)),
                    "{tab:?}: clicking {} must select it",
                    spec.name
                );
                assert_eq!(
                    single_line_edit_layout(&settings, &config, spec.name),
                    Some((row, SETTINGS_PAGE_VALUE_COL)),
                    "{tab:?}: edit cursor for {}",
                    spec.name
                );
                settings.navigate_field(1, fields.len());
            }
        }
    }
}

#[test]
fn web_page_shows_only_the_selected_providers_settings() {
    let page = settings_page_spec(SettingsTab::WebSearch).expect("web page");
    for config in configs() {
        let names = page
            .visible_fields(&config)
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        let keys = ["firecrawl_api_key", "exa_api_key", "tavily_api_key"]
            .into_iter()
            .filter(|name| names.contains(name))
            .collect::<Vec<_>>();
        match config.search_provider.as_str() {
            "firecrawl" => assert_eq!(keys, vec!["firecrawl_api_key"]),
            "exa" => assert_eq!(keys, vec!["exa_api_key"]),
            "tavily" => assert_eq!(keys, vec!["tavily_api_key"]),
            _ => assert!(keys.is_empty(), "{}: {keys:?}", config.search_provider),
        }
        assert_eq!(
            names.contains(&"duckduckgo_region"),
            config.search_provider == "duckduckgo"
        );
    }
}

#[test]
fn sidebar_lists_every_tab_once_in_the_order_tab_key_cycles() {
    let sidebar_tabs = settings_sidebar_rows()
        .into_iter()
        .filter_map(|row| match row {
            SettingsSidebarRow::Tab(tab, _) => Some(tab),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sidebar_tabs, SettingsTab::all().to_vec());
}

#[test]
fn clicking_a_sidebar_entry_selects_that_tab_and_narrow_terminals_keep_the_tab_strip() {
    let area = ratatui::layout::Rect::new(0, 0, 100, 40);
    let layout = settings_layout(area).expect("layout");
    let sidebar = layout.sidebar.expect("wide modal shows a sidebar");
    let rows = settings_sidebar_rows();
    for (row, entry) in rows.iter().enumerate() {
        if let SettingsSidebarRow::Tab(tab, _) = entry {
            assert_eq!(
                sidebar_hit_test(
                    sidebar,
                    SettingsTab::Auth,
                    ratatui::layout::Position::new(sidebar.x + 3, sidebar.y + row as u16)
                ),
                Some(*tab)
            );
        }
    }
    let narrow = settings_layout(ratatui::layout::Rect::new(0, 0, 60, 30)).expect("layout");
    assert!(narrow.sidebar.is_none() && narrow.tabs_line.is_some());
}
