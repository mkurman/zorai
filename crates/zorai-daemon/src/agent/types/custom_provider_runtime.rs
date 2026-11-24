use super::*;
use zorai_shared::providers::*;

pub fn custom_provider_config(id: &str) -> Option<ProviderConfig> {
    let catalog = custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let definition = catalog
        .providers
        .iter()
        .copied()
        .find(|provider| provider.id == id)?;
    let metadata = catalog
        .client_metadata
        .iter()
        .find(|metadata| metadata.id == id)?;
    Some(ProviderConfig {
        presence_penalty: None,
        base_url: definition.default_base_url.to_string(),
        model: definition.default_model.to_string(),
        api_key: metadata.api_key.to_string(),
        assistant_id: String::new(),
        auth_source: metadata.default_auth_source,
        api_transport: definition.default_transport,
        reasoning_effort: default_reasoning_effort(),
        context_window_tokens: definition
            .models
            .iter()
            .find(|model| model.id == definition.default_model)
            .map(|model| model.context_window)
            .unwrap_or_else(default_context_window_tokens),
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
    })
}

pub fn provider_catalog_response() -> ProviderCatalogResponse {
    let custom_provider_report = reload_custom_provider_catalog_from_default_path();
    ProviderCatalogResponse {
        providers: all_provider_definitions()
            .into_iter()
            .map(provider_catalog_entry)
            .collect(),
        custom_provider_report,
    }
}

pub fn provider_catalog_entry(definition: &ProviderDefinition) -> ProviderCatalogEntry {
    let (supported_auth_sources, default_auth_source) =
        provider_auth_sources_for_catalog(definition.id);
    ProviderCatalogEntry {
        id: definition.id.to_string(),
        name: definition.name.to_string(),
        default_base_url: definition.default_base_url.to_string(),
        default_model: definition.default_model.to_string(),
        api_type: definition.api_type,
        auth_method: definition.auth_method,
        models: definition
            .models
            .iter()
            .map(|model| ProviderCatalogModelEntry {
                id: model.id.to_string(),
                name: model.name.to_string(),
                context_window: model.context_window,
                modalities: model.modalities.to_vec(),
            })
            .collect(),
        supports_model_fetch: definition.supports_model_fetch,
        anthropic_base_url: definition.anthropic_base_url.map(str::to_string),
        supported_transports: definition.supported_transports.to_vec(),
        default_transport: definition.default_transport,
        native_transport_kind: definition.native_transport_kind,
        native_base_url: definition.native_base_url.map(str::to_string),
        supports_response_continuity: definition.supports_response_continuity,
        supported_auth_sources,
        default_auth_source,
    }
}

fn provider_auth_sources_for_catalog(provider_id: &str) -> (Vec<AuthSource>, AuthSource) {
    if let Some(metadata) = custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .client_metadata
        .iter()
        .find(|metadata| metadata.id == provider_id)
        .cloned()
    {
        return (
            metadata.supported_auth_sources.to_vec(),
            metadata.default_auth_source,
        );
    }

    match provider_id {
        PROVIDER_ID_OPENAI => (
            vec![AuthSource::ChatgptSubscription, AuthSource::ApiKey],
            AuthSource::ApiKey,
        ),
        PROVIDER_ID_GITHUB_COPILOT => (
            vec![AuthSource::GithubCopilot, AuthSource::ApiKey],
            AuthSource::GithubCopilot,
        ),
        _ => (vec![AuthSource::ApiKey], AuthSource::ApiKey),
    }
}

pub(crate) fn resolve_custom_provider_api_key(
    path: &str,
    provider_id: Option<&str>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    diagnostics: &mut Vec<CustomProviderDiagnostic>,
) -> String {
    let direct = api_key
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if !direct.is_empty() {
        return direct;
    }
    let env_name = api_key_env
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if env_name.is_empty() {
        return String::new();
    }
    match std::env::var(&env_name) {
        Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
        Ok(_) => {
            diagnostics.push(diagnostic(
                path,
                provider_id.map(str::to_string),
                "api_key_env",
                "referenced environment variable is empty",
            ));
            String::new()
        }
        Err(_) => {
            diagnostics.push(diagnostic(
                path,
                provider_id.map(str::to_string),
                "api_key_env",
                "referenced environment variable is not set",
            ));
            String::new()
        }
    }
}
