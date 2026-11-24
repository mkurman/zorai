use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ConfigReconcileState {
    Applied,
    Reconciling,
    Degraded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigRuntimeProjection {
    pub state: ConfigReconcileState,
    pub desired_revision: u64,
    pub effective_revision: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigEffectiveRuntimeState {
    pub reconcile: ConfigRuntimeProjection,
    pub gateway_runtime_connected: bool,
}

impl Default for ConfigRuntimeProjection {
    fn default() -> Self {
        Self {
            state: ConfigReconcileState::Applied,
            desired_revision: 0,
            effective_revision: 0,
            last_error: None,
        }
    }
}

pub(super) const WELES_BUILTIN_ID: &str = super::agent_identity::WELES_BUILTIN_SUBAGENT_ID;
pub(super) const WELES_BUILTIN_NAME: &str = "WELES";
pub(super) const WELES_PROTECTED_REASON: &str = "Daemon-owned WELES registry entry";
pub(super) const DEFAULT_WELES_TOOL_BLACKLIST: &[&str] = &[
    zorai_protocol::tool_names::LIST_TERMINALS,
    zorai_protocol::tool_names::READ_ACTIVE_TERMINAL_CONTENT,
    zorai_protocol::tool_names::RUN_TERMINAL_COMMAND,
    zorai_protocol::tool_names::ALLOCATE_TERMINAL,
    zorai_protocol::tool_names::TYPE_IN_TERMINAL,
];

pub(super) fn is_reserved_builtin_sub_agent_id(id: &str) -> bool {
    id.eq_ignore_ascii_case(WELES_BUILTIN_ID)
}

pub(super) fn is_reserved_builtin_sub_agent_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(WELES_BUILTIN_NAME)
}

pub(super) fn protected_mutation_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::anyhow!("protected mutation: {}", message.into())
}

pub(super) fn builtin_collision_error(field: &str, value: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "reserved built-in sub-agent {} collision: '{}' is reserved for WELES",
        field,
        value
    )
}

pub(super) fn now_millis_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(super) fn resolve_weles_reasoning_effort(overrides: &WelesBuiltinOverrides) -> String {
    overrides
        .reasoning_effort
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "medium".to_string())
}

pub(crate) fn resolve_weles_max_concurrent_reviews(overrides: &WelesBuiltinOverrides) -> usize {
    overrides.max_concurrent_reviews.unwrap_or(2).clamp(1, 16) as usize
}

pub(super) fn default_weles_tool_blacklist() -> Vec<String> {
    DEFAULT_WELES_TOOL_BLACKLIST
        .iter()
        .map(|tool| (*tool).to_string())
        .collect()
}

pub(super) fn resolve_main_agent_default(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|candidate| !candidate.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub(super) fn sanitize_weles_operator_system_prompt(
    system_prompt: Option<String>,
    inherited_system_prompt: &str,
) -> Option<String> {
    let sanitized = system_prompt
        .map(|value| crate::agent::weles_governance::strip_weles_internal_payload_markers(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    sanitized.filter(|value| value != inherited_system_prompt)
}

pub(super) fn sanitize_weles_builtin_overrides_struct(
    overrides: &mut WelesBuiltinOverrides,
    inherited_system_prompt: &str,
) {
    if overrides
        .role
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "governance")
    {
        overrides.role = None;
    }
    overrides.system_prompt = sanitize_weles_operator_system_prompt(
        overrides.system_prompt.take(),
        inherited_system_prompt,
    );
}

pub(in crate::agent) fn build_effective_weles_definition(
    config: &AgentConfig,
) -> SubAgentDefinition {
    let mut overrides = config.builtin_sub_agents.weles.clone();
    sanitize_weles_builtin_overrides_struct(&mut overrides, &config.system_prompt);
    let reasoning_effort = resolve_weles_reasoning_effort(&overrides);
    let system_prompt =
        resolve_main_agent_default(overrides.system_prompt.take(), &config.system_prompt);
    SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        id: WELES_BUILTIN_ID.to_string(),
        name: WELES_BUILTIN_NAME.to_string(),
        provider: resolve_main_agent_default(overrides.provider.clone(), &config.provider),
        model: resolve_main_agent_default(overrides.model.clone(), &config.model),
        role: overrides
            .role
            .clone()
            .or_else(|| Some("governance".to_string())),
        system_prompt: Some(system_prompt),
        tool_whitelist: overrides.tool_whitelist.clone(),
        tool_blacklist: overrides
            .tool_blacklist
            .clone()
            .or_else(|| Some(default_weles_tool_blacklist())),
        context_budget_tokens: overrides.context_budget_tokens,
        context_window_tokens: overrides.context_window_tokens,
        max_duration_secs: overrides.max_duration_secs,
        supervisor_config: overrides.supervisor_config.clone(),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some(WELES_PROTECTED_REASON.to_string()),
        reasoning_effort: Some(reasoning_effort),
        api_transport: overrides.api_transport,
        claude_permission_mode: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: overrides.huggingface_provider.clone(),
        created_at: 0,
    }
}

pub(super) fn is_weles_builtin_update_shape(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
        && def.name == WELES_BUILTIN_NAME
        && def.builtin
        && def.immutable_identity
        && !def.disable_allowed
        && !def.delete_allowed
        && def.protected_reason.as_deref() == Some(WELES_PROTECTED_REASON)
}

pub(super) fn is_weles_builtin_target(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
}

pub(super) fn is_attempted_weles_builtin_edit(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
        && (def.builtin
            || def.immutable_identity
            || !def.disable_allowed
            || !def.delete_allowed
            || def.protected_reason.is_some())
}

pub(crate) fn canonicalize_weles_client_update(def: &mut SubAgentDefinition) {
    if def.id != WELES_BUILTIN_ID {
        return;
    }
    if def.name.trim().is_empty() {
        def.name = WELES_BUILTIN_NAME.to_string();
    }
    if def.name == WELES_BUILTIN_NAME {
        if def.protected_reason.is_none() {
            def.protected_reason = Some(WELES_PROTECTED_REASON.to_string());
        }
        if !def.builtin && !def.immutable_identity && def.disable_allowed && def.delete_allowed {
            def.builtin = true;
            def.immutable_identity = true;
            def.disable_allowed = false;
            def.delete_allowed = false;
        }
    }
}

pub(super) fn gateway_runtime_is_desired_at_startup(config: &AgentConfig) -> bool {
    config.gateway.enabled
        && (!config.gateway.slack_token.trim().is_empty()
            || !config.gateway.telegram_token.trim().is_empty()
            || !config.gateway.discord_token.trim().is_empty()
            || !config.gateway.whatsapp_token.trim().is_empty()
            || config.gateway.whatsapp_link_fallback_electron)
}

pub(crate) fn derive_startup_config_runtime_projection(
    config: &AgentConfig,
) -> ConfigRuntimeProjection {
    if gateway_runtime_is_desired_at_startup(config) {
        ConfigRuntimeProjection {
            state: ConfigReconcileState::Degraded,
            desired_revision: 1,
            effective_revision: 0,
            last_error: Some(
                "startup: gateway runtime is not connected; effective state must be re-derived"
                    .to_string(),
            ),
        }
    } else {
        ConfigRuntimeProjection::default()
    }
}

#[cfg(test)]
pub(super) fn config_reconcile_delay_map(
) -> &'static std::sync::Mutex<std::collections::HashMap<usize, std::time::Duration>> {
    static MAP: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<usize, std::time::Duration>>,
    > = std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
pub(super) fn config_reconcile_failure_map(
) -> &'static std::sync::Mutex<std::collections::HashMap<usize, String>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}
