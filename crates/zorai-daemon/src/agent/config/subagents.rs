use super::*;

fn optional_non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn apply_builtin_persona_overrides_from_sub_agent(
    config: &mut AgentConfig,
    def: &SubAgentDefinition,
) -> Result<()> {
    let alias = if is_explicit_builtin_persona_scope(&def.id) {
        def.id.as_str()
    } else if is_explicit_builtin_persona_scope(&def.name) {
        def.name.as_str()
    } else {
        return Err(protected_mutation_error(
            "unexpected built-in persona target",
        ));
    };
    let Some(overrides) = builtin_persona_overrides_mut(config, alias) else {
        return Err(protected_mutation_error(format!(
            "unknown built-in persona '{alias}'"
        )));
    };
    if def.provider.trim().is_empty() {
        anyhow::bail!("provider cannot be empty");
    }
    if def.model.trim().is_empty() {
        anyhow::bail!("model cannot be empty");
    }

    overrides.provider = Some(def.provider.trim().to_string());
    overrides.model = Some(def.model.trim().to_string());
    overrides.reasoning_effort = optional_non_empty(def.reasoning_effort.as_deref());
    overrides.context_window_tokens = def.context_window_tokens;
    overrides.openrouter_provider_order = def.openrouter_provider_order.clone();
    overrides.openrouter_provider_ignore = def.openrouter_provider_ignore.clone();
    overrides.openrouter_allow_fallbacks = def.openrouter_allow_fallbacks;
    overrides.huggingface_provider = optional_non_empty(def.huggingface_provider.as_deref());
    Ok(())
}

pub(super) fn apply_weles_allowed_overrides(
    config: &mut AgentConfig,
    def: &SubAgentDefinition,
) -> Result<()> {
    if !is_weles_builtin_target(def) {
        return Err(protected_mutation_error(
            "unexpected built-in sub-agent target",
        ));
    }
    if def.name != WELES_BUILTIN_NAME {
        return Err(protected_mutation_error("cannot change WELES name"));
    }
    if !def.enabled {
        return Err(protected_mutation_error(
            "cannot disable daemon-owned WELES",
        ));
    }
    if !def.builtin || !def.immutable_identity || def.disable_allowed || def.delete_allowed {
        return Err(protected_mutation_error(
            "cannot change WELES built-in protection metadata",
        ));
    }
    if def.protected_reason.as_deref() != Some(WELES_PROTECTED_REASON) {
        return Err(protected_mutation_error(
            "cannot change WELES protected reason",
        ));
    }

    let inherited_provider = resolve_main_agent_default(None, &config.provider);
    let inherited_model = resolve_main_agent_default(None, &config.model);
    let inherited_role = Some("governance".to_string());
    let inherited_system_prompt = Some(resolve_main_agent_default(None, &config.system_prompt));
    let inherited_tool_whitelist = None::<Vec<String>>;
    let inherited_tool_blacklist = Some(default_weles_tool_blacklist());
    let inherited_context_budget_tokens = None::<u32>;
    let inherited_context_window_tokens = None::<u32>;
    let inherited_max_duration_secs = None::<u64>;
    let inherited_supervisor_config = None::<SupervisorConfig>;
    let inherited_reasoning_effort = Some("medium".to_string());

    let system_prompt = sanitize_weles_operator_system_prompt(
        if def.system_prompt == inherited_system_prompt {
            None
        } else {
            def.system_prompt.clone()
        },
        &config.system_prompt,
    );

    config.builtin_sub_agents.weles = WelesBuiltinOverrides {
        provider: if def.provider == inherited_provider {
            None
        } else {
            Some(def.provider.clone()).filter(|value| !value.trim().is_empty())
        },
        model: if def.model == inherited_model {
            None
        } else {
            Some(def.model.clone()).filter(|value| !value.trim().is_empty())
        },
        role: if def.role == inherited_role {
            None
        } else {
            def.role.clone()
        },
        system_prompt,
        tool_whitelist: if def.tool_whitelist == inherited_tool_whitelist {
            None
        } else {
            def.tool_whitelist.clone()
        },
        tool_blacklist: if def.tool_blacklist == inherited_tool_blacklist {
            None
        } else {
            def.tool_blacklist.clone()
        },
        context_budget_tokens: if def.context_budget_tokens == inherited_context_budget_tokens {
            None
        } else {
            def.context_budget_tokens
        },
        context_window_tokens: if def.context_window_tokens == inherited_context_window_tokens {
            None
        } else {
            def.context_window_tokens
        },
        max_duration_secs: if def.max_duration_secs == inherited_max_duration_secs {
            None
        } else {
            def.max_duration_secs
        },
        supervisor_config: if serde_json::to_value(&def.supervisor_config).ok()
            == serde_json::to_value(&inherited_supervisor_config).ok()
        {
            None
        } else {
            def.supervisor_config.clone()
        },
        reasoning_effort: if def.reasoning_effort == inherited_reasoning_effort {
            None
        } else {
            def.reasoning_effort.clone()
        },
        openrouter_provider_order: if def.openrouter_provider_order.is_empty() {
            Vec::new()
        } else {
            def.openrouter_provider_order.clone()
        },
        openrouter_provider_ignore: if def.openrouter_provider_ignore.is_empty() {
            Vec::new()
        } else {
            def.openrouter_provider_ignore.clone()
        },
        openrouter_allow_fallbacks: def.openrouter_allow_fallbacks,
        huggingface_provider: def.huggingface_provider.clone(),
        max_concurrent_reviews: None,
        api_transport: def.api_transport,
    };
    Ok(())
}

pub(super) fn filter_user_sub_agents_and_collect_collisions(
    config: &AgentConfig,
) -> (Vec<SubAgentDefinition>, Vec<SubAgentDefinition>) {
    let mut filtered = Vec::new();
    let mut collisions = Vec::new();
    for def in &config.sub_agents {
        if is_reserved_builtin_sub_agent_id(&def.id)
            || is_reserved_builtin_sub_agent_name(&def.name)
        {
            collisions.push(def.clone());
        } else {
            filtered.push(def.clone());
        }
    }
    (filtered, collisions)
}

pub(crate) fn effective_sub_agents_from_config(
    config: &AgentConfig,
) -> (Vec<SubAgentDefinition>, Vec<SubAgentDefinition>) {
    let (user_sub_agents, collisions) = filter_user_sub_agents_and_collect_collisions(config);
    let mut effective = user_sub_agents;
    effective.push(build_effective_weles_definition(config));
    effective.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    (effective, collisions)
}

pub(super) fn sanitize_weles_builtin_overrides(root: &mut serde_json::Map<String, Value>) {
    let Some(Value::Object(builtin_sub_agents)) = root.get_mut("builtin_sub_agents") else {
        return;
    };
    let Some(Value::Object(weles)) = builtin_sub_agents.get_mut("weles") else {
        return;
    };

    for forbidden_key in [
        "enabled",
        "builtin",
        "immutable_identity",
        "disable_allowed",
        "delete_allowed",
        "protected_reason",
        "tool_name",
        "tool_args",
        "security_level",
        "suspicion_reasons",
        "task_metadata",
    ] {
        weles.remove(forbidden_key);
    }

    if weles
        .get("role")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "governance")
    {
        weles.remove("role");
    }

    if let Some(Value::String(system_prompt)) = weles.get_mut("system_prompt") {
        let sanitized =
            crate::agent::weles_governance::strip_weles_internal_payload_markers(system_prompt);
        *system_prompt = sanitized;
    }
}

pub(in crate::agent) fn sanitize_weles_collisions_from_config(
    config: &mut AgentConfig,
) -> Vec<SubAgentDefinition> {
    let (filtered, collisions) = filter_user_sub_agents_and_collect_collisions(config);
    config.sub_agents = filtered;
    collisions
}

pub(in crate::agent) fn config_to_items(config: &AgentConfig) -> Vec<(String, Value)> {
    let mut value =
        serde_json::to_value(config).unwrap_or_else(|_| Value::Object(Default::default()));
    normalize_config_keys_to_snake_case(&mut value);
    sanitize_config_value(&mut value);
    let mut items = Vec::new();
    flatten_config_value_to_items(&value, "", &mut items);
    items
}
