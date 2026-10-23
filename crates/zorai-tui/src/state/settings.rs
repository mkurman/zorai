#![allow(dead_code)]

#[path = "settings_cursor.rs"]
mod cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Auth,
    Provider,
    Tools,
    WebSearch,
    Chat,
    Mlflow,
    Gateway,
    Agent,
    SubAgents,
    Concierge,
    Skills,
    Media,
    Memory,
    Heartbeat,
    Proactivity,
    Workflows,
    Compaction,
    Execution,
    Safety,
    Storage,
    Plugins,
    Database,
    About,
}

#[path = "settings_impl_parts/all.rs"]
mod all;

#[derive(Debug, Clone)]
pub enum SettingsAction {
    Open,
    Close,
    SwitchTab(SettingsTab),
    NavigateField(i32),
    EditField,
    ConfirmEdit,
    CancelEdit,
    InsertChar(char),
    Backspace,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorUp,
    MoveCursorDown,
    MoveCursorHome,
    MoveCursorEnd,
    SetCursor(usize),
    SetCursorLineCol(usize, usize),
    ToggleCheckbox,
    SelectRadio,
    OpenDropdown,
    NavigateDropdown(i32),
    SelectDropdown,
    Save,
}

pub struct SettingsState {
    active_tab: SettingsTab,
    field_cursor: usize,
    editing_field: Option<String>,
    edit_buffer: String,
    edit_cursor: usize,
    textarea_mode: bool,
    dropdown_open: bool,
    dropdown_cursor: usize,
    dirty: bool,
}

pub fn is_tool_toggle_field(field: &str) -> bool {
    matches!(
        field,
        "tool_bash"
            | "tool_file_ops"
            | "tool_web_search"
            | "tool_web_browse"
            | "tool_vision"
            | "tool_system_info"
            | "tool_gateway"
    )
}

fn field_uses_textarea(field: &str) -> bool {
    matches!(
        field,
        "system_prompt"
            | "subagent_system_prompt"
            | "whatsapp_allowed_contacts"
            | "workspace_repo_monitor_include_dirs"
            | "workspace_repo_monitor_exclude_dirs"
    )
}

#[path = "settings_impl_parts/advanced_field_names_for_strategy_to_navigate_field.rs"]
mod advanced_field_names_for_strategy_to_navigate_field;
#[path = "settings_impl_parts/reduce.rs"]
mod reduce;

#[path = "settings_impl_parts/default.rs"]
mod default;

/// State for the Plugins settings tab. Stored separately from SettingsState
/// because plugin data is dynamic (varies by installed plugins).
#[derive(Debug, Clone)]
pub struct PluginSettingsState {
    /// List of installed plugins (from daemon PluginListResult).
    pub plugins: Vec<PluginListItem>,
    /// Index of selected plugin in the list.
    pub selected_index: usize,
    /// Settings schema fields for the selected plugin (parsed from manifest JSON).
    pub schema_fields: Vec<PluginSchemaField>,
    /// Current setting values for the selected plugin (from daemon).
    pub settings_values: Vec<(String, String, bool)>,
    /// Whether we're in plugin list mode (true) or plugin detail mode (false).
    pub list_mode: bool,
    /// Test connection result message (None = not tested yet).
    pub test_result: Option<(bool, String)>,
    /// Loading flag.
    pub loading: bool,
    /// Field cursor in detail mode (indexes into schema_fields + action buttons).
    pub detail_cursor: usize,
    /// Whether the list view is showing the plugin install prompt.
    pub install_mode: bool,
    /// Source entered in the plugin install prompt.
    pub install_source_buffer: String,
    /// Cursor position in the plugin install source buffer.
    pub install_source_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct PluginListItem {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub has_api: bool,
    pub has_auth: bool,
    pub settings_count: u32,
    pub description: Option<String>,
    pub install_source: String,
    pub auth_status: String,
    pub connector_kind: Option<String>,
    pub readiness_state: String,
    pub readiness_message: Option<String>,
    pub recovery_hint: Option<String>,
    pub setup_hint: Option<String>,
    pub docs_path: Option<String>,
    pub workflow_primitives: Vec<String>,
    pub read_actions: Vec<String>,
    pub write_actions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PluginSchemaField {
    pub key: String,
    pub field_type: String,
    pub label: String,
    pub required: bool,
    pub secret: bool,
    pub options: Option<Vec<String>>,
    pub description: Option<String>,
}

#[path = "settings_impl_parts/new_to_is_key_secret.rs"]
mod new_to_is_key_secret;

#[path = "settings_impl_parts/default_02.rs"]
mod default_02;

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
