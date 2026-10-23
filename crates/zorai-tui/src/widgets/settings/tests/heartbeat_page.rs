use super::*;
use crate::state::settings::{SettingsState, SettingsTab};
use crate::state::settings_registry::settings_page_spec;
use crate::theme::ThemeTokens;

fn page_text(tab: SettingsTab, config: &ConfigState) -> String {
    render_settings_page(
        settings_page_spec(tab).expect("registry page"),
        &SettingsState::default(),
        config,
        &ThemeTokens::default(),
    )
    .iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n")
}

#[test]
fn heartbeat_rows_show_the_daemon_values_the_scheduler_actually_reads() {
    let mut config = ConfigState::new();
    config.agent_config_raw = Some(serde_json::json!({
        "heartbeat_interval_mins": 120,
        "heartbeat_cron": null,
        "quiet_hours_start": 22,
        "quiet_hours_end": null,
        "heartbeat_checks": { "repo_changes_enabled": false, "plugin_auth_enabled": true }
    }));
    let text = page_text(SettingsTab::Heartbeat, &config);

    let heartbeat_toggle = text
        .lines()
        .find(|line| {
            line.trim_start_matches(['>', ' ']).starts_with("Heartbeat") && line.contains('[')
        })
        .unwrap_or_else(|| panic!("heartbeat toggle row missing: {text}"));
    assert!(
        heartbeat_toggle.contains("[ ]"),
        "a config saved before heartbeat_enabled existed must show heartbeats off: {heartbeat_toggle}"
    );
    assert!(text.contains("120 min"), "{text}");
    assert!(text.contains("(uses interval)"), "{text}");
    assert!(text.contains("22:00"), "{text}");
    let repo_row = text
        .lines()
        .find(|line| line.contains("Repo changes"))
        .expect("repo changes row");
    assert!(repo_row.contains("[ ]"), "{repo_row}");
}

#[test]
fn storage_page_shows_repo_monitor_state_and_snapshot_usage() {
    let mut config = ConfigState::new();
    config.workspace_repo_monitor_enabled = true;
    config.workspace_repo_monitor_include_dirs = "crates\nfrontend/src".to_string();
    config.snapshot_count = 4;
    config.snapshot_total_size_bytes = 3 * 1024 * 1024;
    let text = page_text(SettingsTab::Storage, &config);

    let monitor_row = text
        .lines()
        .find(|line| line.contains("Repo monitor"))
        .expect("repo monitor row");
    assert!(monitor_row.contains("[x]"), "{monitor_row}");
    assert!(text.contains("crates, frontend/src"), "{text}");
    assert!(text.contains("4 snapshots stored, 3.0 MB"), "{text}");
}
