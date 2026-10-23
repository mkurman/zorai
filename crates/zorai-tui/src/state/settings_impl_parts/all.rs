use super::*;
impl SettingsTab {
    const ALL: &'static [SettingsTab] = &[
        SettingsTab::Auth,
        SettingsTab::Agent,
        SettingsTab::Concierge,
        SettingsTab::SubAgents,
        SettingsTab::Tools,
        SettingsTab::Skills,
        SettingsTab::WebSearch,
        SettingsTab::Media,
        SettingsTab::Memory,
        SettingsTab::Heartbeat,
        SettingsTab::Proactivity,
        SettingsTab::Workflows,
        SettingsTab::Gateway,
        SettingsTab::Chat,
        SettingsTab::Compaction,
        SettingsTab::Execution,
        SettingsTab::Safety,
        SettingsTab::Mlflow,
        SettingsTab::Storage,
        SettingsTab::Plugins,
        SettingsTab::Database,
        SettingsTab::About,
    ];

    pub fn all() -> &'static [SettingsTab] {
        Self::ALL
    }
}
