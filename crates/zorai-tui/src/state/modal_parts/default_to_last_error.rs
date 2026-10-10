use super::default_command_items::default_command_items;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    CommandPalette,
    Status,
    Statistics,
    PromptViewer,
    ThreadParticipants,
    ThreadHandoff,
    ThreadParticipantAgentPicker,
    ThreadParticipantActions,
    GatewayDefaultAgentPicker,
    ThreadPicker,
    GoalPicker,
    WorkspacePicker,
    WorkspaceCreate,
    WorkspaceCreateTask,
    WorkspaceReviewTask,
    WorkspaceEditTask,
    WorkspaceTaskDetail,
    WorkspaceTaskHistory,
    WorkspaceActorPicker,
    GoalStepActionPicker,
    ProviderPicker,
    ModelPicker,
    OpenRouterProviderPicker,
    RolePicker,
    OpenAIAuth,
    ErrorViewer,
    QueuedPrompts,
    ApprovalOverlay,
    GoalApprovalRejectPrompt,
    ApprovalCenter,
    OperatorQuestionOverlay,
    OperatorProfileOnboarding,
    ChatActionConfirm,
    PinnedBudgetExceeded,
    Settings,
    EffortPicker,
    ContextWindowEditor,
    CustomModelEditor,
    Notifications,
    ToolsPicker,
    ViewPicker,
    Help,
    WhatsAppLink,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThreadPickerTab {
    Swarog,
    Rarog,
    Weles,
    Goals,
    Workspace,
    Playgrounds,
    Internal,
    Gateway,
    Agent(String),
}

impl Default for ThreadPickerTab {
    fn default() -> Self {
        Self::Swarog
    }
}

impl ThreadPickerTab {
    pub fn is_playgrounds(&self) -> bool {
        matches!(self, Self::Playgrounds)
    }

    pub fn agent_id(&self) -> Option<&str> {
        match self {
            Self::Swarog => Some(zorai_protocol::AGENT_ID_SWAROG),
            Self::Rarog => Some(zorai_protocol::AGENT_ID_RAROG),
            Self::Weles => Some("weles"),
            Self::Agent(agent_id) => Some(agent_id.as_str()),
            Self::Goals | Self::Workspace | Self::Playgrounds | Self::Internal | Self::Gateway => {
                None
            }
        }
    }

    /// Server-side `agent_filter` value (matches the daemon's
    /// `persisted_thread_agent_name_filter` resolver).
    ///
    /// `None` means "no agent-based filter" — used for tabs that aren't
    /// scoped to a specific agent (Goals, Workspace, Playgrounds, Internal,
    /// Gateway). For those the daemon returns the full thread summary list.
    pub fn agent_filter(&self) -> Option<String> {
        match self {
            Self::Swarog => Some(zorai_protocol::AGENT_HANDLE_SVAROG.to_string()),
            Self::Rarog => Some("rarog".to_string()),
            Self::Weles => Some("weles".to_string()),
            Self::Agent(agent_id) => Some(agent_id.clone()),
            Self::Goals | Self::Workspace | Self::Playgrounds | Self::Internal | Self::Gateway => {
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    Push(ModalKind),
    Pop,
    RemoveAll(ModalKind),
    SetQuery(String),
    Navigate(i32),
    Execute,
    FuzzyFilter,
}

pub struct ModalState {
    stack: Vec<ModalKind>,
    command_query: String,
    command_palette_explicit_selection: bool,
    command_items: Vec<CommandItem>,
    filtered_indices: Vec<usize>,
    picker_cursor: usize,
    /// Override item count for non-command-palette pickers (providers, models, etc.)
    picker_item_count: Option<usize>,
    thread_picker_tab: ThreadPickerTab,
    whatsapp_link: WhatsAppLinkState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppLinkPhase {
    Idle,
    Starting,
    AwaitingScan,
    Connected,
    Error,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct WhatsAppLinkState {
    phase: WhatsAppLinkPhase,
    status_text: String,
    ascii_qr: Option<String>,
    expires_at_ms: Option<u64>,
    phone: Option<String>,
    last_error: Option<String>,
}

impl Default for WhatsAppLinkState {
    fn default() -> Self {
        Self {
            phase: WhatsAppLinkPhase::Idle,
            status_text: "Ready to start WhatsApp device linking".to_string(),
            ascii_qr: None,
            expires_at_ms: None,
            phone: None,
            last_error: None,
        }
    }
}

impl ModalState {
    pub fn new() -> Self {
        let items = default_command_items();
        let filtered = (0..items.len()).collect();
        Self {
            stack: Vec::new(),
            command_query: String::new(),
            command_palette_explicit_selection: false,
            command_items: items,
            filtered_indices: filtered,
            picker_cursor: 0,
            picker_item_count: None,
            thread_picker_tab: ThreadPickerTab::Swarog,
            whatsapp_link: WhatsAppLinkState::default(),
        }
    }

    pub fn top(&self) -> Option<ModalKind> {
        self.stack.last().copied()
    }
    pub fn contains(&self, kind: ModalKind) -> bool {
        self.stack.contains(&kind)
    }
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
    pub fn command_query(&self) -> &str {
        &self.command_query
    }
    pub fn command_display_query(&self) -> &str {
        &self.command_query
    }
    pub fn command_palette_has_explicit_selection(&self) -> bool {
        self.command_palette_explicit_selection
    }
    pub fn command_items(&self) -> &[CommandItem] {
        &self.command_items
    }
    pub fn filtered_items(&self) -> &[usize] {
        &self.filtered_indices
    }
    pub fn picker_cursor(&self) -> usize {
        self.picker_cursor
    }
    pub fn set_picker_cursor(&mut self, cursor: usize) {
        let max = self.picker_item_count.unwrap_or(0).saturating_sub(1);
        self.picker_cursor = cursor.min(max);
    }
    pub fn set_picker_item_count(&mut self, count: usize) {
        if self.top() == Some(ModalKind::CommandPalette) {
            return;
        }
        self.picker_item_count = Some(count);
        self.picker_cursor = if count == 0 {
            0
        } else {
            self.picker_cursor.min(count - 1)
        };
    }
    pub fn thread_picker_tab(&self) -> ThreadPickerTab {
        self.thread_picker_tab.clone()
    }
    pub fn set_thread_picker_tab(&mut self, tab: ThreadPickerTab) {
        self.thread_picker_tab = tab;
        self.picker_cursor = 0;
    }
    pub fn whatsapp_link(&self) -> &WhatsAppLinkState {
        &self.whatsapp_link
    }
    pub fn reset_whatsapp_link(&mut self) {
        self.whatsapp_link = WhatsAppLinkState::default();
    }
    pub fn set_whatsapp_link_starting(&mut self) {
        self.whatsapp_link.phase = WhatsAppLinkPhase::Starting;
        self.whatsapp_link.status_text = "Starting WhatsApp linking…".to_string();
        self.whatsapp_link.ascii_qr = None;
        self.whatsapp_link.expires_at_ms = None;
        self.whatsapp_link.phone = None;
        self.whatsapp_link.last_error = None;
    }
    pub fn set_whatsapp_link_status(
        &mut self,
        state: &str,
        phone: Option<String>,
        last_error: Option<String>,
    ) {
        self.whatsapp_link.phone = phone;
        self.whatsapp_link.last_error = last_error;
        match state {
            "starting" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Starting;
                self.whatsapp_link.status_text = "Starting WhatsApp linking…".to_string();
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
            }
            "qr_ready" | "awaiting_qr" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::AwaitingScan;
                self.whatsapp_link.status_text =
                    "Scan the QR code in WhatsApp on your phone".to_string();
            }
            "connected" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Connected;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let phone_display = self.whatsapp_link.phone.as_deref().unwrap_or("device");
                self.whatsapp_link.status_text = format!("Connected: {phone_display}");
            }
            "error" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Error;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let message = self
                    .whatsapp_link
                    .last_error
                    .as_deref()
                    .unwrap_or("Unknown WhatsApp linking error");
                self.whatsapp_link.status_text = format!("Error: {message}");
            }
            "disconnected" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Disconnected;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let reason = self
                    .whatsapp_link
                    .last_error
                    .as_deref()
                    .unwrap_or("Disconnected");
                self.whatsapp_link.status_text = format!("Disconnected: {reason}");
            }
            _ => {}
        }
    }
    pub fn set_whatsapp_link_qr(&mut self, ascii_qr: String, expires_at_ms: Option<u64>) {
        self.whatsapp_link.phase = WhatsAppLinkPhase::AwaitingScan;
        self.whatsapp_link.status_text = "Scan the QR code in WhatsApp on your phone".to_string();
        self.whatsapp_link.ascii_qr = Some(ascii_qr);
        self.whatsapp_link.expires_at_ms = expires_at_ms;
        self.whatsapp_link.last_error = None;
    }
    pub fn set_whatsapp_link_connected(&mut self, phone: Option<String>) {
        self.whatsapp_link.phone = phone;
        self.set_whatsapp_link_status("connected", self.whatsapp_link.phone.clone(), None);
    }
    pub fn set_whatsapp_link_error(&mut self, message: String) {
        self.set_whatsapp_link_status("error", self.whatsapp_link.phone.clone(), Some(message));
    }
    pub fn set_whatsapp_link_disconnected(&mut self, reason: Option<String>) {
        let preserved_reason = reason.or_else(|| self.whatsapp_link.last_error.clone());
        self.set_whatsapp_link_status(
            "disconnected",
            self.whatsapp_link.phone.clone(),
            preserved_reason,
        );
    }

    /// Merge plugin commands into the command palette.
    /// Removes any previously added plugin commands, then appends the new ones.
    pub fn set_plugin_commands(&mut self, commands: Vec<CommandItem>) {
        self.command_items
            .retain(|item| !item.command.contains('.'));
        self.command_items.extend(commands);
        self.refilter();
        let len = if self.top() == Some(ModalKind::CommandPalette) {
            self.filtered_indices.len()
        } else {
            self.picker_item_count
                .unwrap_or(self.filtered_indices.len())
        };
        self.picker_cursor = if len == 0 {
            0
        } else {
            self.picker_cursor.min(len - 1)
        };
    }

    pub fn reduce(&mut self, action: ModalAction) {
        match action {
            ModalAction::Push(kind) => {
                self.stack.push(kind);
                self.command_query.clear();
                self.command_palette_explicit_selection = false;
                self.picker_cursor = 0;
                self.picker_item_count = None;
                if kind == ModalKind::ThreadPicker {
                    self.thread_picker_tab = ThreadPickerTab::Swarog;
                } else if kind == ModalKind::RolePicker {
                    self.picker_item_count =
                        Some(crate::state::subagents::role_picker_item_count());
                }
                self.refilter();
            }
            ModalAction::Pop => {
                self.stack.pop();
                self.command_query.clear();
                self.command_palette_explicit_selection = false;
                self.picker_cursor = 0;
                self.picker_item_count = None;
                self.refilter();
            }
            ModalAction::RemoveAll(kind) => {
                let top_before = self.top();
                self.stack.retain(|entry| *entry != kind);
                if self.top() != top_before {
                    self.command_query.clear();
                    self.command_palette_explicit_selection = false;
                    self.picker_cursor = 0;
                    self.picker_item_count = None;
                    self.refilter();
                }
            }
            ModalAction::SetQuery(query) => {
                self.command_query = if self.top() == Some(ModalKind::CommandPalette) {
                    query.trim_start_matches('/').to_string()
                } else {
                    query
                };
                self.command_palette_explicit_selection = false;
                self.refilter();
                self.picker_cursor = 0;
                if self.top() == Some(ModalKind::CommandPalette) {
                    self.picker_item_count = None;
                }
            }
            ModalAction::Navigate(delta) => {
                let len = if self.top() == Some(ModalKind::CommandPalette) {
                    self.filtered_indices.len()
                } else {
                    self.picker_item_count
                        .unwrap_or(self.filtered_indices.len())
                };
                if len == 0 {
                    return;
                }
                if delta > 0 {
                    self.picker_cursor = (self.picker_cursor + delta as usize).min(len - 1);
                } else {
                    self.picker_cursor = self.picker_cursor.saturating_sub((-delta) as usize);
                }
                if self.top() == Some(ModalKind::CommandPalette) {
                    self.command_palette_explicit_selection = true;
                }
            }
            ModalAction::Execute => {}
            ModalAction::FuzzyFilter => {
                self.refilter();
            }
        }
    }

    /// Get the currently selected command (if any)
    pub fn selected_command(&self) -> Option<&CommandItem> {
        self.filtered_indices
            .get(self.picker_cursor)
            .and_then(|&idx| self.command_items.get(idx))
    }

    fn refilter(&mut self) {
        let query = self.command_query.to_lowercase();
        if query.is_empty() {
            self.filtered_indices = (0..self.command_items.len()).collect();
        } else {
            let q = query.strip_prefix('/').unwrap_or(&query);
            let q = q.split_whitespace().next().unwrap_or(q);
            self.filtered_indices = self
                .command_items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.command.to_lowercase().contains(q)
                        || item.description.to_lowercase().contains(q)
                })
                .map(|(idx, _)| idx)
                .collect();
        }
    }
}

impl WhatsAppLinkState {
    pub fn phase(&self) -> WhatsAppLinkPhase {
        self.phase
    }
    pub fn status_text(&self) -> &str {
        &self.status_text
    }
    pub fn ascii_qr(&self) -> Option<&str> {
        self.ascii_qr.as_deref()
    }
    pub fn expires_at_ms(&self) -> Option<u64> {
        self.expires_at_ms
    }
    pub fn phone(&self) -> Option<&str> {
        self.phone.as_deref()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}
