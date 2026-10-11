#![allow(dead_code)]

//! Shared agent identity helpers for main, concierge, and spawned agents.

use anyhow::anyhow;
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};

use zorai_protocol::{
    AGENT_HANDLE_SVAROG, AGENT_ID_RAROG, AGENT_ID_SWAROG, AGENT_NAME_RAROG, AGENT_NAME_SWAROG,
};

use super::types::{AgentConfig, AgentTask, BuiltinPersonaOverrides, SubAgentDefinition};

pub(crate) const MAIN_AGENT_ID: &str = AGENT_ID_SWAROG;
pub(crate) const MAIN_AGENT_NAME: &str = AGENT_NAME_SWAROG;
pub(crate) const CONCIERGE_AGENT_ID: &str = AGENT_ID_RAROG;
pub(crate) const CONCIERGE_AGENT_NAME: &str = AGENT_NAME_RAROG;
pub(crate) const INTERNAL_DM_THREAD_PREFIX: &str = "dm:";
pub(in crate::agent) const PARTICIPANT_PLAYGROUND_THREAD_PREFIX: &str = "playground:";
pub(crate) const PERSONA_MARKER: &str = "Agent persona:";
pub(crate) const PERSONA_ID_MARKER: &str = "Agent persona id:";
pub(crate) const MAIN_AGENT_PUBLIC_ALIAS: &str = AGENT_HANDLE_SVAROG;
pub(crate) const MAIN_AGENT_ALIAS: &str = "main";
pub(crate) const MAIN_AGENT_LEGACY_ALIAS: &str = "main-agent";
pub(crate) const MAIN_AGENT_FALLBACK_ALIAS: &str = "assistant";
pub(crate) const CONCIERGE_AGENT_ALIAS: &str = "concierge";
pub(crate) const CONCIERGE_AGENT_LEGACY_ALIAS: &str = "concierge-agent";

pub(crate) const SWAROZYC_AGENT_ID: &str = "swarozyc";
pub(crate) const SWAROZYC_AGENT_NAME: &str = "Swarozyc";
pub(crate) const RADOGOST_AGENT_ID: &str = "radogost";
pub(crate) const RADOGOST_AGENT_NAME: &str = "Radogost";
pub(crate) const DOMOWOJ_AGENT_ID: &str = "domowoj";
pub(crate) const DOMOWOJ_AGENT_NAME: &str = "Domowoj";
pub(crate) const SWIETOWIT_AGENT_ID: &str = "swietowit";
pub(crate) const SWIETOWIT_AGENT_NAME: &str = "Swietowit";
pub(crate) const PERUN_AGENT_ID: &str = "perun";
pub(crate) const PERUN_AGENT_NAME: &str = "Perun";
pub(crate) const MOKOSH_AGENT_ID: &str = "mokosh";
pub(crate) const MOKOSH_AGENT_NAME: &str = "Mokosh";
pub(crate) const DAZHBOG_AGENT_ID: &str = "dazhbog";
pub(crate) const DAZHBOG_AGENT_NAME: &str = "Dazhbog";
pub(crate) const ROD_AGENT_ID: &str = "rod";
pub(crate) const ROD_AGENT_NAME: &str = "Rod";
pub(crate) const WELES_AGENT_ID: &str = "weles";
pub(crate) const WELES_AGENT_NAME: &str = "Weles";
pub(crate) const WELES_BUILTIN_SUBAGENT_ID: &str = "weles_builtin";
pub(crate) const WELES_GOVERNANCE_SCOPE: &str = "governance";
pub(crate) const WELES_VITALITY_SCOPE: &str = "vitality";

struct PersonaSeed {
    id: &'static str,
    name: &'static str,
    guidance: &'static str,
}

impl PersonaSeed {
    pub(crate) fn id(&self) -> &'static str {
        self.id
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedAgentTarget {
    pub(super) scope_id: String,
    pub(super) agent_name: String,
    pub(super) matched_sub_agent: Option<SubAgentDefinition>,
}

const SPAWNED_PERSONAS: [PersonaSeed; 8] = [
    PersonaSeed {
        id: SWAROZYC_AGENT_ID,
        name: SWAROZYC_AGENT_NAME,
        guidance: "You inherit the main agent's craft but stay narrower, quicker, and more execution-focused than the main agent.",
    },
    PersonaSeed {
        id: RADOGOST_AGENT_ID,
        name: RADOGOST_AGENT_NAME,
        guidance: "You specialize in negotiation between options, comparing tradeoffs, and surfacing the strongest route forward.",
    },
    PersonaSeed {
        id: DOMOWOJ_AGENT_ID,
        name: DOMOWOJ_AGENT_NAME,
        guidance: "You are a careful keeper of the working environment. Favor stability, cleanup, and precise local fixes.",
    },
    PersonaSeed {
        id: SWIETOWIT_AGENT_ID,
        name: SWIETOWIT_AGENT_NAME,
        guidance: "You maintain broader situational awareness than most subagents and should keep the surrounding architecture in view.",
    },
    PersonaSeed {
        id: PERUN_AGENT_ID,
        name: PERUN_AGENT_NAME,
        guidance: "You are decisive, forceful, and comfortable making hard calls under uncertainty. Favor robust action, security, and infrastructure discipline.",
    },
    PersonaSeed {
        id: MOKOSH_AGENT_ID,
        name: MOKOSH_AGENT_NAME,
        guidance: "You are steady, careful, and maintenance-minded. Favor reliability, operational continuity, and durable local care for the working environment.",
    },
    PersonaSeed {
        id: DAZHBOG_AGENT_ID,
        name: DAZHBOG_AGENT_NAME,
        guidance: "You bring illumination and clarity. Favor turning complexity into a crisp plan, a clear explanation, or an immediately useful next step.",
    },
    PersonaSeed {
        id: ROD_AGENT_ID,
        name: ROD_AGENT_NAME,
        guidance: "You are continuity-minded. Prefer solutions that preserve durable structure, conventions, and long-term coherence.",
    },
];

pub(crate) const LEAD_PERSONA_SPAWN_ERROR: &str = "Svarog, Rarog, and Weles are lead personas and cannot be spawned. Reuse their provider and model on another persona.";

pub(crate) fn spawned_persona_count() -> usize {
    SPAWNED_PERSONAS.len()
}

/// Lead personas keep an extended policy. Spawned workers may use the same
/// provider and model, but they must not become these personas.
pub(crate) fn is_lead_only_persona(alias: &str) -> bool {
    let mut normalized = alias.trim().to_ascii_lowercase();
    if let Some(stripped) = normalized.strip_suffix("_builtin") {
        normalized = stripped.to_string();
    }
    matches!(
        normalized.as_str(),
        MAIN_AGENT_ID | MAIN_AGENT_PUBLIC_ALIAS | CONCIERGE_AGENT_ID | WELES_AGENT_ID | "veles"
    )
}

tokio::task_local! {
    static ACTIVE_AGENT_SCOPE_ID: String;
}

fn spawned_persona(seed: &str) -> &'static PersonaSeed {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let idx = (hasher.finish() as usize) % SPAWNED_PERSONAS.len();
    &SPAWNED_PERSONAS[idx]
}

pub(crate) fn spawned_persona_id_for_seed(seed: &str) -> &'static str {
    spawned_persona(seed).id
}

/// Pick a spawned persona for `seed` that is not already used by an active
/// sibling task under the same parent scope.
/// Two concurrent subagents hashing to the same persona name made
/// `message_agent` route sibling DMs into one shared context-free
/// `internal-dm:<persona-a>:<persona-b>` thread, so persona names must stay
/// unique within a parent scope.
pub(crate) fn pick_unique_spawned_persona_seed<'a>(
    seed: &str,
    taken_persona_ids: impl IntoIterator<Item = &'a str>,
) -> Option<&'static str> {
    let taken: std::collections::HashSet<String> = taken_persona_ids
        .into_iter()
        .map(|id| id.trim().to_ascii_lowercase())
        .collect();
    let primary = spawned_persona(seed);
    if !taken.contains(primary.id) {
        return Some(primary.id);
    }
    // Deterministic fallback: stable rotation from the primary slot so the
    // same seed always maps to the same alternate persona.
    let primary_idx = SPAWNED_PERSONAS
        .iter()
        .position(|persona| persona.id == primary.id)
        .unwrap_or(0);
    (1..=SPAWNED_PERSONAS.len())
        .map(|offset| SPAWNED_PERSONAS[(primary_idx + offset) % SPAWNED_PERSONAS.len()].id)
        .find(|persona_id| !taken.contains(&persona_id.to_ascii_lowercase()))
}

fn persona_by_alias(alias: &str) -> Option<&'static PersonaSeed> {
    let normalized = alias.trim().to_ascii_lowercase();
    SPAWNED_PERSONAS
        .iter()
        .find(|persona| normalized == persona.id || normalized == persona.name.to_ascii_lowercase())
}

fn sub_agent_by_alias<'a>(
    alias: &str,
    sub_agents: &'a [SubAgentDefinition],
) -> Option<&'a SubAgentDefinition> {
    sub_agents.iter().find(|def| {
        def.id.eq_ignore_ascii_case(alias)
            || def.name.eq_ignore_ascii_case(alias)
            || def
                .id
                .strip_suffix("_builtin")
                .is_some_and(|value| value.eq_ignore_ascii_case(alias))
    })
}

pub(crate) fn resolve_agent_target(
    alias: &str,
    sub_agents: &[SubAgentDefinition],
) -> ResolvedAgentTarget {
    let normalized = alias.trim().to_ascii_lowercase();
    match normalized.as_str() {
        MAIN_AGENT_ID
        | MAIN_AGENT_PUBLIC_ALIAS
        | MAIN_AGENT_ALIAS
        | MAIN_AGENT_LEGACY_ALIAS
        | MAIN_AGENT_FALLBACK_ALIAS => ResolvedAgentTarget {
            scope_id: MAIN_AGENT_ID.to_string(),
            agent_name: MAIN_AGENT_NAME.to_string(),
            matched_sub_agent: None,
        },
        CONCIERGE_AGENT_ID | CONCIERGE_AGENT_ALIAS | CONCIERGE_AGENT_LEGACY_ALIAS => {
            ResolvedAgentTarget {
                scope_id: CONCIERGE_AGENT_ID.to_string(),
                agent_name: CONCIERGE_AGENT_NAME.to_string(),
                matched_sub_agent: None,
            }
        }
        "veles" | WELES_AGENT_ID => ResolvedAgentTarget {
            scope_id: WELES_AGENT_ID.to_string(),
            agent_name: WELES_AGENT_NAME.to_string(),
            matched_sub_agent: sub_agent_by_alias(WELES_BUILTIN_SUBAGENT_ID, sub_agents).cloned(),
        },
        _ => {
            if let Some(agent_id) = agent_id_for_alias(alias) {
                return resolve_agent_target(agent_id, sub_agents);
            }
            if let Some(persona) = persona_by_alias(alias) {
                return ResolvedAgentTarget {
                    scope_id: persona.id.to_string(),
                    agent_name: persona.name.to_string(),
                    matched_sub_agent: None,
                };
            }
            if let Some(def) = sub_agent_by_alias(alias, sub_agents) {
                let scope_id = def
                    .id
                    .strip_suffix("_builtin")
                    .unwrap_or(&def.id)
                    .to_string();
                let agent_name = if scope_id == WELES_AGENT_ID {
                    WELES_AGENT_NAME.to_string()
                } else {
                    def.name.clone()
                };
                return ResolvedAgentTarget {
                    scope_id,
                    agent_name,
                    matched_sub_agent: Some(def.clone()),
                };
            }
            ResolvedAgentTarget {
                scope_id: MAIN_AGENT_ID.to_string(),
                agent_name: MAIN_AGENT_NAME.to_string(),
                matched_sub_agent: None,
            }
        }
    }
}

pub(crate) const ALIASABLE_AGENT_IDS: [&str; 3] = [MAIN_AGENT_ID, CONCIERGE_AGENT_ID, WELES_AGENT_ID];

static AGENT_ALIASES: std::sync::RwLock<std::collections::BTreeMap<String, String>> =
    std::sync::RwLock::new(std::collections::BTreeMap::new());

pub(crate) fn set_agent_aliases(aliases: &std::collections::BTreeMap<String, String>) {
    if let Ok(mut current) = AGENT_ALIASES.write() {
        *current = aliases.clone();
    }
}

pub(crate) fn agent_aliases() -> std::collections::BTreeMap<String, String> {
    AGENT_ALIASES
        .read()
        .map(|aliases| aliases.clone())
        .unwrap_or_default()
}

pub(crate) fn agent_id_for_alias_in(
    name: &str,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Option<&'static str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    ALIASABLE_AGENT_IDS.into_iter().find(|id| {
        aliases
            .get(*id)
            .is_some_and(|alias| alias.trim().eq_ignore_ascii_case(trimmed))
    })
}

pub(crate) fn agent_id_for_alias(name: &str) -> Option<&'static str> {
    agent_id_for_alias_in(name, &agent_aliases())
}

pub(crate) fn agent_display_name_in(
    agent_id: &str,
    aliases: &std::collections::BTreeMap<String, String>,
) -> String {
    let canonical = canonical_agent_id(agent_id);
    aliases
        .get(canonical)
        .map(|alias| alias.trim())
        .filter(|alias| !alias.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| canonical_agent_name(canonical).to_string())
}

pub(crate) fn agent_display_name(agent_id: &str) -> String {
    agent_display_name_in(agent_id, &agent_aliases())
}

pub(crate) fn alias_for_agent_name_in(
    agent_name: &str,
    aliases: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let agent_id = builtin_agent_id(&agent_name.trim().to_ascii_lowercase())?;
    if !ALIASABLE_AGENT_IDS.contains(&agent_id) {
        return None;
    }
    aliases
        .get(agent_id)
        .map(|alias| alias.trim().to_string())
        .filter(|alias| !alias.is_empty())
}

pub(crate) fn agent_alias_prompt_note_in(
    aliases: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let lines = ALIASABLE_AGENT_IDS
        .into_iter()
        .filter_map(|id| {
            let alias = aliases.get(id)?.trim();
            (!alias.is_empty()).then(|| {
                format!(
                    "- {alias} is the operator's name for {} (agent id `{id}`).",
                    canonical_agent_name(id)
                )
            })
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "## Agent Aliases\nThe operator renamed some built-in agents. Use the alias when talking to the operator; the original name and id still work when addressing agents.\n{}",
        lines.join("\n")
    ))
}

pub(crate) fn agent_alias_prompt_note() -> Option<String> {
    agent_alias_prompt_note_in(&agent_aliases())
}

pub(crate) fn validate_agent_aliases(
    aliases: &std::collections::BTreeMap<String, String>,
    sub_agents: &[SubAgentDefinition],
) -> anyhow::Result<()> {
    let mut seen = std::collections::HashSet::new();
    for (id, alias) in aliases {
        if !ALIASABLE_AGENT_IDS.contains(&id.as_str()) {
            return Err(anyhow!(
                "agent alias key `{id}` is not one of: {}",
                ALIASABLE_AGENT_IDS.join(", ")
            ));
        }
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > 40 {
            return Err(anyhow!("agent alias `{trimmed}` is longer than 40 characters"));
        }
        let normalized = trimmed.to_ascii_lowercase();
        if !seen.insert(normalized.clone()) {
            return Err(anyhow!("agent alias `{trimmed}` is used for more than one agent"));
        }
        if let Some(builtin) = builtin_agent_id(&normalized).filter(|builtin| *builtin != id) {
            return Err(anyhow!(
                "agent alias `{trimmed}` already names the built-in agent {}",
                canonical_agent_name(builtin)
            ));
        }
        if sub_agent_by_alias(trimmed, sub_agents)
            .is_some_and(|def| def.id.strip_suffix("_builtin").unwrap_or(&def.id) != id.as_str())
        {
            return Err(anyhow!("agent alias `{trimmed}` already names a subagent"));
        }
    }
    Ok(())
}

fn builtin_agent_id(normalized: &str) -> Option<&'static str> {
    match normalized {
        MAIN_AGENT_ID
        | MAIN_AGENT_PUBLIC_ALIAS
        | MAIN_AGENT_ALIAS
        | MAIN_AGENT_LEGACY_ALIAS
        | MAIN_AGENT_FALLBACK_ALIAS => Some(MAIN_AGENT_ID),
        CONCIERGE_AGENT_ID | CONCIERGE_AGENT_ALIAS | CONCIERGE_AGENT_LEGACY_ALIAS => {
            Some(CONCIERGE_AGENT_ID)
        }
        "veles" | WELES_AGENT_ID => Some(WELES_AGENT_ID),
        _ => persona_by_alias(normalized).map(|persona| persona.id),
    }
}

pub(crate) fn canonical_agent_id(alias: &str) -> &'static str {
    let normalized = alias.trim().to_ascii_lowercase();
    builtin_agent_id(&normalized)
        .or_else(|| agent_id_for_alias(&normalized))
        .unwrap_or(MAIN_AGENT_ID)
}

pub(crate) fn canonical_agent_name(alias: &str) -> &'static str {
    match canonical_agent_id(alias) {
        CONCIERGE_AGENT_ID => CONCIERGE_AGENT_NAME,
        SWAROZYC_AGENT_ID => SWAROZYC_AGENT_NAME,
        RADOGOST_AGENT_ID => RADOGOST_AGENT_NAME,
        DOMOWOJ_AGENT_ID => DOMOWOJ_AGENT_NAME,
        SWIETOWIT_AGENT_ID => SWIETOWIT_AGENT_NAME,
        PERUN_AGENT_ID => PERUN_AGENT_NAME,
        MOKOSH_AGENT_ID => MOKOSH_AGENT_NAME,
        DAZHBOG_AGENT_ID => DAZHBOG_AGENT_NAME,
        ROD_AGENT_ID => ROD_AGENT_NAME,
        WELES_AGENT_ID => WELES_AGENT_NAME,
        _ => MAIN_AGENT_NAME,
    }
}

pub(crate) fn canonical_agent_guidance(alias: &str) -> Option<&'static str> {
    let normalized = alias.trim().to_ascii_lowercase();
    match normalized.as_str() {
        MAIN_AGENT_ID
        | MAIN_AGENT_PUBLIC_ALIAS
        | MAIN_AGENT_ALIAS
        | MAIN_AGENT_LEGACY_ALIAS
        | MAIN_AGENT_FALLBACK_ALIAS => None,
        CONCIERGE_AGENT_ID | CONCIERGE_AGENT_ALIAS | CONCIERGE_AGENT_LEGACY_ALIAS => None,
        _ => persona_by_alias(&normalized).map(|persona| persona.guidance),
    }
}

pub(crate) fn is_concierge_target(alias: &str) -> bool {
    canonical_agent_id(alias) == CONCIERGE_AGENT_ID
}

pub(crate) fn is_main_agent_scope(alias: &str) -> bool {
    let normalized = alias.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        MAIN_AGENT_ID
            | MAIN_AGENT_PUBLIC_ALIAS
            | MAIN_AGENT_ALIAS
            | MAIN_AGENT_LEGACY_ALIAS
            | MAIN_AGENT_FALLBACK_ALIAS
    )
}

pub(crate) fn is_weles_internal_scope(scope: &str) -> bool {
    let normalized = scope.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        WELES_GOVERNANCE_SCOPE | WELES_VITALITY_SCOPE
    )
}

pub(crate) fn is_weles_agent_scope(scope: &str) -> bool {
    let normalized = scope.trim().to_ascii_lowercase();
    normalized == WELES_AGENT_ID || is_weles_internal_scope(&normalized)
}

pub(crate) fn is_explicit_builtin_persona_scope(alias: &str) -> bool {
    matches!(
        canonical_agent_id(alias),
        SWAROZYC_AGENT_ID
            | RADOGOST_AGENT_ID
            | DOMOWOJ_AGENT_ID
            | SWIETOWIT_AGENT_ID
            | PERUN_AGENT_ID
            | MOKOSH_AGENT_ID
            | DAZHBOG_AGENT_ID
            | ROD_AGENT_ID
    )
}

pub(crate) fn builtin_persona_overrides<'a>(
    config: &'a AgentConfig,
    alias: &str,
) -> Option<&'a BuiltinPersonaOverrides> {
    if !is_explicit_builtin_persona_scope(alias) {
        return None;
    }
    match canonical_agent_id(alias) {
        SWAROZYC_AGENT_ID => Some(&config.builtin_sub_agents.swarozyc),
        RADOGOST_AGENT_ID => Some(&config.builtin_sub_agents.radogost),
        DOMOWOJ_AGENT_ID => Some(&config.builtin_sub_agents.domowoj),
        SWIETOWIT_AGENT_ID => Some(&config.builtin_sub_agents.swietowit),
        PERUN_AGENT_ID => Some(&config.builtin_sub_agents.perun),
        MOKOSH_AGENT_ID => Some(&config.builtin_sub_agents.mokosh),
        DAZHBOG_AGENT_ID => Some(&config.builtin_sub_agents.dazhbog),
        ROD_AGENT_ID => Some(&config.builtin_sub_agents.rod),
        _ => None,
    }
}

pub(crate) fn builtin_persona_overrides_mut<'a>(
    config: &'a mut AgentConfig,
    alias: &str,
) -> Option<&'a mut BuiltinPersonaOverrides> {
    if !is_explicit_builtin_persona_scope(alias) {
        return None;
    }
    match canonical_agent_id(alias) {
        SWAROZYC_AGENT_ID => Some(&mut config.builtin_sub_agents.swarozyc),
        RADOGOST_AGENT_ID => Some(&mut config.builtin_sub_agents.radogost),
        DOMOWOJ_AGENT_ID => Some(&mut config.builtin_sub_agents.domowoj),
        SWIETOWIT_AGENT_ID => Some(&mut config.builtin_sub_agents.swietowit),
        PERUN_AGENT_ID => Some(&mut config.builtin_sub_agents.perun),
        MOKOSH_AGENT_ID => Some(&mut config.builtin_sub_agents.mokosh),
        DAZHBOG_AGENT_ID => Some(&mut config.builtin_sub_agents.dazhbog),
        ROD_AGENT_ID => Some(&mut config.builtin_sub_agents.rod),
        _ => None,
    }
}

pub(crate) fn configurable_sub_agent_mut<'a>(
    sub_agents: &'a mut [SubAgentDefinition],
    aliases: &[&str],
) -> Option<&'a mut SubAgentDefinition> {
    sub_agents.iter_mut().find(|def| {
        aliases.iter().copied().any(|alias| {
            let trimmed = alias.trim();
            !trimmed.is_empty()
                && (def.id.eq_ignore_ascii_case(trimmed)
                    || def.name.eq_ignore_ascii_case(trimmed)
                    || def
                        .id
                        .strip_suffix("_builtin")
                        .is_some_and(|value| value.eq_ignore_ascii_case(trimmed)))
        })
    })
}

pub(crate) fn builtin_persona_requires_setup(config: &AgentConfig, alias: &str) -> bool {
    let Some(overrides) = builtin_persona_overrides(config, alias) else {
        return false;
    };
    overrides
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
        || overrides
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
}

pub(crate) fn builtin_persona_setup_error(alias: &str) -> anyhow::Error {
    let canonical_id = canonical_agent_id(alias);
    anyhow!(
        "builtin agent '{}' is not configured. Choose provider and model first.",
        canonical_id
    )
}

pub(crate) fn internal_dm_thread_id(agent_a: &str, agent_b: &str) -> String {
    let mut ids = [agent_turn_scope_id(agent_a), agent_turn_scope_id(agent_b)];
    ids.sort();
    format!("{INTERNAL_DM_THREAD_PREFIX}{}:{}", ids[0], ids[1])
}

fn internal_agent_display_name(alias: &str) -> String {
    let scoped = agent_turn_scope_id(alias);
    if canonical_agent_id(alias) == MAIN_AGENT_ID && !is_main_agent_scope(alias) {
        scoped
    } else {
        canonical_agent_name(alias).to_string()
    }
}

pub(crate) fn internal_dm_thread_title(agent_a: &str, agent_b: &str) -> String {
    let mut names = [
        internal_agent_display_name(agent_a),
        internal_agent_display_name(agent_b),
    ];
    names.sort();
    format!("Internal DM · {} ↔ {}", names[0], names[1])
}

pub(in crate::agent) fn is_internal_dm_thread(thread_id: &str) -> bool {
    thread_id.starts_with(INTERNAL_DM_THREAD_PREFIX)
}

pub(in crate::agent) fn is_goal_run_thread(thread_id: &str) -> bool {
    thread_id.starts_with("goal:")
}

pub(in crate::agent) fn participant_playground_thread_id(
    visible_thread_id: &str,
    participant_agent_id: &str,
) -> String {
    format!(
        "{PARTICIPANT_PLAYGROUND_THREAD_PREFIX}{}:{visible_thread_id}",
        canonical_agent_id(participant_agent_id)
    )
}

pub(in crate::agent) fn participant_playground_thread_title(
    visible_thread_id: &str,
    participant_agent_id: &str,
) -> String {
    format!(
        "Participant Playground · {} @ {visible_thread_id}",
        canonical_agent_name(participant_agent_id)
    )
}

pub(in crate::agent) fn is_participant_playground_thread(thread_id: &str) -> bool {
    thread_id.starts_with(PARTICIPANT_PLAYGROUND_THREAD_PREFIX)
}

pub(crate) fn spawned_persona_name(seed: &str) -> &'static str {
    spawned_persona(seed).name
}

pub(crate) fn spawned_persona_id(seed: &str) -> &'static str {
    spawned_persona(seed).id
}

pub(crate) fn build_spawned_persona_prompt(seed: &str) -> String {
    let persona = spawned_persona(seed);
    format!(
        "{PERSONA_MARKER} {}\n{PERSONA_ID_MARKER} {}\nYou are {} ({}) operating as a spawned zorai agent.\n{}\n{} is the main agent. {} is {}'s concierge. Know who they are, do not impersonate them, and coordinate with them when that helps move the task forward.\nKeep your personality distinct but concise, pragmatic, and production-focused.",
        persona.name,
        persona.id,
        persona.name,
        persona.id,
        persona.guidance,
        MAIN_AGENT_NAME,
        CONCIERGE_AGENT_NAME,
        MAIN_AGENT_NAME,
    )
}

pub(crate) fn build_user_subagent_persona_prompt(def: &SubAgentDefinition) -> String {
    let role_sentence = def
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("Your configured role is: {value}.\n"))
        .unwrap_or_default();
    format!(
        "{PERSONA_MARKER} {}\n{PERSONA_ID_MARKER} {}\nYou are {} ({}) operating as a user-defined zorai subagent for this thread.\n{}{} is the main agent. {} is {}'s concierge. Know who they are, do not impersonate them, and coordinate with them when that helps move the task forward.\nKeep your behavior aligned with your configured role and system prompt, and stay concise, pragmatic, and production-focused.",
        def.name,
        def.id,
        def.name,
        def.id,
        role_sentence,
        MAIN_AGENT_NAME,
        CONCIERGE_AGENT_NAME,
        MAIN_AGENT_NAME,
    )
}

pub(crate) fn build_weles_persona_prompt(scope: &str) -> String {
    format!(
        "{PERSONA_MARKER} {WELES_AGENT_NAME}\n{PERSONA_ID_MARKER} {WELES_AGENT_ID}\nYou are {WELES_AGENT_NAME} ({WELES_AGENT_ID}) operating as the daemon-owned WELES subagent.\nYour current internal scope is {scope}.\nYou exist to inspect risky execution paths and preserve daemon governance guarantees.\n{} is the main agent. {} is {}'s concierge. Do not impersonate either of them, and keep your reporting concrete.",
        MAIN_AGENT_NAME, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME,
    )
}

pub(crate) fn extract_persona_name(system_prompt: Option<&str>) -> Option<String> {
    system_prompt.and_then(|prompt| {
        prompt.lines().find_map(|line| {
            line.trim()
                .strip_prefix(PERSONA_MARKER)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
    })
}

pub(crate) fn extract_persona_id(system_prompt: Option<&str>) -> Option<String> {
    system_prompt.and_then(|prompt| {
        prompt.lines().find_map(|line| {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix(PERSONA_ID_MARKER) {
                return Some(value.trim().to_string()).filter(|value| !value.is_empty());
            }
            let prefix = "You are ";
            let suffix = " operating as a spawned zorai agent.";
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if let Some(body) = rest.strip_suffix(suffix) {
                    if let Some(start) = body.rfind('(') {
                        if let Some(end) = body.rfind(')') {
                            if start < end {
                                let id = body[start + 1..end].trim();
                                if !id.is_empty() {
                                    return Some(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
            None
        })
    })
}

pub(crate) fn agent_scope_id_for_task(task: Option<&AgentTask>) -> String {
    task.and_then(|item| extract_persona_id(item.override_system_prompt.as_deref()))
        .unwrap_or_else(|| MAIN_AGENT_ID.to_string())
}

pub(crate) fn current_agent_scope_id() -> String {
    ACTIVE_AGENT_SCOPE_ID
        .try_with(Clone::clone)
        .unwrap_or_else(|_| MAIN_AGENT_ID.to_string())
}

/// Resolve the task-local agent scope id for a turn driven by `alias`.
///
/// Built-in aliases (main, concierge, weles, spawned personas) canonicalize to
/// their stable ids, but a user sub-agent id — which `canonical_agent_id`
/// collapses to the main agent — is preserved so the turn runs under that
/// sub-agent's own provider/model config instead of the main agent's.
pub(crate) fn agent_turn_scope_id(alias: &str) -> String {
    let canonical = canonical_agent_id(alias);
    if canonical == MAIN_AGENT_ID && !is_main_agent_scope(alias) {
        alias.trim().to_string()
    } else {
        canonical.to_string()
    }
}

pub(crate) async fn run_with_agent_scope<F>(scope_id: String, future: F) -> F::Output
where
    F: Future,
{
    ACTIVE_AGENT_SCOPE_ID.scope(scope_id, future).await
}

pub(crate) fn sender_name_for_task(task: Option<&AgentTask>) -> String {
    task.and_then(|item| extract_persona_name(item.override_system_prompt.as_deref()))
        .unwrap_or_else(|| MAIN_AGENT_NAME.to_string())
}

pub(crate) fn wrap_internal_message(sender: &str, recipient: &str, content: &str) -> String {
    format!(
        "Internal agent message from {} to {}.\nRespond directly to the request below and assume the recipient will relay or integrate your answer.\n\n{}",
        internal_agent_display_name(sender),
        internal_agent_display_name(recipient),
        content.trim()
    )
}

pub(crate) fn concierge_should_escalate(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let keywords = [
        "code",
        "coding",
        "rust",
        "typescript",
        "react",
        "debug",
        "bug",
        "build",
        "compile",
        "test",
        "refactor",
        "implement",
        "file",
        "patch",
        "database",
        "schema",
        "migration",
        "daemon",
        "cli",
        "review",
    ];
    lower.len() > 220 || keywords.iter().any(|keyword| lower.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_unique_spawned_persona_seed_prefers_unused_persona() {
        let primary_id = spawned_persona_id_for_seed("task_seed_a");
        let picked_id = pick_unique_spawned_persona_seed("task_seed_a", ["weles"])
            .expect("persona should be picked when primary is free");
        assert_eq!(picked_id, spawned_persona_id_for_seed("task_seed_a"));
    }

    #[test]
    fn pick_unique_spawned_persona_seed_rotates_on_collision() {
        let primary_id = spawned_persona_id_for_seed("task_seed_a");
        let picked = pick_unique_spawned_persona_seed("task_seed_a", [primary_id])
            .expect("an alternate persona should be picked");
        assert_ne!(picked, primary_id);
        // Deterministic: the same collision yields the same alternate.
        let picked_again = pick_unique_spawned_persona_seed("task_seed_a", [primary_id])
            .expect("alternate pick should be repeatable");
        assert_eq!(picked, picked_again);
    }

    #[test]
    fn pick_unique_spawned_persona_seed_returns_none_when_all_taken() {
        let all: Vec<&str> = SPAWNED_PERSONAS.iter().map(|persona| persona.id).collect();
        assert!(pick_unique_spawned_persona_seed("task_seed_a", all).is_none());
    }

    #[test]
    fn internal_dm_thread_id_is_stable_and_sorted() {
        assert_eq!(
            internal_dm_thread_id(CONCIERGE_AGENT_ID, MAIN_AGENT_ID),
            internal_dm_thread_id(MAIN_AGENT_ID, CONCIERGE_AGENT_ID)
        );
        assert_eq!(
            internal_dm_thread_id(MAIN_AGENT_ALIAS, CONCIERGE_AGENT_ALIAS),
            format!("dm:{}:{}", CONCIERGE_AGENT_ID, MAIN_AGENT_ID)
        );
    }

    #[test]
    fn internal_dm_thread_id_preserves_user_subagent_sender() {
        let sender = "subagent-1781800311670";
        assert_eq!(
            internal_dm_thread_id(sender, WELES_AGENT_ID),
            format!("dm:{sender}:{WELES_AGENT_ID}")
        );
        assert!(wrap_internal_message(sender, WELES_AGENT_ID, "inspect")
            .starts_with("Internal agent message from subagent-1781800311670 to Weles."));
    }

    #[test]
    fn spawned_persona_prompt_is_stable_for_seed() {
        let first = build_spawned_persona_prompt("task-123");
        let second = build_spawned_persona_prompt("task-123");
        assert_eq!(first, second);
        assert!(first.contains(PERSONA_MARKER));
    }

    #[test]
    fn spawned_persona_id_is_stable_for_seed() {
        let first = spawned_persona_id("task-123");
        let second = spawned_persona_id("task-123");
        assert_eq!(first, second);
        assert!(SPAWNED_PERSONAS.iter().any(|persona| persona.id == first));
    }

    #[test]
    fn agent_turn_scope_preserves_user_subagent_ids() {
        // A user sub-agent id must drive a turn under its own scope (and thus its
        // own provider), not collapse to the main agent.
        assert_eq!(
            agent_turn_scope_id("subagent-1777071832136"),
            "subagent-1777071832136"
        );
        assert_eq!(agent_turn_scope_id("main"), MAIN_AGENT_ID);
        assert_eq!(agent_turn_scope_id(MAIN_AGENT_ID), MAIN_AGENT_ID);
        assert_eq!(agent_turn_scope_id("radogost"), RADOGOST_AGENT_ID);
        assert_eq!(agent_turn_scope_id("veles"), WELES_AGENT_ID);
    }

    #[test]
    fn canonical_ids_follow_aliases() {
        assert_eq!(canonical_agent_id(MAIN_AGENT_ALIAS), MAIN_AGENT_ID);
        assert_eq!(
            canonical_agent_id(CONCIERGE_AGENT_ALIAS),
            CONCIERGE_AGENT_ID
        );
        assert_eq!(canonical_agent_id(MAIN_AGENT_FALLBACK_ALIAS), MAIN_AGENT_ID);
        assert_eq!(canonical_agent_id("veles"), WELES_AGENT_ID);
    }

    #[test]
    fn canonical_ids_and_names_cover_spawned_personas() {
        assert_eq!(canonical_agent_id(RADOGOST_AGENT_ID), RADOGOST_AGENT_ID);
        assert_eq!(canonical_agent_id("Radogost"), RADOGOST_AGENT_ID);
        assert_eq!(canonical_agent_name(RADOGOST_AGENT_ID), RADOGOST_AGENT_NAME);
        assert_eq!(canonical_agent_id("perun"), "perun");
        assert_eq!(canonical_agent_name("perun"), "Perun");
        assert_eq!(canonical_agent_id("mokosh"), "mokosh");
        assert_eq!(canonical_agent_name("mokosh"), "Mokosh");
        assert_eq!(canonical_agent_id("dazhbog"), "dazhbog");
        assert_eq!(canonical_agent_name("dazhbog"), "Dazhbog");
        assert_eq!(canonical_agent_id("rod"), ROD_AGENT_ID);
        assert_eq!(canonical_agent_name("Rod"), ROD_AGENT_NAME);
    }

    #[test]
    fn lead_personas_cannot_be_assigned_by_spawn() {
        for alias in [
            "svarog",
            "Svarog",
            "swarog",
            "rarog",
            "Rarog",
            "weles",
            "Weles",
            "veles",
            "weles_builtin",
        ] {
            assert!(is_lead_only_persona(alias), "{alias} is a lead persona");
        }
        for persona in SPAWNED_PERSONAS {
            assert!(!is_lead_only_persona(persona.id), "{}", persona.id);
            assert!(!is_lead_only_persona(persona.name), "{}", persona.name);
        }
        assert_eq!(spawned_persona_count(), 8);
        let assigned = spawned_persona_id("task-123");
        assert_ne!(assigned, WELES_AGENT_ID);
        assert_ne!(assigned, MAIN_AGENT_ID);
        assert_ne!(assigned, CONCIERGE_AGENT_ID);
    }

    #[test]
    fn canonical_agent_guidance_resolves_spawned_personas_only() {
        assert!(canonical_agent_guidance(MAIN_AGENT_ID).is_none());
        assert!(canonical_agent_guidance(CONCIERGE_AGENT_ID).is_none());
        assert_eq!(
            canonical_agent_guidance(RADOGOST_AGENT_ID),
            Some(
                "You specialize in negotiation between options, comparing tradeoffs, and surfacing the strongest route forward."
            )
        );
    }

    fn aliases(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(id, alias)| (id.to_string(), alias.to_string()))
            .collect()
    }

    #[test]
    fn alias_resolves_to_the_canonical_agent_so_routing_keeps_using_ids() {
        let map = aliases(&[(MAIN_AGENT_ID, "Atlas"), (CONCIERGE_AGENT_ID, "Butler")]);
        assert_eq!(agent_id_for_alias_in(" atlas ", &map), Some(MAIN_AGENT_ID));
        assert_eq!(agent_id_for_alias_in("BUTLER", &map), Some(CONCIERGE_AGENT_ID));
        assert_eq!(agent_id_for_alias_in("weles", &map), None);
    }

    #[test]
    fn display_name_uses_alias_and_falls_back_to_the_builtin_name() {
        let map = aliases(&[(WELES_AGENT_ID, "Judge")]);
        assert_eq!(agent_display_name_in(WELES_AGENT_ID, &map), "Judge");
        assert_eq!(agent_display_name_in(CONCIERGE_AGENT_ID, &map), CONCIERGE_AGENT_NAME);
    }

    #[test]
    fn only_builtin_names_pick_up_an_alias_so_subagents_are_never_renamed() {
        let map = aliases(&[(MAIN_AGENT_ID, "Atlas")]);
        assert_eq!(alias_for_agent_name_in(MAIN_AGENT_NAME, &map).as_deref(), Some("Atlas"));
        assert_eq!(alias_for_agent_name_in("Coder", &map), None);
        assert_eq!(alias_for_agent_name_in(RADOGOST_AGENT_NAME, &map), None);
    }

    #[test]
    fn prompt_note_tells_agents_who_each_alias_is() {
        let map = aliases(&[(MAIN_AGENT_ID, "Atlas")]);
        let note = agent_alias_prompt_note_in(&map).expect("note");
        assert!(note.contains("Atlas is the operator's name for Svarog"));
        assert!(agent_alias_prompt_note_in(&aliases(&[(MAIN_AGENT_ID, "  ")])).is_none());
    }

    #[test]
    fn validation_rejects_aliases_that_would_make_routing_ambiguous() {
        assert!(validate_agent_aliases(&aliases(&[(MAIN_AGENT_ID, "Atlas")]), &[]).is_ok());
        assert!(validate_agent_aliases(&aliases(&[(MAIN_AGENT_ID, "Svarog")]), &[]).is_ok());
        assert!(validate_agent_aliases(&aliases(&[(MAIN_AGENT_ID, "Rarog")]), &[]).is_err());
        assert!(validate_agent_aliases(&aliases(&[(MAIN_AGENT_ID, "Perun")]), &[]).is_err());
        assert!(validate_agent_aliases(
            &aliases(&[(MAIN_AGENT_ID, "Atlas"), (WELES_AGENT_ID, "atlas")]),
            &[]
        )
        .is_err());
        assert!(validate_agent_aliases(&aliases(&[("perun", "Atlas")]), &[]).is_err());
    }
}
