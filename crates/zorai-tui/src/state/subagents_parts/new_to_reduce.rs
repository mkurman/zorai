use super::visible_for_provider_to_normalize_role_preset_id::SubAgentEditorState;

/// TUI-side sub-agent entry mirroring the daemon's SubAgentDefinition.
#[derive(Debug, Clone)]
pub struct SubAgentEntry {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub role: Option<String>,
    pub enabled: bool,
    pub builtin: bool,
    pub immutable_identity: bool,
    pub disable_allowed: bool,
    pub delete_allowed: bool,
    pub protected_reason: Option<String>,
    pub reasoning_effort: Option<String>,
    pub api_transport: Option<String>,
    pub claude_permission_mode: Option<String>,
    pub openrouter_provider_order: String,
    pub openrouter_provider_ignore: String,
    pub openrouter_allow_fallbacks: bool,
    pub huggingface_provider: String,
    pub raw_json: Option<serde_json::Value>,
}

/// TUI state for the Sub-Agents settings tab.
pub struct SubAgentsState {
    pub entries: Vec<SubAgentEntry>,
    pub selected: usize,
    pub editing: Option<String>,
    pub actions_focused: bool,
    pub action_cursor: usize,
    pub editor: Option<SubAgentEditorState>,
}

impl SubAgentsState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            editing: None,
            actions_focused: false,
            action_cursor: 0,
            editor: None,
        }
    }

    pub fn list_entries(&self) -> Vec<&SubAgentEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.id != "weles_builtin")
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&SubAgentEntry> {
        self.list_entries().get(self.selected).copied()
    }
}

#[derive(Debug, Clone)]
pub enum SubAgentsAction {
    ListReceived(Vec<SubAgentEntry>),
    Added(SubAgentEntry),
    Removed(String),
    Updated(SubAgentEntry),
    Select(usize),
    StartEdit(String),
    CancelEdit,
    ToggleEnabled(String),
}

impl SubAgentsState {
    pub fn reduce(&mut self, action: SubAgentsAction) {
        match action {
            SubAgentsAction::ListReceived(entries) => {
                self.entries = entries;
                let list_len = self.list_entries().len();
                if self.selected >= list_len {
                    self.selected = list_len.saturating_sub(1);
                }
                if list_len == 0 {
                    self.actions_focused = false;
                    self.action_cursor = 0;
                }
            }
            SubAgentsAction::Added(entry) => {
                self.entries.push(entry);
                self.selected = self.list_entries().len().saturating_sub(1);
                self.actions_focused = false;
            }
            SubAgentsAction::Removed(id) => {
                self.entries.retain(|e| e.id != id);
                let list_len = self.list_entries().len();
                if self.selected >= list_len {
                    self.selected = list_len.saturating_sub(1);
                }
                if list_len == 0 {
                    self.actions_focused = false;
                    self.action_cursor = 0;
                }
            }
            SubAgentsAction::Updated(entry) => {
                if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
                    *existing = entry;
                }
            }
            SubAgentsAction::Select(idx) => {
                if idx < self.list_entries().len() {
                    self.selected = idx;
                }
            }
            SubAgentsAction::StartEdit(id) => {
                self.editing = Some(id);
            }
            SubAgentsAction::CancelEdit => {
                self.editing = None;
            }
            SubAgentsAction::ToggleEnabled(id) => {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                    entry.enabled = !entry.enabled;
                }
            }
        }
    }
}
