use super::*;

pub(crate) fn resolve_coder_provider(config: &AgentConfig) -> Result<(String, ProviderConfig)> {
    let coder = &config.coder;
    let provider_id = coder
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&config.provider)
        .to_string();
    let mut resolved = resolve_provider_config_for(config, &provider_id, coder.model.as_deref())?;
    apply_role_transport_override(&provider_id, &mut resolved, coder.api_transport);
    if let Some(base_url) = coder
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        resolved.base_url = base_url.to_string();
    }
    resolved.reasoning_effort = coder
        .reasoning_effort
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| config.reasoning_effort.clone());
    resolved.apply_generation_settings(coder.generation_settings.as_ref());
    Ok((provider_id, resolved))
}

pub(crate) fn ensure_answer_room(provider_config: &mut ProviderConfig, answer_tokens: u32) {
    if let Some(budget) =
        crate::agent::llm_client::anthropic_thinking_budget(&provider_config.reasoning_effort)
    {
        let needed = budget.saturating_add(answer_tokens);
        if provider_config.max_tokens.map_or(true, |max| max < needed) {
            provider_config.max_tokens = Some(needed);
        }
    }
}

pub(crate) fn leszy_persona_prompt(config: &AgentConfig) -> String {
    let identity = "Agent persona: Leszy, the coding agent. You write, fix, and refactor code: read before you change, keep edits surgical, and verify with builds and tests before saying it works.";
    match config
        .coder
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(persona) => format!("{identity}\n\n{persona}"),
        None => identity.to_string(),
    }
}

pub(crate) fn coder_system_prompt(config: &AgentConfig, task_prompt: &str) -> String {
    match config
        .coder
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
    {
        Some(persona) => format!("{persona}\n\n{task_prompt}"),
        None => task_prompt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coder_falls_back_to_the_main_agent_so_it_works_before_any_setup() {
        let mut config = AgentConfig::default();
        config.reasoning_effort = "high".to_string();
        let (provider_id, resolved) = resolve_coder_provider(&config).expect("resolves");
        assert_eq!(provider_id, config.provider);
        assert_eq!(resolved.reasoning_effort, "high");
    }

    #[test]
    fn thinking_never_eats_the_whole_output_budget() {
        let mut provider_config = resolve_coder_provider(&AgentConfig::default())
            .expect("resolves")
            .1;
        provider_config.reasoning_effort = "high".to_string();
        provider_config.max_tokens = None;
        ensure_answer_room(&mut provider_config, 4096);
        assert_eq!(provider_config.max_tokens, Some(8192 + 4096));
        provider_config.max_tokens = Some(64_000);
        ensure_answer_room(&mut provider_config, 4096);
        assert_eq!(provider_config.max_tokens, Some(64_000));
    }

    #[test]
    fn coder_settings_override_the_main_agent() {
        let mut config = AgentConfig::default();
        config.reasoning_effort = "high".to_string();
        config.coder.reasoning_effort = Some("low".to_string());
        config.coder.model = Some("coder-model".to_string());
        let (_, resolved) = resolve_coder_provider(&config).expect("resolves");
        assert_eq!(resolved.reasoning_effort, "low");
        assert_eq!(resolved.model, "coder-model");
    }

    #[test]
    fn leszy_is_a_real_thread_owner_with_its_own_persona() {
        assert_eq!(
            crate::agent::agent_identity::canonical_agent_id("Leszy"),
            crate::agent::agent_identity::LESZY_AGENT_ID
        );
        assert_eq!(
            crate::agent::agent_identity::canonical_agent_name("leszy"),
            "Leszy"
        );
        let mut config = AgentConfig::default();
        assert!(leszy_persona_prompt(&config).contains("Leszy"));
        config.coder.system_prompt = Some("Prefer small diffs.".to_string());
        assert!(leszy_persona_prompt(&config).ends_with("Prefer small diffs."));
    }
}
