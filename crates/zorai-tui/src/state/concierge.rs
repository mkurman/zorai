#![allow(dead_code)]

/// TUI-side concierge state.
pub struct ConciergeState {
    pub enabled: bool,
    pub detail_level: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: String,
    pub reasoning_effort: Option<String>,
    pub api_transport: Option<String>,
    pub claude_permission_mode: Option<String>,
    pub openrouter_provider_order: String,
    pub openrouter_provider_ignore: String,
    pub openrouter_allow_fallbacks: bool,
    pub huggingface_provider: String,
    pub auto_cleanup_on_navigate: bool,
    pub loading: bool,
    pub welcome_content: Option<String>,
    pub welcome_actions: Vec<ConciergeActionVm>,
    pub welcome_visible: bool,
    pub selected_action: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConciergeActionVm {
    pub label: String,
    pub action_type: String,
    pub thread_id: Option<String>,
}

impl ConciergeState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            detail_level: "proactive_triage".into(),
            provider: None,
            model: None,
            base_url: String::new(),
            reasoning_effort: None,
            api_transport: None,
            claude_permission_mode: None,
            openrouter_provider_order: String::new(),
            openrouter_provider_ignore: String::new(),
            openrouter_allow_fallbacks: true,
            huggingface_provider: String::new(),
            auto_cleanup_on_navigate: true,
            loading: false,
            welcome_content: None,
            welcome_actions: Vec::new(),
            welcome_visible: false,
            selected_action: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConciergeAction {
    ConfigReceived {
        enabled: bool,
        detail_level: String,
        provider: Option<String>,
        model: Option<String>,
        base_url: String,
        reasoning_effort: Option<String>,
        api_transport: Option<String>,
        claude_permission_mode: Option<String>,
        openrouter_provider_order: String,
        openrouter_provider_ignore: String,
        openrouter_allow_fallbacks: bool,
        huggingface_provider: String,
        auto_cleanup_on_navigate: bool,
    },
    WelcomeLoading(bool),
    WelcomeReceived {
        content: String,
        actions: Vec<ConciergeActionVm>,
    },
    SelectAction(usize),
    NavigateAction(i32),
    WelcomeDismissed,
}

impl ConciergeState {
    pub fn settings_fields(&self) -> Vec<&'static str> {
        let provider = self.provider.as_deref();
        let openrouter = provider == Some(zorai_shared::providers::PROVIDER_ID_OPENROUTER);
        let mut fields = vec![
            "concierge_enabled",
            "concierge_detail_level",
            "concierge_provider",
            "concierge_model",
            "concierge_reasoning_effort",
            "concierge_api_transport",
        ];
        if !openrouter {
            fields.push("concierge_claude_permission_mode");
        }
        if provider == Some(zorai_shared::providers::PROVIDER_ID_HUGGINGFACE) {
            fields.push("concierge_huggingface_provider");
        }
        if openrouter {
            fields.extend([
                "concierge_openrouter_provider_order",
                "concierge_openrouter_provider_ignore",
                "concierge_openrouter_allow_fallbacks",
            ]);
        }
        fields.extend(["concierge_base_url", "rarog_alias"]);
        fields
    }

    pub fn settings_field_index(&self, field: &str) -> usize {
        self.settings_fields()
            .iter()
            .position(|name| *name == field)
            .unwrap_or(usize::MAX)
    }

    pub fn has_active_welcome(&self) -> bool {
        self.welcome_visible
            && self
                .welcome_content
                .as_ref()
                .is_some_and(|content| !content.trim().is_empty())
    }

    pub fn is_same_welcome(&self, content: &str, actions: &[ConciergeActionVm]) -> bool {
        self.welcome_visible
            && self.welcome_content.as_deref() == Some(content)
            && self.welcome_actions.as_slice() == actions
    }

    pub fn reduce(&mut self, action: ConciergeAction) {
        match action {
            ConciergeAction::ConfigReceived {
                enabled,
                detail_level,
                provider,
                model,
                base_url,
                reasoning_effort,
                api_transport,
                claude_permission_mode,
                openrouter_provider_order,
                openrouter_provider_ignore,
                openrouter_allow_fallbacks,
                huggingface_provider,
                auto_cleanup_on_navigate,
            } => {
                self.enabled = enabled;
                self.detail_level = detail_level;
                self.provider = provider;
                self.model = model;
                self.base_url = base_url;
                self.reasoning_effort = reasoning_effort;
                self.api_transport = api_transport;
                self.claude_permission_mode = claude_permission_mode;
                self.openrouter_provider_order = openrouter_provider_order;
                self.openrouter_provider_ignore = openrouter_provider_ignore;
                self.openrouter_allow_fallbacks = openrouter_allow_fallbacks;
                self.huggingface_provider = huggingface_provider;
                self.auto_cleanup_on_navigate = auto_cleanup_on_navigate;
                if !enabled {
                    self.loading = false;
                }
            }
            ConciergeAction::WelcomeLoading(loading) => {
                self.loading = loading && self.enabled;
            }
            ConciergeAction::WelcomeReceived { content, actions } => {
                self.loading = false;
                self.welcome_content = Some(content);
                self.welcome_actions = actions;
                self.welcome_visible = true;
                self.selected_action = 0;
            }
            ConciergeAction::SelectAction(index) => {
                if index < self.welcome_actions.len() {
                    self.selected_action = index;
                }
            }
            ConciergeAction::NavigateAction(delta) => {
                if self.welcome_actions.is_empty() {
                    self.selected_action = 0;
                } else if delta > 0 {
                    self.selected_action =
                        (self.selected_action + delta as usize).min(self.welcome_actions.len() - 1);
                } else {
                    self.selected_action = self.selected_action.saturating_sub((-delta) as usize);
                }
            }
            ConciergeAction::WelcomeDismissed => {
                self.loading = false;
                self.welcome_content = None;
                self.welcome_actions.clear();
                self.welcome_visible = false;
                self.selected_action = 0;
            }
        }
    }
}
