use super::types::*;
use anyhow::{bail, Result};
use zorai_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderModelSwitch {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    pub api_transport: ApiTransport,
    pub context_window_tokens: u32,
}

fn finalize_resolved_provider(
    provider_id: &str,
    mut resolved: ProviderConfig,
    top_level: &AgentConfig,
) -> ProviderConfig {
    if resolved.reasoning_effort.trim().is_empty() {
        resolved.reasoning_effort = top_level.reasoning_effort.clone();
    }
    if resolved.assistant_id.trim().is_empty() {
        resolved.assistant_id = top_level.assistant_id.clone();
    }
    if resolved.context_window_tokens == 0 {
        resolved.context_window_tokens = top_level.context_window_tokens;
    }
    normalize_provider_transport(provider_id, &mut resolved);
    resolved
}

/// Validate and normalize `cfg.api_transport` for `provider_id`: fall back to the
/// provider default when unsupported, force Responses for ChatGPT-subscription OpenAI,
/// apply any model-fixed transport, and swap in the Anthropic base URL when the
/// resolved transport is Anthropic. Shared by config resolution and per-role overrides.
pub(super) fn normalize_provider_transport(provider_id: &str, cfg: &mut ProviderConfig) {
    if !provider_supports_transport(provider_id, cfg.api_transport) {
        cfg.api_transport = default_api_transport_for_provider(provider_id);
    }
    if provider_id == PROVIDER_ID_OPENAI && cfg.auth_source == AuthSource::ChatgptSubscription {
        cfg.api_transport = ApiTransport::Responses;
    }
    if let Some(fixed_transport) = fixed_api_transport_for_model(provider_id, &cfg.model) {
        cfg.api_transport = fixed_transport;
    }
    if cfg.api_transport == ApiTransport::AnthropicMessages
        && !provider_base_url_is_customized(provider_id, &cfg.base_url)
    {
        if let Some(anthropic_base_url) = get_provider_definition(provider_id)
            .and_then(|definition| definition.anthropic_base_url)
        {
            cfg.base_url = anthropic_base_url.to_string();
        }
    }
}

/// Apply a per-role transport override (e.g. from concierge/weles/subagent config)
/// onto an already-resolved `ProviderConfig`, then re-normalize so model-fixed and
/// unsupported-transport rules still win. A `None` override leaves the config untouched.
pub(super) fn apply_role_transport_override(
    provider_id: &str,
    cfg: &mut ProviderConfig,
    transport_override: Option<ApiTransport>,
) {
    if let Some(transport) = transport_override {
        cfg.api_transport = transport;
        normalize_provider_transport(provider_id, cfg);
    }
}

pub(super) fn apply_provider_model_override(
    provider_id: &str,
    provider_config: &mut ProviderConfig,
    model: &str,
) {
    let model = model.trim();
    if model.is_empty() {
        return;
    }

    provider_config.model = model.to_string();
    if let Some(def) = get_provider_definition(provider_id) {
        if !provider_uses_configurable_base_url(provider_id) {
            let configured_base_url = if provider_config.base_url.trim().is_empty() {
                def.default_base_url
            } else {
                provider_config.base_url.as_str()
            };
            provider_config.base_url =
                get_provider_base_url(provider_id, model, configured_base_url);
        } else if provider_config.base_url.trim().is_empty() {
            provider_config.base_url = get_provider_base_url(provider_id, model, "");
        }

        if let Some(model_def) = def.models.iter().find(|entry| entry.id == model) {
            provider_config.context_window_tokens = model_def.context_window;
        }
    }

    if !provider_supports_transport(provider_id, provider_config.api_transport) {
        provider_config.api_transport = default_api_transport_for_provider(provider_id);
    }
    if provider_id == PROVIDER_ID_OPENAI
        && provider_config.auth_source == AuthSource::ChatgptSubscription
    {
        provider_config.api_transport = ApiTransport::Responses;
    }
    if let Some(fixed_transport) =
        fixed_api_transport_for_model(provider_id, &provider_config.model)
    {
        provider_config.api_transport = fixed_transport;
    }
}

pub(super) fn resolve_provider_config_for(
    config: &AgentConfig,
    provider_id: &str,
    model_override: Option<&str>,
) -> Result<ProviderConfig> {
    let explicit_model_override = model_override
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let requested_model = explicit_model_override.unwrap_or(config.model.trim());
    let active_provider = provider_id == config.provider;

    if let Some(pc) = config.providers.get(provider_id) {
        let mut resolved = pc.clone();
        if let Some(model_override) = explicit_model_override {
            resolved.model = model_override.to_string();
        } else if active_provider {
            if !requested_model.is_empty() {
                resolved.model = requested_model.to_string();
            } else if resolved.model.is_empty() {
                if let Some(def) = get_provider_definition(provider_id) {
                    resolved.model = def.default_model.to_string();
                }
            }
            resolved.base_url = config.base_url.clone();
            resolved.context_window_tokens = config.context_window_tokens;
        } else if resolved.model.is_empty() {
            if !requested_model.is_empty() {
                resolved.model = requested_model.to_string();
            } else if let Some(def) = get_provider_definition(provider_id) {
                resolved.model = def.default_model.to_string();
            }
        }
        if !provider_uses_configurable_base_url(provider_id)
            && !provider_base_url_is_customized(provider_id, &resolved.base_url)
        {
            if let Some(def) = get_provider_definition(provider_id) {
                resolved.base_url =
                    get_provider_base_url(provider_id, &resolved.model, def.default_base_url);
            }
        }
        if resolved.base_url.is_empty() {
            let inherited_base = if provider_id == config.provider {
                config.base_url.as_str()
            } else {
                ""
            };
            resolved.base_url = get_provider_base_url(provider_id, &resolved.model, inherited_base);
        }
        return Ok(finalize_resolved_provider(provider_id, resolved, config));
    }

    reload_custom_provider_catalog_from_default_path();
    if let Some(mut resolved) = custom_provider_config(provider_id) {
        if let Some(model_override) = explicit_model_override {
            apply_provider_model_override(provider_id, &mut resolved, model_override);
        } else if active_provider {
            if !config.model.trim().is_empty() {
                apply_provider_model_override(provider_id, &mut resolved, &config.model);
            }
            if !config.base_url.trim().is_empty() {
                resolved.base_url = config.base_url.clone();
            }
            if !config.api_key.trim().is_empty() {
                resolved.api_key = config.api_key.clone();
            }
            resolved.assistant_id = config.assistant_id.clone();
            resolved.auth_source = config.auth_source;
            resolved.reasoning_effort = config.reasoning_effort.clone();
            resolved.context_window_tokens = config.context_window_tokens;
        }
        return Ok(finalize_resolved_provider(provider_id, resolved, config));
    }

    if provider_id != config.provider && !provider_uses_local_subscription_cli(provider_id) {
        anyhow::bail!(
            "No credentials configured for provider '{}'. Log in via Auth settings.",
            provider_id
        );
    }

    let model = if requested_model.is_empty() {
        get_provider_definition(provider_id)
            .map(|definition| definition.default_model.to_string())
            .unwrap_or_default()
    } else {
        requested_model.to_string()
    };
    let base_url = get_provider_base_url(provider_id, &model, &config.base_url);
    if base_url.is_empty() && !provider_uses_local_subscription_cli(provider_id) {
        anyhow::bail!(
            "No base URL configured for provider '{}'. Configure in agent settings.",
            provider_id
        );
    }

    Ok(finalize_resolved_provider(
        provider_id,
        ProviderConfig {
            base_url,
            model,
            api_key: config.api_key.clone(),
            assistant_id: config.assistant_id.clone(),
            auth_source: config.auth_source,
            api_transport: config.api_transport,
            reasoning_effort: config.reasoning_effort.clone(),
            context_window_tokens: config.context_window_tokens,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
            openrouter_provider_order: Vec::new(),
            openrouter_provider_ignore: Vec::new(),
            openrouter_allow_fallbacks: None,
            openrouter_response_cache_enabled: false,
            huggingface_provider: None,
        },
        config,
    ))
}

pub(super) fn resolve_candidate_provider_config(
    config: &AgentConfig,
    provider_id: &str,
) -> Result<ProviderConfig> {
    let model_override = config
        .providers
        .get(provider_id)
        .filter(|provider| provider.model.trim().is_empty())
        .and_then(|_| {
            get_provider_definition(provider_id).map(|definition| definition.default_model)
        });
    resolve_provider_config_for(config, provider_id, model_override)
}

pub(super) fn resolve_active_provider_config(config: &AgentConfig) -> Result<ProviderConfig> {
    resolve_provider_config_for(config, &config.provider, None)
}

pub(super) fn resolve_provider_model_switch(
    config: &AgentConfig,
    provider_id: &str,
    model: &str,
    allow_unlisted_model: bool,
) -> Result<ProviderModelSwitch> {
    let provider_id = provider_id.trim();
    let model = model.trim();

    if provider_id.is_empty() {
        bail!("provider id cannot be empty");
    }
    if model.is_empty() {
        bail!("model cannot be empty");
    }

    let mut base_url = config.base_url.clone();
    let mut api_transport = config.api_transport;
    let mut context_window_tokens = config.context_window_tokens;

    if provider_uses_configurable_base_url(provider_id) {
        if base_url.trim().is_empty() {
            bail!("base URL cannot be empty for provider '{provider_id}'");
        }
    } else {
        let def = get_provider_definition(provider_id)
            .ok_or_else(|| anyhow::anyhow!("unknown provider '{provider_id}'"))?;
        if !allow_unlisted_model
            && !provider_allows_unlisted_models(provider_id)
            && !def.models.is_empty()
            && !def.models.iter().any(|entry| entry.id == model)
        {
            bail!(
                "model '{}' is not available for provider '{}'",
                model,
                provider_id
            );
        }
        let keeps_active_custom_base_url = provider_id == config.provider
            && provider_base_url_is_customized(provider_id, &base_url);
        if !keeps_active_custom_base_url {
            base_url = config
                .providers
                .get(provider_id)
                .map(|provider_config| provider_config.base_url.clone())
                .filter(|url| provider_base_url_is_customized(provider_id, url))
                .unwrap_or_else(|| get_provider_base_url(provider_id, model, def.default_base_url));
        }
        api_transport = if provider_supports_transport(provider_id, config.api_transport) {
            config.api_transport
        } else {
            def.default_transport
        };
        if let Some(fixed_transport) = fixed_api_transport_for_model(provider_id, model) {
            api_transport = fixed_transport;
        }
        if let Some(model_def) = def.models.iter().find(|entry| entry.id == model) {
            context_window_tokens = model_def.context_window;
        }
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI {
        if !super::llm_client::claude_cli_available() {
            bail!(
                "Claude Code CLI not found on PATH. Install Claude Code to use provider '{}'.",
                provider_id
            );
        }
    } else if provider_id == zorai_shared::providers::PROVIDER_ID_CURSOR {
        if !super::cursor_auth::cursor_cli_available() {
            bail!(
                "Cursor CLI not found on PATH. Install the Cursor CLI and run `agent login` to use provider '{}'.",
                provider_id
            );
        }
    } else {
        match config.auth_source {
            AuthSource::ApiKey => {
                if config.api_key.trim().is_empty() {
                    bail!(
                        "API key is required to switch provider '{}' to model '{}'",
                        provider_id,
                        model
                    );
                }
            }
            AuthSource::ChatgptSubscription => {
                if provider_id != PROVIDER_ID_OPENAI
                    || !super::llm_client::has_openai_chatgpt_subscription_auth()
                {
                    bail!(
                        "ChatGPT subscription auth is not available for provider '{}'",
                        provider_id
                    );
                }
            }
            AuthSource::GithubCopilot => {
                if provider_id != PROVIDER_ID_GITHUB_COPILOT
                    || !super::copilot_auth::github_copilot_has_available_models(
                        &config.api_key,
                        config.auth_source,
                    )
                {
                    bail!(
                        "GitHub Copilot auth is not available for provider '{}'",
                        provider_id
                    );
                }
            }
        }
    }

    Ok(ProviderModelSwitch {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        base_url,
        api_transport,
        context_window_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorai_shared::providers::{
        PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_GITHUB_COPILOT,
        PROVIDER_ID_GROQ, PROVIDER_ID_OPENAI, PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
    };

    #[test]
    fn resolves_custom_provider_from_custom_auth_api_key_when_not_saved_in_config() {
        let _lock = crate::test_support::env_test_lock();
        let _guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
        std::fs::write(
            &custom_auth_path,
            r#"
providers:
  - id: local-openai
    name: Local OpenAI-Compatible
    default_base_url: http://127.0.0.1:11434/v1
    default_model: llama3.3
    api_key: local-secret
    supported_transports: [chat_completions]
    default_transport: chat_completions
    models:
      - id: llama3.3
        context_window: 128000
"#,
        )
        .expect("write custom auth");
        std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);
        reload_custom_provider_catalog_from_default_path();

        let config = AgentConfig::default();
        let resolved = resolve_provider_config_for(&config, "local-openai", None)
            .expect("custom auth provider should resolve");

        assert_eq!(resolved.base_url, "http://127.0.0.1:11434/v1");
        assert_eq!(resolved.model, "llama3.3");
        assert_eq!(resolved.api_key, "local-secret");
        assert_eq!(resolved.api_transport, ApiTransport::ChatCompletions);
    }

    #[test]
    fn claude_code_cli_resolves_without_api_key_or_base_url() {
        use zorai_shared::providers::PROVIDER_ID_CLAUDE_CODE_CLI;
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_CLAUDE_CODE_CLI.to_string();
        config.model = "opus".to_string();
        config.api_key = String::new();
        config.base_url = String::new();

        let resolved = resolve_provider_config_for(&config, PROVIDER_ID_CLAUDE_CODE_CLI, None)
            .expect("claude-code-cli must resolve with no pasted key and no base URL");
        assert_eq!(resolved.api_transport, ApiTransport::NativeAssistant);
        assert!(
            resolved.base_url.is_empty(),
            "the local CLI provider has no HTTP base URL by design"
        );
    }

    #[test]
    fn cursor_subscription_resolves_without_api_key_or_base_url() {
        use zorai_shared::providers::PROVIDER_ID_CURSOR;
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_CURSOR.to_string();
        config.model = "auto".to_string();
        config.api_key = String::new();
        config.base_url = String::new();

        let resolved = resolve_provider_config_for(&config, PROVIDER_ID_CURSOR, None)
            .expect("cursor subscription must resolve with no pasted key and no base URL");
        assert_eq!(resolved.api_transport, ApiTransport::NativeAssistant);
        assert_eq!(resolved.model, "auto");
        assert!(resolved.base_url.is_empty());
    }

    #[test]
    fn role_transport_override_selects_anthropic_on_opencode_provider() {
        use zorai_shared::providers::PROVIDER_ID_OPENCODE_GO;
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENCODE_GO.to_string();
        config.model = "glm-5.1".to_string();
        let mut resolved = resolve_provider_config_for(&config, PROVIDER_ID_OPENCODE_GO, None)
            .expect("opencode-go resolves");
        // Provider default lands on chat_completions (responses is unsupported here).
        assert_eq!(resolved.api_transport, ApiTransport::ChatCompletions);
        // An explicit, supported anthropic override sticks.
        apply_role_transport_override(
            PROVIDER_ID_OPENCODE_GO,
            &mut resolved,
            Some(ApiTransport::AnthropicMessages),
        );
        assert_eq!(resolved.api_transport, ApiTransport::AnthropicMessages);
        // A `None` override preserves the already-resolved transport.
        apply_role_transport_override(PROVIDER_ID_OPENCODE_GO, &mut resolved, None);
        assert_eq!(resolved.api_transport, ApiTransport::AnthropicMessages);
        // An unsupported override falls back to the provider default.
        apply_role_transport_override(
            PROVIDER_ID_OPENCODE_GO,
            &mut resolved,
            Some(ApiTransport::Responses),
        );
        assert_eq!(
            resolved.api_transport,
            default_api_transport_for_provider(PROVIDER_ID_OPENCODE_GO)
        );
    }

    #[test]
    fn named_provider_inherits_defaults_and_transport_rules() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.reasoning_effort = "high".to_string();
        config.context_window_tokens = 99_999;
        config.assistant_id = "assistant-root".to_string();
        config.providers.insert(
            PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: String::new(),
                api_key: "dashscope-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            PROVIDER_ID_ALIBABA_CODING_PLAN,
            Some("qwen3.6-plus"),
        )
        .expect("provider should resolve");

        assert_eq!(
            resolved.base_url,
            "https://coding-intl.dashscope.aliyuncs.com/v1"
        );
        assert_eq!(resolved.model, "qwen3.6-plus");
        assert_eq!(resolved.api_key, "dashscope-key");
        assert_eq!(resolved.reasoning_effort, "high");
        assert_eq!(resolved.assistant_id, "assistant-root");
        assert_eq!(resolved.context_window_tokens, 99_999);
        assert_eq!(resolved.api_transport, ApiTransport::ChatCompletions);
    }

    #[test]
    fn xiaomi_anthropic_transport_uses_anthropic_base_url() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.providers.insert(
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: "mimo-v2.5-pro".to_string(),
                api_key: "mimo-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::AnthropicMessages,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
            Some("mimo-v2.5-pro"),
        )
        .expect("provider should resolve");

        assert_eq!(resolved.api_transport, ApiTransport::AnthropicMessages);
        assert_eq!(
            resolved.base_url,
            "https://token-plan-ams.xiaomimimo.com/anthropic"
        );
    }

    #[test]
    fn named_provider_keeps_user_customized_base_url() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.providers.insert(
            zorai_shared::providers::PROVIDER_ID_DEEPSEEK.to_string(),
            ProviderConfig {
                base_url: "https://proxy.example.com/v1".to_string(),
                model: "deepseek-v4-pro".to_string(),
                api_key: "deepseek-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::ChatCompletions,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            zorai_shared::providers::PROVIDER_ID_DEEPSEEK,
            Some("deepseek-v4-pro"),
        )
        .expect("provider should resolve");

        assert_eq!(resolved.base_url, "https://proxy.example.com/v1");
    }

    #[test]
    fn anthropic_transport_keeps_user_customized_base_url() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.providers.insert(
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN.to_string(),
            ProviderConfig {
                base_url: "https://mimo-proxy.example.com/anthropic".to_string(),
                model: "mimo-v2.5-pro".to_string(),
                api_key: "mimo-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::AnthropicMessages,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
            Some("mimo-v2.5-pro"),
        )
        .expect("provider should resolve");

        assert_eq!(resolved.api_transport, ApiTransport::AnthropicMessages);
        assert_eq!(
            resolved.base_url,
            "https://mimo-proxy.example.com/anthropic"
        );
    }

    #[test]
    fn explicit_model_override_wins_over_stored_provider_model() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.providers.insert(
            PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: "MiniMax-M2.5".to_string(),
                api_key: "dashscope-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            PROVIDER_ID_ALIBABA_CODING_PLAN,
            Some("qwen3.6-plus"),
        )
        .expect("provider should resolve");

        assert_eq!(resolved.model, "qwen3.6-plus");
    }

    #[test]
    fn non_active_provider_without_named_credentials_fails() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        let err = resolve_provider_config_for(&config, PROVIDER_ID_GROQ, None).unwrap_err();
        assert!(err
            .to_string()
            .contains("No credentials configured for provider 'groq'"));
    }

    #[test]
    fn candidate_provider_with_empty_model_uses_its_own_default_model() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.model = "gpt-5.4".to_string();
        config.providers.insert(
            PROVIDER_ID_GROQ.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: String::new(),
                api_key: "groq-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_candidate_provider_config(&config, PROVIDER_ID_GROQ)
            .expect("candidate should resolve");

        assert_eq!(resolved.model, "llama-3.3-70b-versatile");
        assert_eq!(resolved.base_url, "https://api.groq.com/openai/v1");
    }

    #[test]
    fn active_provider_resolution_ignores_stale_named_provider_model_and_window() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.context_window_tokens = 128_000;
        config.api_key = "top-level-key".to_string();
        config.providers.insert(
            PROVIDER_ID_OPENAI.to_string(),
            ProviderConfig {
                base_url: "https://stale.invalid/v1".to_string(),
                model: "gpt-5.4".to_string(),
                api_key: "stored-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 400_000,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_active_provider_config(&config).expect("active provider resolves");

        assert_eq!(resolved.model, "gpt-5.4-mini");
        assert_eq!(resolved.context_window_tokens, 128_000);
        assert_eq!(resolved.base_url, "https://api.openai.com/v1");
        assert_eq!(resolved.api_key, "stored-key");
    }

    #[test]
    fn active_provider_resolution_keeps_nested_auth_source_and_transport() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4-mini".to_string();
        config.context_window_tokens = 128_000;
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.providers.insert(
            PROVIDER_ID_OPENAI.to_string(),
            ProviderConfig {
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-5.4".to_string(),
                api_key: String::new(),
                assistant_id: String::new(),
                auth_source: AuthSource::ChatgptSubscription,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 400_000,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_active_provider_config(&config).expect("active provider resolves");

        assert_eq!(resolved.model, "gpt-5.4-mini");
        assert_eq!(resolved.base_url, "https://api.openai.com/v1");
        assert_eq!(resolved.context_window_tokens, 128_000);
        assert_eq!(resolved.auth_source, AuthSource::ChatgptSubscription);
        assert_eq!(resolved.api_transport, ApiTransport::Responses);
    }

    #[test]
    fn azure_openai_keeps_user_configured_base_url() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_AZURE_OPENAI.to_string();
        config.base_url = "https://my-resource.openai.azure.com/openai/v1".to_string();
        config.model = "my-deployment".to_string();
        config.api_key = "azure-key".to_string();
        config.providers.insert(
            PROVIDER_ID_AZURE_OPENAI.to_string(),
            ProviderConfig {
                base_url: "https://my-resource.openai.azure.com/openai/v1".to_string(),
                model: "my-deployment".to_string(),
                api_key: "azure-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 128_000,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_active_provider_config(&config).expect("azure provider resolves");

        assert_eq!(
            resolved.base_url,
            "https://my-resource.openai.azure.com/openai/v1"
        );
        assert_eq!(resolved.model, "my-deployment");
    }

    #[test]
    fn github_copilot_gemini_31_forces_chat_completions_transport() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
        config.base_url = "https://api.githubcopilot.com".to_string();
        config.model = "gemini-3.1-pro-preview".to_string();
        config.auth_source = AuthSource::GithubCopilot;
        config.api_transport = ApiTransport::Responses;
        config.providers.insert(
            PROVIDER_ID_GITHUB_COPILOT.to_string(),
            ProviderConfig {
                base_url: "https://api.githubcopilot.com".to_string(),
                model: "gemini-3.1-pro-preview".to_string(),
                api_key: String::new(),
                assistant_id: String::new(),
                auth_source: AuthSource::GithubCopilot,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 173_000,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
                openrouter_provider_order: Vec::new(),
                openrouter_provider_ignore: Vec::new(),
                openrouter_allow_fallbacks: None,
                openrouter_response_cache_enabled: false,
                huggingface_provider: None,
            },
        );

        let resolved = resolve_active_provider_config(&config).expect("copilot provider resolves");

        assert_eq!(resolved.model, "gemini-3.1-pro-preview");
        assert_eq!(resolved.api_transport, ApiTransport::ChatCompletions);
    }
}
