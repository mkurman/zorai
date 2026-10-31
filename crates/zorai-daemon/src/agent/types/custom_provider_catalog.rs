use super::*;
use serde::{Deserialize, Serialize};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomProviderDiagnostic {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CustomProviderLoadReport {
    pub path: String,
    pub loaded_provider_count: usize,
    #[serde(default)]
    pub diagnostics: Vec<CustomProviderDiagnostic>,
}

#[derive(Debug, Default)]
pub(crate) struct CustomProviderCatalog {
    pub(crate) path: String,
    pub(crate) providers: Vec<&'static ProviderDefinition>,
    pub(crate) client_metadata: Vec<CustomProviderClientMetadata>,
    pub(crate) diagnostics: Vec<CustomProviderDiagnostic>,
}

#[derive(Debug, Clone)]
pub(crate) struct CustomProviderClientMetadata {
    pub(crate) id: &'static str,
    pub(crate) supported_auth_sources: &'static [AuthSource],
    pub(crate) default_auth_source: AuthSource,
    pub(crate) api_key: &'static str,
    pub(crate) models_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCatalogModelEntry {
    pub id: String,
    pub name: String,
    pub context_window: u32,
    pub modalities: Vec<Modality>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCatalogEntry {
    pub id: String,
    pub name: String,
    pub default_base_url: String,
    pub default_model: String,
    pub api_type: ApiType,
    pub auth_method: AuthMethod,
    pub models: Vec<ProviderCatalogModelEntry>,
    pub supports_model_fetch: bool,
    pub anthropic_base_url: Option<String>,
    pub supported_transports: Vec<ApiTransport>,
    pub default_transport: ApiTransport,
    pub native_transport_kind: Option<NativeTransportKind>,
    pub native_base_url: Option<String>,
    pub supports_response_continuity: bool,
    pub supported_auth_sources: Vec<AuthSource>,
    pub default_auth_source: AuthSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderCatalogResponse {
    pub providers: Vec<ProviderCatalogEntry>,
    pub custom_provider_report: CustomProviderLoadReport,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CustomAuthDocument {
    Root { providers: Vec<CustomProviderYaml> },
    Providers(Vec<CustomProviderYaml>),
}

#[derive(Debug, Deserialize)]
struct CustomProviderYaml {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) default_base_url: Option<String>,
    pub(crate) default_model: Option<String>,
    #[serde(default)]
    pub(crate) api_type: Option<ApiType>,
    #[serde(default)]
    pub(crate) auth_method: Option<AuthMethod>,
    #[serde(default)]
    pub(crate) models: Vec<CustomModelYaml>,
    #[serde(default)]
    pub(crate) supports_model_fetch: bool,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub(crate) models_url: Option<String>,
    #[serde(default)]
    pub(crate) anthropic_base_url: Option<String>,
    #[serde(default)]
    pub(crate) supported_transports: Vec<ApiTransport>,
    #[serde(default)]
    pub(crate) default_transport: Option<ApiTransport>,
    #[serde(default)]
    pub(crate) native_transport_kind: Option<NativeTransportKind>,
    #[serde(default)]
    pub(crate) native_base_url: Option<String>,
    #[serde(default)]
    pub(crate) supports_response_continuity: bool,
    #[serde(default)]
    pub(crate) supported_auth_sources: Vec<AuthSource>,
    #[serde(default)]
    pub(crate) default_auth_source: Option<AuthSource>,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub(crate) api_key: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub(crate) api_key_env: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CustomModelYaml {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) context_window: Option<u32>,
    #[serde(default)]
    pub(crate) modalities: Vec<Modality>,
}

pub(crate) fn custom_provider_catalog_cell() -> &'static RwLock<CustomProviderCatalog> {
    static CATALOG: OnceLock<RwLock<CustomProviderCatalog>> = OnceLock::new();
    CATALOG.get_or_init(|| RwLock::new(CustomProviderCatalog::default()))
}

pub fn custom_auth_path() -> PathBuf {
    std::env::var_os("ZORAI_CUSTOM_AUTH_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| zorai_protocol::zorai_root_dir().join("custom-auth.yaml"))
}

pub fn reload_custom_provider_catalog_from_default_path() -> CustomProviderLoadReport {
    reload_custom_provider_catalog_from_path(&custom_auth_path())
}

pub fn reload_custom_provider_catalog_from_path(path: &Path) -> CustomProviderLoadReport {
    let path_display = path.display().to_string();
    let mut diagnostics = Vec::new();
    let (providers, client_metadata) = match std::fs::read_to_string(path) {
        Ok(raw) => match serde_yaml::from_str::<CustomAuthDocument>(&raw) {
            Ok(CustomAuthDocument::Root { providers })
            | Ok(CustomAuthDocument::Providers(providers)) => {
                hydrate_custom_providers(&path_display, providers, &mut diagnostics)
            }
            Err(error) => {
                diagnostics.push(CustomProviderDiagnostic {
                    path: path_display.clone(),
                    provider_id: None,
                    field: None,
                    message: format!("failed to parse custom-auth.yaml: {error}"),
                });
                (Vec::new(), Vec::new())
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), Vec::new()),
        Err(error) => {
            diagnostics.push(CustomProviderDiagnostic {
                path: path_display.clone(),
                provider_id: None,
                field: None,
                message: format!("failed to read custom-auth.yaml: {error}"),
            });
            (Vec::new(), Vec::new())
        }
    };

    let report = CustomProviderLoadReport {
        path: path_display.clone(),
        loaded_provider_count: providers.len(),
        diagnostics: diagnostics.clone(),
    };
    let mut catalog = custom_provider_catalog_cell()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *catalog = CustomProviderCatalog {
        path: path_display,
        providers,
        client_metadata,
        diagnostics,
    };
    report
}

pub fn custom_provider_definition(id: &str) -> Option<&'static ProviderDefinition> {
    custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .providers
        .iter()
        .copied()
        .find(|provider| provider.id == id)
}

pub fn custom_provider_definitions() -> Vec<&'static ProviderDefinition> {
    custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .providers
        .clone()
}

pub fn custom_provider_models_url(id: &str) -> Option<String> {
    custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .client_metadata
        .iter()
        .find(|metadata| metadata.id == id)
        .and_then(|metadata| metadata.models_url)
        .map(str::to_string)
}

pub fn custom_provider_load_report() -> CustomProviderLoadReport {
    let catalog = custom_provider_catalog_cell()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    CustomProviderLoadReport {
        path: catalog.path.clone(),
        loaded_provider_count: catalog.providers.len(),
        diagnostics: catalog.diagnostics.clone(),
    }
}

fn hydrate_custom_providers(
    path: &str,
    providers: Vec<CustomProviderYaml>,
    diagnostics: &mut Vec<CustomProviderDiagnostic>,
) -> (
    Vec<&'static ProviderDefinition>,
    Vec<CustomProviderClientMetadata>,
) {
    let mut seen = HashSet::new();
    let mut hydrated_providers = Vec::new();
    let mut metadata = Vec::new();
    for provider in providers {
        if let Some((definition, client_metadata)) =
            hydrate_custom_provider(path, provider, &mut seen, diagnostics)
        {
            hydrated_providers.push(definition);
            metadata.push(client_metadata);
        }
    }
    (hydrated_providers, metadata)
}

fn hydrate_custom_provider(
    path: &str,
    provider: CustomProviderYaml,
    seen: &mut HashSet<String>,
    diagnostics: &mut Vec<CustomProviderDiagnostic>,
) -> Option<(&'static ProviderDefinition, CustomProviderClientMetadata)> {
    let id = required_string(path, None, "id", provider.id, diagnostics)?;
    let provider_id = Some(id.clone());
    if PROVIDER_DEFINITIONS
        .iter()
        .any(|built_in| built_in.id == id)
    {
        diagnostics.push(diagnostic(
            path,
            provider_id,
            "id",
            "custom provider id conflicts with a built-in provider id",
        ));
        return None;
    }
    if !seen.insert(id.clone()) {
        diagnostics.push(diagnostic(
            path,
            Some(id),
            "id",
            "custom provider id is duplicated in custom-auth.yaml",
        ));
        return None;
    }

    let name = required_string(
        path,
        provider_id.as_deref(),
        "name",
        provider.name,
        diagnostics,
    )?;
    let default_base_url = required_string(
        path,
        provider_id.as_deref(),
        "default_base_url",
        provider.default_base_url,
        diagnostics,
    )?;
    let default_model = required_string(
        path,
        provider_id.as_deref(),
        "default_model",
        provider.default_model,
        diagnostics,
    )?;
    let mut supported_transports = provider.supported_transports;
    if supported_transports.is_empty() {
        supported_transports.push(ApiTransport::ChatCompletions);
    }
    let default_transport = provider
        .default_transport
        .unwrap_or_else(|| supported_transports[0]);
    if !supported_transports.contains(&default_transport) {
        diagnostics.push(diagnostic(
            path,
            provider_id.clone(),
            "default_transport",
            "default_transport must be included in supported_transports",
        ));
        return None;
    }
    let mut supported_auth_sources = provider.supported_auth_sources;
    if supported_auth_sources.is_empty() {
        supported_auth_sources.push(AuthSource::ApiKey);
    }
    let default_auth_source = provider
        .default_auth_source
        .unwrap_or(supported_auth_sources[0]);
    if !supported_auth_sources.contains(&default_auth_source) {
        diagnostics.push(diagnostic(
            path,
            provider_id.clone(),
            "default_auth_source",
            "default_auth_source must be included in supported_auth_sources",
        ));
        return None;
    }

    let models = hydrate_custom_models(path, &id, provider.models, diagnostics)?;
    let models_url = provider
        .models_url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let leaked_id = leak_str(id);
    let definition = leak_provider_definition(ProviderDefinition {
        id: leaked_id,
        name: leak_str(name),
        default_base_url: leak_str(default_base_url),
        default_model: leak_str(default_model),
        api_type: provider.api_type.unwrap_or(ApiType::OpenAI),
        auth_method: provider.auth_method.unwrap_or(AuthMethod::Bearer),
        models,
        supports_model_fetch: provider.supports_model_fetch || models_url.is_some(),
        anthropic_base_url: provider.anthropic_base_url.map(leak_str),
        supported_transports: leak_vec(supported_transports),
        default_transport,
        native_transport_kind: provider.native_transport_kind,
        native_base_url: provider.native_base_url.map(leak_str),
        supports_response_continuity: provider.supports_response_continuity,
    });
    let client_metadata = CustomProviderClientMetadata {
        id: leaked_id,
        supported_auth_sources: leak_vec(supported_auth_sources),
        default_auth_source,
        api_key: leak_str(resolve_custom_provider_api_key(
            path,
            provider_id.as_deref(),
            provider.api_key,
            provider.api_key_env,
            diagnostics,
        )),
        models_url: models_url.map(leak_str),
    };

    Some((definition, client_metadata))
}

fn hydrate_custom_models(
    path: &str,
    provider_id: &str,
    models: Vec<CustomModelYaml>,
    diagnostics: &mut Vec<CustomProviderDiagnostic>,
) -> Option<&'static [ModelDefinition]> {
    let provider_id_option = Some(provider_id);
    let mut seen = HashSet::new();
    let mut hydrated = Vec::new();
    for model in models {
        let id = required_string(
            path,
            provider_id_option,
            "models[].id",
            model.id,
            diagnostics,
        )?;
        if !seen.insert(id.clone()) {
            diagnostics.push(diagnostic(
                path,
                Some(provider_id.to_string()),
                "models[].id",
                "model id is duplicated for provider",
            ));
            return None;
        }
        let name = model.name.unwrap_or_else(|| id.clone());
        let Some(context_window) = model.context_window.filter(|value| *value > 0) else {
            diagnostics.push(diagnostic(
                path,
                Some(provider_id.to_string()),
                "models[].context_window",
                "context_window must be a positive integer",
            ));
            return None;
        };
        let modalities = if model.modalities.is_empty() {
            vec![Modality::Text]
        } else {
            model.modalities
        };
        hydrated.push(ModelDefinition {
            id: leak_str(id),
            name: leak_str(name),
            context_window,
            modalities: leak_vec(modalities),
        });
    }
    Some(leak_vec(hydrated))
}

fn required_string(
    path: &str,
    provider_id: Option<&str>,
    field: &str,
    value: Option<String>,
    diagnostics: &mut Vec<CustomProviderDiagnostic>,
) -> Option<String> {
    let value = value
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if value.is_empty() {
        diagnostics.push(diagnostic(
            path,
            provider_id.map(str::to_string),
            field,
            "field is required and must not be empty",
        ));
        None
    } else {
        Some(value)
    }
}

fn deserialize_optional_scalar_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_yaml::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        serde_yaml::Value::Null => Ok(None),
        serde_yaml::Value::String(value) => Ok(Some(value)),
        serde_yaml::Value::Number(value) => Ok(Some(value.to_string())),
        serde_yaml::Value::Bool(value) => Ok(Some(value.to_string())),
        _ => Err(serde::de::Error::custom(
            "expected a scalar string, number, or boolean value",
        )),
    }
}

pub(crate) fn diagnostic(
    path: &str,
    provider_id: Option<String>,
    field: &str,
    message: &str,
) -> CustomProviderDiagnostic {
    CustomProviderDiagnostic {
        path: path.to_string(),
        provider_id,
        field: Some(field.to_string()),
        message: message.to_string(),
    }
}

fn leak_str(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn leak_vec<T>(value: Vec<T>) -> &'static [T] {
    Box::leak(value.into_boxed_slice())
}

fn leak_provider_definition(value: ProviderDefinition) -> &'static ProviderDefinition {
    Box::leak(Box::new(value))
}
