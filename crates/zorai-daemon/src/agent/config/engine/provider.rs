use super::super::*;
use crate::agent::{
    claude_code_auth, copilot_auth, cursor_auth, openai_codex_auth, provider_resolution,
};
use zorai_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

impl AgentEngine {
    pub(super) fn normalize_agent_model_target(target: &str) -> String {
        let normalized = target.trim().to_ascii_lowercase();
        match normalized.as_str() {
            crate::agent::agent_identity::MAIN_AGENT_ID
            | zorai_protocol::AGENT_HANDLE_SVAROG
            | crate::agent::agent_identity::MAIN_AGENT_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_LEGACY_ALIAS
            | crate::agent::agent_identity::MAIN_AGENT_FALLBACK_ALIAS => {
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::CONCIERGE_AGENT_ID
            | crate::agent::agent_identity::CONCIERGE_AGENT_ALIAS
            | crate::agent::agent_identity::CONCIERGE_AGENT_LEGACY_ALIAS => {
                crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::WELES_AGENT_ID
            | "veles"
            | crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID => {
                crate::agent::agent_identity::WELES_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::SWAROZYC_AGENT_ID => {
                crate::agent::agent_identity::SWAROZYC_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::RADOGOST_AGENT_ID => {
                crate::agent::agent_identity::RADOGOST_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::DOMOWOJ_AGENT_ID => {
                crate::agent::agent_identity::DOMOWOJ_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::SWIETOWIT_AGENT_ID => {
                crate::agent::agent_identity::SWIETOWIT_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::PERUN_AGENT_ID => {
                crate::agent::agent_identity::PERUN_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::MOKOSH_AGENT_ID => {
                crate::agent::agent_identity::MOKOSH_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::DAZHBOG_AGENT_ID => {
                crate::agent::agent_identity::DAZHBOG_AGENT_ID.to_string()
            }
            crate::agent::agent_identity::ROD_AGENT_ID => {
                crate::agent::agent_identity::ROD_AGENT_ID.to_string()
            }
            _ if normalized
                == crate::agent::agent_identity::WELES_AGENT_NAME.to_ascii_lowercase() =>
            {
                crate::agent::agent_identity::WELES_AGENT_ID.to_string()
            }
            _ => normalized,
        }
    }

    fn validate_non_main_agent_provider_model(
        &self,
        config: &AgentConfig,
        provider_id: &str,
        model: &str,
    ) -> Result<()> {
        let resolved =
            provider_resolution::resolve_provider_config_for(config, provider_id, Some(model))?;
        let mut validation_config = config.clone();
        validation_config.provider = provider_id.trim().to_string();
        validation_config.model = model.trim().to_string();
        validation_config.base_url = resolved.base_url.clone();
        validation_config.api_key = resolved.api_key.clone();
        validation_config.auth_source = resolved.auth_source;
        validation_config.api_transport = resolved.api_transport;
        validation_config.context_window_tokens = resolved.context_window_tokens;
        let _ = provider_resolution::resolve_provider_model_switch(
            &validation_config,
            provider_id,
            model,
            true,
        )?;
        Ok(())
    }

    pub async fn set_provider_model_json(
        &self,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let updated = self.prepare_provider_model_json(provider_id, model).await?;
        self.persist_prepared_provider_model_json(updated.clone())
            .await;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(self.get_config().await)
    }

    pub async fn persist_prepared_provider_model_json(&self, merged: AgentConfig) {
        let mut merged = merged;
        let collisions = sanitize_weles_collisions_from_config(&mut merged);
        let _ = self.persist_sanitized_config(merged, collisions).await;
        let mut projection = self.config_runtime_projection.lock().await;
        projection.desired_revision = projection.desired_revision.saturating_add(1);
        projection.state = ConfigReconcileState::Reconciling;
        projection.last_error = None;
    }

    pub async fn prepare_provider_model_json(
        &self,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let current = self.get_config().await;
        let selection = provider_resolution::resolve_provider_model_switch(
            &current,
            provider_id,
            model,
            false,
        )?;

        let mut updated = current;
        updated.provider = selection.provider_id;
        updated.model = selection.model;
        updated.base_url = selection.base_url;
        updated.api_transport = selection.api_transport;
        updated.context_window_tokens = selection.context_window_tokens;

        Ok(updated)
    }

    pub async fn prepare_agent_provider_model_json(
        &self,
        target_agent: &str,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let provider_id = provider_id.trim();
        let model = model.trim();
        let target = Self::normalize_agent_model_target(target_agent);

        if provider_id.is_empty() {
            anyhow::bail!("provider id cannot be empty");
        }
        if model.is_empty() {
            anyhow::bail!("model cannot be empty");
        }

        let current = self.get_config().await;
        match target.as_str() {
            crate::agent::agent_identity::MAIN_AGENT_ID => {
                self.prepare_provider_model_json(provider_id, model).await
            }
            crate::agent::agent_identity::CONCIERGE_AGENT_ID => {
                self.validate_non_main_agent_provider_model(&current, provider_id, model)?;
                let mut updated = current;
                updated.concierge.provider = Some(provider_id.to_string());
                updated.concierge.model = Some(model.to_string());
                Ok(updated)
            }
            crate::agent::agent_identity::WELES_AGENT_ID => {
                self.validate_non_main_agent_provider_model(&current, provider_id, model)?;
                let mut updated = current;
                updated.builtin_sub_agents.weles.provider = Some(provider_id.to_string());
                updated.builtin_sub_agents.weles.model = Some(model.to_string());
                Ok(updated)
            }
            _ => {
                self.validate_non_main_agent_provider_model(&current, provider_id, model)?;
                let mut updated = current;
                if let Some(overrides) = crate::agent::agent_identity::builtin_persona_overrides_mut(
                    &mut updated,
                    &target,
                ) {
                    overrides.provider = Some(provider_id.to_string());
                    overrides.model = Some(model.to_string());
                    return Ok(updated);
                }
                let sub_agent = crate::agent::agent_identity::configurable_sub_agent_mut(
                    &mut updated.sub_agents,
                    &[target_agent, target.as_str()],
                )
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown agent '{}'. Use `list_agents` to inspect valid targets.",
                        target_agent.trim()
                    )
                })?;
                sub_agent.provider = provider_id.to_string();
                sub_agent.model = model.to_string();
                Ok(updated)
            }
        }
    }

    pub async fn switch_agent_provider_model_json(
        &self,
        target_agent: &str,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let updated = self
            .prepare_agent_provider_model_json(target_agent, provider_id, model)
            .await?;
        self.persist_prepared_provider_model_json(updated.clone())
            .await;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(self.get_config().await)
    }

    pub async fn prepare_agent_reasoning_effort_json(
        &self,
        target_agent: &str,
        reasoning_effort: &str,
    ) -> Result<AgentConfig> {
        let target = Self::normalize_agent_model_target(target_agent);
        let reasoning_effort = reasoning_effort.trim().to_string();
        let current = self.get_config().await;
        match target.as_str() {
            crate::agent::agent_identity::MAIN_AGENT_ID => {
                let mut updated = current;
                updated.reasoning_effort = reasoning_effort.clone();
                let provider_id = updated.provider.clone();
                if let Some(provider) = updated.providers.get_mut(&provider_id) {
                    provider.reasoning_effort = reasoning_effort;
                }
                Ok(updated)
            }
            crate::agent::agent_identity::CONCIERGE_AGENT_ID => {
                let mut updated = current;
                updated.concierge.reasoning_effort =
                    (!reasoning_effort.is_empty()).then_some(reasoning_effort);
                Ok(updated)
            }
            crate::agent::agent_identity::WELES_AGENT_ID => {
                let mut updated = current;
                updated.builtin_sub_agents.weles.reasoning_effort =
                    (!reasoning_effort.is_empty()).then_some(reasoning_effort);
                Ok(updated)
            }
            _ => {
                let mut updated = current;
                if let Some(overrides) = crate::agent::agent_identity::builtin_persona_overrides_mut(
                    &mut updated,
                    &target,
                ) {
                    overrides.reasoning_effort =
                        (!reasoning_effort.is_empty()).then_some(reasoning_effort);
                    return Ok(updated);
                }
                let sub_agent = crate::agent::agent_identity::configurable_sub_agent_mut(
                    &mut updated.sub_agents,
                    &[target_agent, target.as_str()],
                )
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown agent '{}'. Use `list_agents` to inspect valid targets.",
                        target_agent.trim()
                    )
                })?;
                sub_agent.reasoning_effort =
                    (!reasoning_effort.is_empty()).then_some(reasoning_effort);
                Ok(updated)
            }
        }
    }

    pub async fn merge_config_patch_json(&self, patch_json: &str) -> Result<AgentConfig> {
        let mut patch_value: Value =
            serde_json::from_str(patch_json).context("invalid config patch JSON")?;
        normalize_config_keys_to_snake_case(&mut patch_value);
        let mut merged_value = serde_json::to_value(self.get_config().await)?;
        normalize_config_keys_to_snake_case(&mut merged_value);
        merge_json_value(&mut merged_value, patch_value);
        sanitize_config_value(&mut merged_value);
        let merged = match serde_json::from_value::<AgentConfig>(merged_value.clone()) {
            Ok(merged) => merged,
            Err(error) => {
                let redacted_patch = serde_json::from_str::<Value>(patch_json)
                    .map(|value| redact_config_value(&value))
                    .unwrap_or_else(|_| Value::String("<invalid-json>".to_string()));
                let redacted_merged = redact_config_value(&merged_value);
                tracing::warn!(
                    error = %error,
                    patch_keys = ?top_level_config_keys(&redacted_patch),
                    merged_keys = ?top_level_config_keys(&redacted_merged),
                    patch = %redacted_patch,
                    merged = %redacted_merged,
                    "agent config patch merge failed"
                );
                return Err(error).context("merged config patch could not be parsed");
            }
        };
        self.set_config(merged.clone()).await;
        let _ = self.reinit_gateway().await;
        Ok(merged)
    }

    /// Build provider auth states by merging persisted config with PROVIDER_DEFINITIONS.
    pub async fn get_provider_auth_states(&self) -> Vec<ProviderAuthState> {
        use crate::agent::types::{
            all_provider_definitions, custom_provider_config,
            reload_custom_provider_catalog_from_default_path, ProviderAuthState,
        };

        let _ = reload_custom_provider_catalog_from_default_path();

        let config = self.config.read().await;
        let mut states = Vec::new();
        let use_legacy_top_level_fallback = config.providers.is_empty();

        for def in all_provider_definitions() {
            if def.id == zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI {
                let pc = config.providers.get(def.id);
                states.push(ProviderAuthState {
                    provider_id: def.id.to_string(),
                    provider_name: def.name.to_string(),
                    authenticated: claude_code_auth::claude_code_cli_authenticated(),
                    auth_source: AuthSource::default(),
                    model: pc
                        .map(|pc| pc.model.clone())
                        .filter(|model| !model.trim().is_empty())
                        .unwrap_or_else(|| def.default_model.to_string()),
                    base_url: pc.map(|pc| pc.base_url.clone()).unwrap_or_default(),
                });
                continue;
            }
            if def.id == zorai_shared::providers::PROVIDER_ID_CURSOR {
                let pc = config.providers.get(def.id);
                let has_key = pc.map(|pc| !pc.api_key.trim().is_empty()).unwrap_or(false)
                    || (use_legacy_top_level_fallback
                        && config.provider == def.id
                        && !config.api_key.trim().is_empty());
                states.push(ProviderAuthState {
                    provider_id: def.id.to_string(),
                    provider_name: def.name.to_string(),
                    authenticated: cursor_auth::cursor_subscription_authenticated() || has_key,
                    auth_source: AuthSource::default(),
                    model: pc
                        .map(|pc| pc.model.clone())
                        .filter(|model| !model.trim().is_empty())
                        .unwrap_or_else(|| def.default_model.to_string()),
                    base_url: String::new(),
                });
                continue;
            }
            let (authenticated, auth_source, model, base_url) =
                if let Some(pc) = config.providers.get(def.id) {
                    if def.id == PROVIDER_ID_GITHUB_COPILOT {
                        let resolved =
                            copilot_auth::resolve_github_copilot_auth(&pc.api_key, pc.auth_source);
                        (
                            resolved.is_some(),
                            resolved
                                .as_ref()
                                .map(|auth| auth.auth_source)
                                .unwrap_or(pc.auth_source),
                            pc.model.clone(),
                            pc.base_url.clone(),
                        )
                    } else if def.id == PROVIDER_ID_OPENAI
                        && pc.auth_source == AuthSource::ChatgptSubscription
                    {
                        (
                            openai_codex_auth::provider_auth_state_authenticated(),
                            pc.auth_source,
                            pc.model.clone(),
                            pc.base_url.clone(),
                        )
                    } else {
                        (
                            !pc.api_key.is_empty(),
                            pc.auth_source,
                            pc.model.clone(),
                            pc.base_url.clone(),
                        )
                    }
                } else if use_legacy_top_level_fallback && config.provider == def.id {
                    if def.id == PROVIDER_ID_GITHUB_COPILOT {
                        let resolved = copilot_auth::resolve_github_copilot_auth(
                            &config.api_key,
                            config.auth_source,
                        );
                        (
                            resolved.is_some(),
                            resolved
                                .as_ref()
                                .map(|auth| auth.auth_source)
                                .unwrap_or(config.auth_source),
                            config.model.clone(),
                            config.base_url.clone(),
                        )
                    } else if def.id == PROVIDER_ID_OPENAI
                        && config.auth_source == AuthSource::ChatgptSubscription
                    {
                        (
                            openai_codex_auth::provider_auth_state_authenticated(),
                            config.auth_source,
                            config.model.clone(),
                            config.base_url.clone(),
                        )
                    } else {
                        (
                            !config.api_key.is_empty(),
                            config.auth_source,
                            config.model.clone(),
                            config.base_url.clone(),
                        )
                    }
                } else if def.id == PROVIDER_ID_GITHUB_COPILOT {
                    let resolved =
                        copilot_auth::resolve_github_copilot_auth("", AuthSource::GithubCopilot);
                    (
                        resolved.is_some(),
                        resolved
                            .as_ref()
                            .map(|auth| auth.auth_source)
                            .unwrap_or(AuthSource::GithubCopilot),
                        def.default_model.to_string(),
                        def.default_base_url.to_string(),
                    )
                } else if let Some(custom_config) = custom_provider_config(def.id) {
                    (
                        !custom_config.api_key.trim().is_empty(),
                        custom_config.auth_source,
                        custom_config.model,
                        custom_config.base_url,
                    )
                } else {
                    (
                        false,
                        AuthSource::default(),
                        def.default_model.to_string(),
                        def.default_base_url.to_string(),
                    )
                };

            states.push(ProviderAuthState {
                provider_id: def.id.to_string(),
                provider_name: def.name.to_string(),
                authenticated,
                auth_source,
                model,
                base_url,
            });
        }

        states
    }
}
