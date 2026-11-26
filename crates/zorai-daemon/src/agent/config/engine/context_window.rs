use super::super::*;
use crate::agent::agent_identity::{
    canonical_agent_id, is_concierge_target, is_explicit_builtin_persona_scope,
    is_main_agent_scope, is_weles_agent_scope, CONCIERGE_AGENT_ID, MAIN_AGENT_ID, WELES_AGENT_ID,
};
use crate::agent::types::{AgentThread, ThreadExecutionProfile};

const MIN_CONTEXT_WINDOW_TOKENS: u32 = 1_000;
const MAX_CONTEXT_WINDOW_TOKENS: u32 = 2_000_000;

fn clamp_context_window_tokens(tokens: u32) -> u32 {
    tokens.clamp(MIN_CONTEXT_WINDOW_TOKENS, MAX_CONTEXT_WINDOW_TOKENS)
}

struct AgentExecutionFields {
    provider: String,
    model: String,
    reasoning_effort: Option<String>,
    context_window_tokens: Option<u32>,
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn agent_ids_match(left: &str, right: &str) -> bool {
    if left.eq_ignore_ascii_case(right) {
        return true;
    }
    let known = |alias: &str| {
        is_main_agent_scope(alias)
            || is_concierge_target(alias)
            || is_weles_agent_scope(alias)
            || is_explicit_builtin_persona_scope(alias)
    };
    known(left) && known(right) && canonical_agent_id(left) == canonical_agent_id(right)
}

fn thread_matches_agent(thread: &AgentThread, agent_id: &str) -> bool {
    let target = agent_id.trim();
    if target.is_empty() {
        return false;
    }
    let target_is_main = is_main_agent_scope(target);
    let Some(name) = thread
        .agent_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return target_is_main;
    };
    if name.eq_ignore_ascii_case(target) {
        return true;
    }
    agent_ids_match(name, target)
}

fn execution_fields_for_agent(
    config: &AgentConfig,
    agent_id: &str,
) -> Option<AgentExecutionFields> {
    let target = AgentEngine::normalize_agent_model_target(agent_id);
    match target.as_str() {
        MAIN_AGENT_ID => Some(AgentExecutionFields {
            provider: config.provider.clone(),
            model: config.model.clone(),
            reasoning_effort: nonempty(Some(config.reasoning_effort.clone())),
            context_window_tokens: (config.context_window_tokens > 0)
                .then_some(config.context_window_tokens),
        }),
        CONCIERGE_AGENT_ID => Some(AgentExecutionFields {
            provider: nonempty(config.concierge.provider.clone())
                .unwrap_or_else(|| config.provider.clone()),
            model: nonempty(config.concierge.model.clone()).unwrap_or_else(|| config.model.clone()),
            reasoning_effort: nonempty(config.concierge.reasoning_effort.clone()),
            context_window_tokens: None,
        }),
        crate::agent::agent_identity::LESZY_AGENT_ID => Some(AgentExecutionFields {
            provider: nonempty(config.coder.provider.clone())
                .unwrap_or_else(|| config.provider.clone()),
            model: nonempty(config.coder.model.clone()).unwrap_or_else(|| config.model.clone()),
            reasoning_effort: nonempty(config.coder.reasoning_effort.clone()),
            context_window_tokens: None,
        }),
        WELES_AGENT_ID => {
            let overrides = &config.builtin_sub_agents.weles;
            Some(AgentExecutionFields {
                provider: nonempty(overrides.provider.clone())
                    .unwrap_or_else(|| config.provider.clone()),
                model: nonempty(overrides.model.clone()).unwrap_or_else(|| config.model.clone()),
                reasoning_effort: nonempty(overrides.reasoning_effort.clone()),
                context_window_tokens: overrides.context_window_tokens,
            })
        }
        crate::agent::agent_identity::SWAROZYC_AGENT_ID
        | crate::agent::agent_identity::RADOGOST_AGENT_ID
        | crate::agent::agent_identity::DOMOWOJ_AGENT_ID
        | crate::agent::agent_identity::SWIETOWIT_AGENT_ID
        | crate::agent::agent_identity::PERUN_AGENT_ID
        | crate::agent::agent_identity::MOKOSH_AGENT_ID
        | crate::agent::agent_identity::DAZHBOG_AGENT_ID
        | crate::agent::agent_identity::ROD_AGENT_ID
        | crate::agent::agent_identity::KIKIMORA_AGENT_ID
        | crate::agent::agent_identity::BANNIK_AGENT_ID
        | crate::agent::agent_identity::OVINNIK_AGENT_ID
        | crate::agent::agent_identity::DZIAD_AGENT_ID => {
            let overrides =
                crate::agent::agent_identity::builtin_persona_overrides(config, &target)?;
            Some(AgentExecutionFields {
                provider: nonempty(overrides.provider.clone())
                    .unwrap_or_else(|| config.provider.clone()),
                model: nonempty(overrides.model.clone()).unwrap_or_else(|| config.model.clone()),
                reasoning_effort: nonempty(overrides.reasoning_effort.clone()),
                context_window_tokens: overrides.context_window_tokens,
            })
        }
        _ => {
            let sub_agent = config.sub_agents.iter().find(|candidate| {
                candidate.id.eq_ignore_ascii_case(&target)
                    || candidate.name.eq_ignore_ascii_case(agent_id.trim())
            })?;
            Some(AgentExecutionFields {
                provider: sub_agent.provider.clone(),
                model: sub_agent.model.clone(),
                reasoning_effort: nonempty(sub_agent.reasoning_effort.clone()),
                context_window_tokens: sub_agent.context_window_tokens,
            })
        }
    }
}

fn apply_context_window_to_target(
    config: &mut AgentConfig,
    target: &str,
    original_target: &str,
    tokens: u32,
) -> Result<()> {
    match target {
        MAIN_AGENT_ID => {
            config.context_window_tokens = tokens;
            let provider_id = config.provider.clone();
            if let Some(provider) = config.providers.get_mut(&provider_id) {
                provider.context_window_tokens = tokens;
            }
        }
        CONCIERGE_AGENT_ID => {}
        WELES_AGENT_ID => {
            config.builtin_sub_agents.weles.context_window_tokens = Some(tokens);
        }
        _ => {
            if let Some(overrides) =
                crate::agent::agent_identity::builtin_persona_overrides_mut(config, target)
            {
                overrides.context_window_tokens = Some(tokens);
                return Ok(());
            }
            let sub_agent = crate::agent::agent_identity::configurable_sub_agent_mut(
                &mut config.sub_agents,
                &[target, original_target],
            )
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown agent '{}'. Use `list_agents` to inspect valid targets.",
                    original_target.trim()
                )
            })?;
            sub_agent.context_window_tokens = Some(tokens);
        }
    }
    Ok(())
}

impl AgentEngine {
    pub(crate) fn config_item_affects_swarog_execution_profile(key_path: &str) -> bool {
        let key = key_path.trim().trim_start_matches('/');
        matches!(
            key,
            "model" | "provider" | "context_window_tokens" | "reasoning_effort"
        ) || key.ends_with("/model")
            || key.ends_with("/context_window_tokens")
            || key.ends_with("/reasoning_effort")
    }

    pub async fn prepare_agent_context_window_json(
        &self,
        target_agent: &str,
        context_window_tokens: u32,
    ) -> Result<AgentConfig> {
        let tokens = clamp_context_window_tokens(context_window_tokens);
        let target = Self::normalize_agent_model_target(target_agent);
        let mut updated = self.get_config().await;
        apply_context_window_to_target(&mut updated, &target, target_agent, tokens)?;
        Ok(updated)
    }

    pub async fn sync_thread_execution_profiles_for_agent(&self, agent_id: &str) {
        let config = self.get_config().await;
        let Some(fields) = execution_fields_for_agent(&config, agent_id) else {
            return;
        };
        self.patch_owned_thread_execution_profiles(agent_id, |profile| {
            profile.provider = nonempty(Some(fields.provider.clone()));
            profile.model = nonempty(Some(fields.model.clone()));
            profile.reasoning_effort = fields.reasoning_effort.clone();
            if fields.context_window_tokens.is_some() {
                profile.context_window_tokens = fields.context_window_tokens;
            }
        })
        .await;
    }

    pub async fn set_owned_thread_execution_profile_context(
        &self,
        agent_id: &str,
        context_window_tokens: u32,
    ) {
        let tokens = clamp_context_window_tokens(context_window_tokens);
        self.patch_owned_thread_execution_profiles(agent_id, |profile| {
            profile.context_window_tokens = Some(tokens);
        })
        .await;
    }

    async fn patch_owned_thread_execution_profiles(
        &self,
        agent_id: &str,
        mut patch: impl FnMut(&mut ThreadExecutionProfile),
    ) {
        let threads = self.threads.read().await;
        let handoffs = self.thread_handoff_states.read().await;
        let mut owned_ids = Vec::new();
        for (thread_id, thread) in threads.iter() {
            if handoffs
                .get(thread_id)
                .is_some_and(|state| agent_ids_match(&state.active_agent_id, agent_id))
                || thread_matches_agent(thread, agent_id)
            {
                owned_ids.push(thread_id.clone());
            }
        }
        drop(handoffs);
        drop(threads);

        if owned_ids.is_empty() {
            return;
        }

        let mut profiles = self.thread_execution_profiles.write().await;
        for thread_id in &owned_ids {
            patch(profiles.entry(thread_id.clone()).or_default());
        }
        drop(profiles);
        for thread_id in owned_ids {
            self.notify_stream_retry_waiters(&thread_id).await;
        }
    }
}
