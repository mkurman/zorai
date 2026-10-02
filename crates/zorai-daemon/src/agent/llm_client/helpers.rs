use super::*;
pub(crate) struct PendingToolCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
}

pub(crate) fn synthesize_tool_call_id(seed: &str, index: usize, name: &str) -> String {
    let clean_name: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("synthetic_tool_call_{seed}_{index}_{clean_name}")
}

pub(crate) fn drain_tool_calls(map: &mut HashMap<u32, PendingToolCall>) -> Vec<ToolCall> {
    let mut entries: Vec<(u32, PendingToolCall)> = map.drain().collect();
    entries.sort_by_key(|(idx, _)| *idx);
    entries
        .into_iter()
        .map(|(idx, tc)| {
            ToolCall::with_default_weles_review(
                if tc.id.trim().is_empty() {
                    synthesize_tool_call_id(
                        zorai_shared::providers::PROVIDER_ID_OPENAI,
                        idx as usize,
                        &tc.name,
                    )
                } else {
                    tc.id
                },
                ToolFunction {
                    name: tc.name,
                    arguments: tc.arguments,
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FetchedModelPricing {
    pub prompt: Option<String>,
    pub completion: Option<String>,
    pub image: Option<String>,
    pub request: Option<String>,
    pub web_search: Option<String>,
    pub internal_reasoning: Option<String>,
    pub input_cache_read: Option<String>,
    pub input_cache_write: Option<String>,
    pub audio: Option<String>,
}

impl FetchedModelPricing {
    fn is_empty(&self) -> bool {
        self.prompt.is_none()
            && self.completion.is_none()
            && self.image.is_none()
            && self.request.is_none()
            && self.web_search.is_none()
            && self.internal_reasoning.is_none()
            && self.input_cache_read.is_none()
            && self.input_cache_write.is_none()
            && self.audio.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
    pub pricing: Option<FetchedModelPricing>,
    pub metadata: Option<serde_json::Value>,
}

#[allow(dead_code)]
pub fn fetched_model_feature_capabilities(
    provider_id: &str,
    model: &FetchedModel,
) -> zorai_shared::providers::ModelFeatureCapabilities {
    zorai_shared::providers::derive_model_feature_capabilities(
        provider_id,
        &model.id,
        model.metadata.as_ref(),
        model
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.image.as_deref())
            .is_some(),
    )
}

fn json_string_field(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_model_pricing(model: &serde_json::Value) -> Option<FetchedModelPricing> {
    let pricing = model.get("pricing")?.as_object()?;
    let pricing = FetchedModelPricing {
        prompt: json_string_field(pricing.get("prompt")),
        completion: json_string_field(pricing.get("completion")),
        image: json_string_field(pricing.get("image")),
        request: json_string_field(pricing.get("request")),
        web_search: json_string_field(pricing.get("web_search")),
        internal_reasoning: json_string_field(pricing.get("internal_reasoning")),
        input_cache_read: json_string_field(pricing.get("input_cache_read")),
        input_cache_write: json_string_field(pricing.get("input_cache_write")),
        audio: json_string_field(pricing.get("audio")),
    };
    (!pricing.is_empty()).then_some(pricing)
}

pub(crate) fn parse_fetched_models_response(json: &serde_json::Value) -> Vec<FetchedModel> {
    json.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let name = m
                        .get("name")
                        .or_else(|| m.get("display_name"))
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string());

                    let context_window = m
                        .get("context_length")
                        .or_else(|| m.get("context_window"))
                        .and_then(|c| match c {
                            serde_json::Value::Number(number) => number.as_u64(),
                            serde_json::Value::String(text) => text.trim().parse::<u64>().ok(),
                            _ => None,
                        })
                        .and_then(|n| u32::try_from(n).ok());

                    Some(FetchedModel {
                        id,
                        name,
                        context_window,
                        pricing: parse_model_pricing(m),
                        metadata: Some(m.clone()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn built_in_models_for_provider(provider_id: &str) -> Result<Vec<FetchedModel>> {
    get_provider_definition(provider_id)
        .map(|definition| {
            let mut models = definition
                .models
                .iter()
                .map(|model| FetchedModel {
                    id: model.id.to_string(),
                    name: Some(model.name.to_string()),
                    context_window: Some(model.context_window),
                    pricing: None,
                    metadata: None,
                })
                .collect::<Vec<_>>();
            if models.is_empty() && !definition.default_model.is_empty() {
                models.push(FetchedModel {
                    id: definition.default_model.to_string(),
                    name: Some(definition.default_model.to_string()),
                    context_window: None,
                    pricing: None,
                    metadata: None,
                });
            }
            models
        })
        .ok_or_else(|| anyhow::anyhow!("Unknown provider '{}'", provider_id))
}

pub async fn fetch_models(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    output_modalities: Option<&str>,
) -> Result<Vec<FetchedModel>> {
    let _ = crate::agent::types::reload_custom_provider_catalog_from_default_path();

    if provider_id == zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT {
        return built_in_models_for_provider(provider_id);
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_OLLAMA {
        return fetch_ollama_models(base_url, api_key).await;
    }

    let def = crate::agent::types::get_provider_definition(provider_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider '{}'", provider_id))?;

    if !def.supports_model_fetch {
        tracing::info!(
            provider_id,
            "provider does not support remote model fetching; returning built-in catalog"
        );
        return built_in_models_for_provider(provider_id);
    }

    match fetch_remote_models(def, provider_id, base_url, api_key, output_modalities).await {
        Ok(models) => Ok(models),
        Err(error) => {
            let fallback = built_in_models_for_provider(provider_id).unwrap_or_default();
            if fallback.is_empty() {
                Err(error)
            } else {
                tracing::warn!(
                    provider_id,
                    %error,
                    "remote model fetch failed; returning built-in catalog"
                );
                Ok(fallback)
            }
        }
    }
}

async fn fetch_remote_models(
    def: &crate::agent::types::ProviderDefinition,
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    output_modalities: Option<&str>,
) -> Result<Vec<FetchedModel>> {
    let client = reqwest::Client::new();
    let trimmed_base_url = base_url.trim_end_matches('/');
    let mut url = if def.api_type == ApiType::Anthropic && !trimmed_base_url.ends_with("/v1") {
        format!("{trimmed_base_url}/v1/models")
    } else {
        format!("{trimmed_base_url}/models")
    };
    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENROUTER {
        if let Some(output_modalities) = output_modalities
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let normalized = output_modalities.to_ascii_lowercase();
            if normalized == "embedding" || normalized == "embeddings" {
                url = format!("{trimmed_base_url}/embeddings/models");
            } else {
                url.push_str("?output_modalities=");
                url.push_str(output_modalities);
            }
        }
    }
    let send_request = |include_auth: bool| {
        let mut req = client.get(&url).header("Content-Type", "application/json");
        if include_auth && !api_key.is_empty() {
            req = def.auth_method.apply(req, api_key);
        }
        let req = apply_openrouter_attribution_headers(req, provider_id);
        let req = apply_opencode_go_headers(req, provider_id, None);
        apply_dashscope_coding_plan_sdk_headers(req, provider_id, trimmed_base_url, def.api_type)
    };

    let mut response = send_request(true).send().await?;

    if provider_id == zorai_shared::providers::PROVIDER_ID_CHUTES
        && !api_key.is_empty()
        && matches!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        )
    {
        tracing::info!(
            provider_id,
            status = %response.status(),
            "retrying model catalog fetch without auth after auth failure"
        );
        response = send_request(false).send().await?;
    }

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch models: {} - {}",
            status,
            text
        ));
    }

    let json: serde_json::Value = response.json().await?;
    Ok(parse_fetched_models_response(&json))
}

async fn fetch_ollama_models(base_url: &str, api_key: &str) -> Result<Vec<FetchedModel>> {
    let client = reqwest::Client::new();
    let trimmed = base_url.trim_end_matches('/');
    let api_root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    let tags_url = format!("{api_root}/api/tags");

    let mut req = client
        .get(&tags_url)
        .header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let response = req.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch Ollama models: {} - {}",
            status,
            text
        ));
    }

    let json: serde_json::Value = response.json().await?;
    let names: Vec<String> = json
        .get("models")
        .and_then(|models| models.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|model| {
                    model
                        .get("model")
                        .or_else(|| model.get("name"))
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let details = futures::future::join_all(
        names
            .iter()
            .map(|name| fetch_ollama_model_detail(&client, api_root, api_key, name)),
    )
    .await;

    Ok(names
        .into_iter()
        .zip(details)
        .map(|(name, (context_window, metadata))| FetchedModel {
            id: name.clone(),
            name: Some(name),
            context_window,
            pricing: None,
            metadata,
        })
        .collect())
}

async fn fetch_ollama_model_detail(
    client: &reqwest::Client,
    api_root: &str,
    api_key: &str,
    model: &str,
) -> (Option<u32>, Option<serde_json::Value>) {
    let url = format!("{api_root}/api/show");
    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "model": model }));
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let Ok(response) = req.send().await else {
        return (None, None);
    };
    if !response.status().is_success() {
        return (None, None);
    }
    let Ok(json) = response.json::<serde_json::Value>().await else {
        return (None, None);
    };

    parse_ollama_model_detail(&json)
}

pub(crate) fn parse_ollama_model_detail(
    json: &serde_json::Value,
) -> (Option<u32>, Option<serde_json::Value>) {
    let context_window = json
        .get("model_info")
        .and_then(|info| info.as_object())
        .and_then(|info| {
            info.iter().find_map(|(key, value)| {
                if key.ends_with(".context_length") {
                    value.as_u64()
                } else {
                    None
                }
            })
        })
        .and_then(|value| u32::try_from(value).ok());

    let vision = json
        .get("capabilities")
        .and_then(|caps| caps.as_array())
        .map(|caps| {
            caps.iter().any(|cap| {
                cap.as_str()
                    .map(|cap| cap.eq_ignore_ascii_case("vision"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    let mut input_modalities = vec![serde_json::Value::String("text".to_string())];
    if vision {
        input_modalities.push(serde_json::Value::String("image".to_string()));
    }

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "input_modalities".to_string(),
        serde_json::Value::Array(input_modalities),
    );
    if let Some(details) = json.get("details") {
        metadata.insert("details".to_string(), details.clone());
    }
    if let Some(caps) = json.get("capabilities") {
        metadata.insert("capabilities".to_string(), caps.clone());
    }

    (context_window, Some(serde_json::Value::Object(metadata)))
}

pub async fn validate_provider_connection(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    auth_source: AuthSource,
) -> Result<Option<Vec<FetchedModel>>> {
    let _ = reload_custom_provider_catalog_from_default_path();

    let def = get_provider_definition(provider_id)
        .with_context(|| format!("Unknown provider '{}'", provider_id))?;
    let resolved_base_url = if base_url.trim().is_empty() {
        def.default_base_url.to_string()
    } else {
        base_url.trim().to_string()
    };

    if provider_id == zorai_shared::providers::PROVIDER_ID_CURSOR {
        if !crate::agent::cursor_auth::cursor_cli_available() {
            anyhow::bail!(
                "Cursor CLI (`agent`) was not found on PATH. Install the Cursor CLI and run `agent login`."
            );
        }
        if api_key.trim().is_empty()
            && !crate::agent::cursor_auth::cursor_subscription_authenticated()
        {
            anyhow::bail!(
                "Cursor subscription is not logged in. Use Subscription login or run `agent login`."
            );
        }
        return Ok(None);
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_AZURE_OPENAI {
        let models = fetch_models(provider_id, &resolved_base_url, api_key, None).await?;
        return Ok(Some(models));
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT {
        let models = super::super::copilot_auth::list_github_copilot_models(api_key, auth_source)?
            .into_iter()
            .map(|model| FetchedModel {
                id: model.id,
                name: model.name,
                context_window: model.context_window,
                pricing: None,
                metadata: None,
            })
            .collect::<Vec<_>>();
        if models.is_empty() {
            anyhow::bail!("GitHub Copilot auth is valid but no models are available");
        }
        return Ok(Some(models));
    }

    if provider_id == zorai_shared::providers::PROVIDER_ID_OPENAI
        && auth_source == AuthSource::ChatgptSubscription
    {
        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: resolved_base_url,
            model: def.default_model.to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source,
            api_transport: def.default_transport,
            reasoning_effort: "off".to_string(),
            context_window_tokens: def
                .models
                .iter()
                .find(|model| model.id == def.default_model)
                .map(|model| model.context_window)
                .unwrap_or(128_000),
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
        };
        let _ = resolve_openai_codex_request_auth(&client, provider_id, &config).await?;
        return Ok(None);
    }

    let client = reqwest::Client::new();
    let api_type = get_provider_api_type(provider_id, def.default_model, &resolved_base_url);
    let request = match api_type {
        ApiType::OpenAI => {
            let url = build_chat_completion_url(&resolved_base_url);
            let body = serde_json::json!({
                "model": def.default_model,
                "messages": [{ "role": "user", "content": "ok" }],
                "max_tokens": 1,
                "stream": false,
            });
            let req = client.post(url).header("Content-Type", "application/json");
            let req = if !api_key.is_empty() {
                def.auth_method.apply(req, api_key)
            } else {
                req
            };
            apply_opencode_go_headers(
                apply_dashscope_coding_plan_sdk_headers(
                    req,
                    provider_id,
                    &resolved_base_url,
                    api_type,
                ),
                provider_id,
                None,
            )
            .json(&body)
        }
        ApiType::Anthropic => {
            let base = resolved_base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                format!("{}/messages", base)
            } else {
                format!("{}/v1/messages", base)
            };
            let body = serde_json::json!({
                "model": def.default_model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "ok" }],
            });
            let req = client.post(url).header("Content-Type", "application/json");
            let req = if !is_dashscope_coding_plan_anthropic_base_url(&resolved_base_url) {
                req.header("anthropic-version", "2023-06-01")
            } else {
                req
            };
            let req = if !api_key.is_empty() {
                def.auth_method.apply(req, api_key)
            } else {
                req
            };
            apply_opencode_go_headers(
                apply_dashscope_coding_plan_sdk_headers(
                    req,
                    provider_id,
                    &resolved_base_url,
                    api_type,
                ),
                provider_id,
                None,
            )
            .json(&body)
        }
    };

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Validation failed: {} - {}", status, text);
    }

    Ok(None)
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    fn write_custom_auth(yaml: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let custom_auth_path = temp_dir.path().join("custom-auth.yaml");
        std::fs::write(&custom_auth_path, yaml).expect("write custom auth");
        (temp_dir, custom_auth_path)
    }

    #[tokio::test]
    async fn fetch_models_returns_yaml_models_for_custom_provider_without_remote_fetch() {
        let _lock = crate::test_support::env_test_lock();
        let _guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
        let (_temp_dir, custom_auth_path) = write_custom_auth(
            r#"
providers:
  - id: yaml-provider
    name: Yaml Provider
    default_base_url: http://127.0.0.1:9/v1
    default_model: model-a
    models:
      - id: model-a
        context_window: 128000
      - id: model-b
        name: Model B
        context_window: 64000
"#,
        );
        std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);

        let models = fetch_models("yaml-provider", "http://127.0.0.1:9/v1", "", None)
            .await
            .expect("custom provider models should come from the yaml catalog");

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "model-a");
        assert_eq!(models[1].id, "model-b");
        assert_eq!(models[1].name.as_deref(), Some("Model B"));
        assert_eq!(models[1].context_window, Some(64_000));
    }

    #[tokio::test]
    async fn fetch_models_falls_back_to_yaml_models_when_remote_fetch_fails() {
        let _lock = crate::test_support::env_test_lock();
        let _guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
        let (_temp_dir, custom_auth_path) = write_custom_auth(
            r#"
providers:
  - id: yaml-fetch-provider
    name: Yaml Fetch Provider
    default_base_url: http://127.0.0.1:9/v1
    default_model: model-a
    supports_model_fetch: true
    models:
      - id: model-a
        context_window: 128000
"#,
        );
        std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);

        let models = fetch_models("yaml-fetch-provider", "http://127.0.0.1:9/v1", "", None)
            .await
            .expect("remote fetch failure should fall back to the yaml catalog");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "model-a");
    }

    #[tokio::test]
    async fn fetch_models_returns_default_model_when_custom_provider_has_no_models() {
        let _lock = crate::test_support::env_test_lock();
        let _guard = crate::test_support::EnvGuard::new(&["ZORAI_CUSTOM_AUTH_PATH"]);
        let (_temp_dir, custom_auth_path) = write_custom_auth(
            r#"
providers:
  - id: yaml-default-only
    name: Yaml Default Only
    default_base_url: http://127.0.0.1:9/v1
    default_model: only-model
"#,
        );
        std::env::set_var("ZORAI_CUSTOM_AUTH_PATH", &custom_auth_path);

        let models = fetch_models("yaml-default-only", "http://127.0.0.1:9/v1", "", None)
            .await
            .expect("provider with only a default model should still list it");

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "only-model");
    }

    #[test]
    fn parse_fetched_models_response_preserves_xai_metadata_and_pricing() {
        let json = serde_json::json!({
            "data": [{
                "id": "grok-4",
                "name": "Grok 4",
                "context_length": "262144",
                "pricing": {
                    "prompt": "3.00",
                    "completion": "15.00",
                    "input_cache_read": "0.75"
                },
                "modalities": ["text", "image"],
                "owned_by": "xai",
                "supports_reasoning": true
            }, {
                "id": "grok-code-fast-1",
                "context_window": 131072,
                "pricing": {
                    "request": 42
                },
                "mode": "code"
            }, {
                "id": "grok-overflow",
                "context_length": "4294967296"
            }]
        });

        let models = parse_fetched_models_response(&json);

        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "grok-4");
        assert_eq!(models[0].name.as_deref(), Some("Grok 4"));
        assert_eq!(models[0].context_window, Some(262_144));
        assert_eq!(
            models[0].pricing.as_ref().and_then(|p| p.prompt.as_deref()),
            Some("3.00")
        );
        assert_eq!(
            models[0]
                .pricing
                .as_ref()
                .and_then(|p| p.completion.as_deref()),
            Some("15.00")
        );
        assert_eq!(
            models[0]
                .pricing
                .as_ref()
                .and_then(|p| p.input_cache_read.as_deref()),
            Some("0.75")
        );
        assert_eq!(
            models[0]
                .metadata
                .as_ref()
                .and_then(|m| m.get("owned_by"))
                .and_then(|v| v.as_str()),
            Some("xai")
        );
        assert_eq!(
            models[0]
                .metadata
                .as_ref()
                .and_then(|m| m.get("modalities"))
                .and_then(|v| v.as_array())
                .map(|items| items.len()),
            Some(2)
        );

        assert_eq!(models[1].id, "grok-code-fast-1");
        assert_eq!(models[1].context_window, Some(131_072));
        assert_eq!(
            models[1]
                .pricing
                .as_ref()
                .and_then(|p| p.request.as_deref()),
            Some("42")
        );
        assert_eq!(
            models[1]
                .metadata
                .as_ref()
                .and_then(|m| m.get("mode"))
                .and_then(|v| v.as_str()),
            Some("code")
        );

        assert_eq!(models[2].id, "grok-overflow");
        assert_eq!(models[2].context_window, None);
    }

    #[test]
    fn parse_fetched_models_response_uses_display_name_when_name_is_absent() {
        let json = serde_json::json!({
            "data": [{
                "id": "MiniMax-M3",
                "display_name": "MiniMax-M3",
                "type": "model"
            }]
        });

        let models = parse_fetched_models_response(&json);

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "MiniMax-M3");
        assert_eq!(models[0].name.as_deref(), Some("MiniMax-M3"));
    }
}
