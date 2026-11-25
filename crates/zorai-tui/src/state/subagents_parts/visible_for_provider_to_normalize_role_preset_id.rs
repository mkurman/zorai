pub const CLAUDE_PERMISSION_MODE_OPTIONS: &[&str] = &[
    "",
    "default",
    "acceptEdits",
    "plan",
    "bypassPermissions",
    "dontAsk",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentEditorField {
    Name,
    Provider,
    Model,
    BaseUrl,
    ContextWindowTokens,
    OpenRouterProviderOrder,
    OpenRouterProviderIgnore,
    OpenRouterAllowFallbacks,
    HuggingFaceProvider,
    ReasoningEffort,
    ApiTransport,
    ClaudePermissionMode,
    Role,
    SystemPrompt,
    Save,
    Cancel,
}

impl SubAgentEditorField {
    pub const ALL: [SubAgentEditorField; 16] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::BaseUrl,
        SubAgentEditorField::ContextWindowTokens,
        SubAgentEditorField::OpenRouterProviderOrder,
        SubAgentEditorField::OpenRouterProviderIgnore,
        SubAgentEditorField::OpenRouterAllowFallbacks,
        SubAgentEditorField::HuggingFaceProvider,
        SubAgentEditorField::ReasoningEffort,
        SubAgentEditorField::ApiTransport,
        SubAgentEditorField::ClaudePermissionMode,
        SubAgentEditorField::Role,
        SubAgentEditorField::SystemPrompt,
        SubAgentEditorField::Save,
        SubAgentEditorField::Cancel,
    ];
    const OPENROUTER: [SubAgentEditorField; 15] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::BaseUrl,
        SubAgentEditorField::ContextWindowTokens,
        SubAgentEditorField::OpenRouterProviderOrder,
        SubAgentEditorField::OpenRouterProviderIgnore,
        SubAgentEditorField::OpenRouterAllowFallbacks,
        SubAgentEditorField::ReasoningEffort,
        SubAgentEditorField::ApiTransport,
        SubAgentEditorField::ClaudePermissionMode,
        SubAgentEditorField::Role,
        SubAgentEditorField::SystemPrompt,
        SubAgentEditorField::Save,
        SubAgentEditorField::Cancel,
    ];
    const HUGGINGFACE: [SubAgentEditorField; 13] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::BaseUrl,
        SubAgentEditorField::ContextWindowTokens,
        SubAgentEditorField::HuggingFaceProvider,
        SubAgentEditorField::ReasoningEffort,
        SubAgentEditorField::ApiTransport,
        SubAgentEditorField::ClaudePermissionMode,
        SubAgentEditorField::Role,
        SubAgentEditorField::SystemPrompt,
        SubAgentEditorField::Save,
        SubAgentEditorField::Cancel,
    ];
    const NON_OPENROUTER: [SubAgentEditorField; 12] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::BaseUrl,
        SubAgentEditorField::ContextWindowTokens,
        SubAgentEditorField::ReasoningEffort,
        SubAgentEditorField::ApiTransport,
        SubAgentEditorField::ClaudePermissionMode,
        SubAgentEditorField::Role,
        SubAgentEditorField::SystemPrompt,
        SubAgentEditorField::Save,
        SubAgentEditorField::Cancel,
    ];

    fn visible_for_provider(provider: &str) -> &'static [SubAgentEditorField] {
        if provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
            &Self::OPENROUTER
        } else if provider == zorai_shared::providers::PROVIDER_ID_HUGGINGFACE {
            &Self::HUGGINGFACE
        } else {
            &Self::NON_OPENROUTER
        }
    }

    pub fn prev(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[if index == 0 {
            Self::ALL.len().saturating_sub(1)
        } else {
            index - 1
        }]
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn prev_for_provider(self, provider: &str) -> Self {
        let visible = Self::visible_for_provider(provider);
        let index = visible.iter().position(|field| *field == self).unwrap_or(0);
        visible[if index == 0 {
            visible.len().saturating_sub(1)
        } else {
            index - 1
        }]
    }

    pub fn next_for_provider(self, provider: &str) -> Self {
        let visible = Self::visible_for_provider(provider);
        let index = visible.iter().position(|field| *field == self).unwrap_or(0);
        visible[(index + 1) % visible.len()]
    }
}

pub fn openrouter_provider_list_from_json(raw: &serde_json::Value, key: &str) -> String {
    raw.get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

pub fn openrouter_provider_list_to_json(value: &str) -> serde_json::Value {
    serde_json::Value::Array(
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| serde_json::Value::String(value.to_string()))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy)]
pub struct SubAgentRolePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub system_prompt: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolePickerChoiceKind {
    Preset,
    Persona,
}

#[derive(Debug, Clone, Copy)]
pub struct RolePickerChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: RolePickerChoiceKind,
}

pub const SUBAGENT_ROLE_PRESETS: &[SubAgentRolePreset] = &[
    SubAgentRolePreset {
        id: "code_review",
        label: "Code Review",
        system_prompt: "You are a code review specialist. Focus on correctness, regressions, security, edge cases, missing tests, and actionable fixes. Be concise and precise.",
    },
    SubAgentRolePreset {
        id: "research",
        label: "Research",
        system_prompt: "You are a research specialist. Gather relevant code and runtime context, compare options, identify constraints, and return clear conclusions with supporting evidence.",
    },
    SubAgentRolePreset {
        id: "executor",
        label: "Executor / Performer",
        system_prompt: "You are an execution specialist. Carry assigned work through to completion, make concrete progress, coordinate dependencies, and report blockers with exact next actions.",
    },
    SubAgentRolePreset {
        id: "testing",
        label: "Testing",
        system_prompt: "You are a testing specialist. Design focused verification, find reproducible failure cases, validate fixes, and call out remaining risks or missing coverage.",
    },
    SubAgentRolePreset {
        id: "planning",
        label: "Planning",
        system_prompt: "You are a planning specialist. Break work into durable, ordered steps with clear dependencies, acceptance criteria, and realistic implementation boundaries.",
    },
    SubAgentRolePreset {
        id: "documentation",
        label: "Documentation",
        system_prompt: "You are a documentation specialist. Produce clear developer-facing docs, explain behavior accurately, and keep examples aligned with the current implementation.",
    },
    SubAgentRolePreset {
        id: "refactoring",
        label: "Refactoring",
        system_prompt: "You are a refactoring specialist. Improve structure and maintainability without changing behavior, preserve intent, and keep edits scoped and defensible.",
    },
    SubAgentRolePreset {
        id: "implementation",
        label: "Implementation",
        system_prompt: "You are an implementation specialist. Build requested code changes end to end, follow local patterns, keep edits scoped, and verify behavior with focused tests or checks.",
    },
    SubAgentRolePreset {
        id: "debugging",
        label: "Debugging",
        system_prompt: "You are a debugging specialist. Reproduce failures, isolate root causes, distinguish symptoms from causes, and propose or apply fixes backed by evidence.",
    },
    SubAgentRolePreset {
        id: "architecture",
        label: "Architecture",
        system_prompt: "You are an architecture specialist. Evaluate system boundaries, data flow, contracts, dependencies, and trade-offs, then recommend designs that fit the existing codebase.",
    },
    SubAgentRolePreset {
        id: "security",
        label: "Security",
        system_prompt: "You are a security specialist. Identify realistic abuse paths, sensitive boundaries, unsafe defaults, and mitigation options while keeping recommendations practical and scoped.",
    },
    SubAgentRolePreset {
        id: "data_analysis",
        label: "Data Analysis",
        system_prompt: "You are a data analysis specialist. Inspect datasets, logs, metrics, or structured outputs, validate assumptions, summarize patterns, and call out uncertainty clearly.",
    },
    SubAgentRolePreset {
        id: "medical_research",
        label: "Medical Research",
        system_prompt: "You are a medical research specialist. Summarize evidence, compare guidelines, explain uncertainty clearly, and keep outputs informational rather than diagnostic or prescriptive.",
    },
    SubAgentRolePreset {
        id: "financial_analysis",
        label: "Financial Analysis",
        system_prompt: "You are a financial analysis specialist. Make assumptions explicit, compare scenarios, surface downside risk, and provide informational analysis without claiming fiduciary authority or guaranteed outcomes.",
    },
    SubAgentRolePreset {
        id: "legal_research",
        label: "Legal Research",
        system_prompt: "You are a legal research specialist. Summarize authorities, compare interpretations, spot issues, and state uncertainty clearly without presenting the result as legal advice.",
    },
    SubAgentRolePreset {
        id: "art_direction",
        label: "Art Direction",
        system_prompt: "You are an art direction specialist. Produce multiple credible creative directions, explain visual trade-offs, and keep concepts coherent across style, brand, and presentation.",
    },
    SubAgentRolePreset {
        id: "scientific_review",
        label: "Scientific Review",
        system_prompt: "You are a scientific review specialist. Evaluate methodology, evidence quality, and statistical reasoning, then separate robust findings from speculation.",
    },
    SubAgentRolePreset {
        id: "product_marketing",
        label: "Product Marketing",
        system_prompt: "You are a product marketing specialist. Clarify audience, positioning, messaging, and launch trade-offs, then tie recommendations to market context and measurable outcomes.",
    },
    SubAgentRolePreset {
        id: "writing",
        label: "Writing",
        system_prompt: "You are a writing specialist. Produce clear audience-appropriate prose, preserve factual accuracy, structure information for scanning, and keep tone aligned with the task.",
    },
    SubAgentRolePreset {
        id: "coordination",
        label: "Coordination",
        system_prompt: "You are a coordination specialist. Track owners, dependencies, decisions, risks, and next actions so multi-person or multi-agent work stays unblocked and explicit.",
    },
    SubAgentRolePreset {
        id: "product_strategy",
        label: "Product Strategy",
        system_prompt: "You are a product strategy specialist. Clarify user needs, compare options, define acceptance criteria, prioritize scope, and connect recommendations to outcomes.",
    },
    SubAgentRolePreset {
        id: "operations",
        label: "Operations",
        system_prompt: "You are an operations specialist. Handle process, rollout, support, incident, and administrative work with concrete checklists, owners, timelines, and status updates.",
    },
];

pub const BUILTIN_PERSONA_ROLE_CHOICES: &[RolePickerChoice] = &[
    RolePickerChoice {
        id: zorai_protocol::AGENT_ID_SWAROG,
        label: "Svarog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: zorai_protocol::AGENT_ID_RAROG,
        label: "Rarog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "weles",
        label: "Weles",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "swarozyc",
        label: "Swarozyc",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "radogost",
        label: "Radogost",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "domowoj",
        label: "Domowoj",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "swietowit",
        label: "Swietowit",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "perun",
        label: "Perun",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "mokosh",
        label: "Mokosh",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "dazhbog",
        label: "Dazhbog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "rod",
        label: "Rod",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "kikimora",
        label: "Kikimora",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "bannik",
        label: "Bannik",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "ovinnik",
        label: "Ovinnik",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "dziad",
        label: "Dziad",
        kind: RolePickerChoiceKind::Persona,
    },
];

pub fn role_picker_custom_index() -> usize {
    SUBAGENT_ROLE_PRESETS.len() + BUILTIN_PERSONA_ROLE_CHOICES.len()
}

pub fn role_picker_item_count() -> usize {
    role_picker_custom_index() + 1
}

pub fn role_picker_choice(index: usize) -> Option<RolePickerChoice> {
    if let Some(preset) = SUBAGENT_ROLE_PRESETS.get(index) {
        return Some(RolePickerChoice {
            id: preset.id,
            label: preset.label,
            kind: RolePickerChoiceKind::Preset,
        });
    }
    BUILTIN_PERSONA_ROLE_CHOICES
        .get(index.saturating_sub(SUBAGENT_ROLE_PRESETS.len()))
        .copied()
}

pub fn role_picker_index_for_id(id: &str) -> Option<usize> {
    let normalized = normalize_role_preset_id(id);
    SUBAGENT_ROLE_PRESETS
        .iter()
        .position(|preset| preset.id.eq_ignore_ascii_case(normalized.as_str()))
        .or_else(|| {
            BUILTIN_PERSONA_ROLE_CHOICES
                .iter()
                .position(|choice| choice.id.eq_ignore_ascii_case(normalized.as_str()))
                .map(|index| SUBAGENT_ROLE_PRESETS.len() + index)
        })
}

#[derive(Debug, Clone)]
pub struct SubAgentEditorState {
    pub id: Option<String>,
    pub created_at: u64,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub context_window_tokens: Option<u32>,
    pub role: String,
    pub system_prompt: String,
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
    pub field: SubAgentEditorField,
    pub previous_role_preset: Option<String>,
}

impl SubAgentEditorState {
    pub fn new(id: Option<String>, created_at: u64, provider: String, model: String) -> Self {
        Self {
            id,
            created_at,
            name: String::new(),
            provider,
            model,
            base_url: String::new(),
            context_window_tokens: None,
            role: String::new(),
            system_prompt: String::new(),
            enabled: true,
            builtin: false,
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
            raw_json: None,
            field: SubAgentEditorField::Name,
            previous_role_preset: None,
        }
    }

    pub fn identity_is_mutable(&self) -> bool {
        self.id.is_none() || !self.immutable_identity
    }

    pub fn role_preset_index(&self) -> Option<usize> {
        SUBAGENT_ROLE_PRESETS
            .iter()
            .position(|preset| preset.id == self.role)
    }

    pub fn apply_role_preset_by_index(&mut self, index: usize) {
        if let Some(preset) = SUBAGENT_ROLE_PRESETS.get(index) {
            let previous_prompt = self
                .previous_role_preset
                .as_deref()
                .and_then(find_role_preset)
                .map(|preset| preset.system_prompt)
                .unwrap_or("");
            if self.system_prompt.trim().is_empty() || self.system_prompt == previous_prompt {
                self.system_prompt = preset.system_prompt.to_string();
            }
            self.role = preset.id.to_string();
            self.previous_role_preset = Some(preset.id.to_string());
        }
    }
}

pub fn find_role_preset(id: &str) -> Option<&'static SubAgentRolePreset> {
    let normalized = normalize_role_preset_id(id);
    SUBAGENT_ROLE_PRESETS
        .iter()
        .find(|preset| preset.id == normalized)
}

fn normalize_role_preset_id(id: &str) -> String {
    match id.trim().to_ascii_lowercase().as_str() {
        "performer" => "executor".to_string(),
        normalized => normalized.to_string(),
    }
}
