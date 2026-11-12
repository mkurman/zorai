#![allow(deprecated)]

//! Handlebars template engine for plugin API proxy.
//!
//! Provides template rendering with strict mode and 5 custom helpers:
//! `urlencode`, `json`, `default`, `truncate`, `join`.
//!
//! Templates are rendered with an isolated per-call context containing
//! `params` and `settings` keys. Rendering is capped at 1 second via
//! tokio timeout.

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext, RenderError};

use super::api_proxy::{PluginApiError, RenderedRequest};
use super::manifest::{ApiSection, EndpointDef};

/// Create a Handlebars registry with strict mode and all custom helpers registered.
pub fn create_registry() -> Handlebars<'static> {
    let mut hbs = Handlebars::new();
    hbs.set_strict_mode(true);

    hbs.register_helper("urlencode", Box::new(helper_urlencode));
    hbs.register_helper("json", Box::new(helper_json));
    hbs.register_helper("default", Box::new(helper_default));
    hbs.register_helper("truncate", Box::new(helper_truncate));
    hbs.register_helper("join", Box::new(helper_join));

    hbs
}

/// Build an isolated template context with `params`, `settings`, and optional `auth` keys.
///
/// `params` comes from the API call arguments.
/// `settings` is built from the plugin's persisted settings (key, value, is_secret).
/// `auth` (if Some) provides OAuth token variables (`auth.access_token`, etc.) per D-11.
pub fn build_context(
    params: serde_json::Value,
    settings: Vec<(String, String, bool)>,
    auth: Option<serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Value {
    let mut settings_map = serde_json::Map::new();
    for (key, value, _is_secret) in settings {
        settings_map.insert(key, serde_json::Value::String(value));
    }

    let mut ctx = serde_json::json!({
        "params": params,
        "settings": settings_map,
    });

    if let Some(auth_map) = auth {
        ctx["auth"] = serde_json::Value::Object(auth_map);
    }

    ctx
}

/// Render a full HTTP request from an EndpointDef and template context.
///
/// Renders URL path, headers, and body templates. Uses the endpoint's method as-is.
/// Wrapped in a 1-second tokio timeout.
pub async fn render_request(
    registry: &Handlebars<'static>,
    api: &ApiSection,
    endpoint: &EndpointDef,
    context: &serde_json::Value,
) -> Result<RenderedRequest, PluginApiError> {
    let reg = registry.clone();
    let api = api.clone();
    let endpoint = endpoint.clone();
    let ctx = context.clone();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::task::spawn_blocking(move || render_request_sync(&reg, &api, &endpoint, &ctx)),
    )
    .await;

    match result {
        Ok(Ok(inner)) => inner,
        Ok(Err(join_err)) => Err(PluginApiError::TemplateError {
            detail: format!("template task panicked: {join_err}"),
        }),
        Err(_timeout) => Err(PluginApiError::TemplateError {
            detail: "template rendering timed out (1s limit)".to_string(),
        }),
    }
}

/// Synchronous inner function for render_request (runs inside spawn_blocking).
fn render_request_sync(
    registry: &Handlebars<'_>,
    api: &ApiSection,
    endpoint: &EndpointDef,
    context: &serde_json::Value,
) -> Result<RenderedRequest, PluginApiError> {
    let base = api.base_url.as_deref().unwrap_or("");
    let path_rendered = registry
        .render_template(&endpoint.path, context)
        .map_err(|e| PluginApiError::TemplateError {
            detail: format!("path template: {e}"),
        })?;
    let url = format!("{}{}", base.trim_end_matches('/'), path_rendered);

    let mut headers = Vec::new();
    if let Some(ref header_map) = endpoint.headers {
        for (key, value_tpl) in header_map {
            let rendered = registry.render_template(value_tpl, context).map_err(|e| {
                PluginApiError::TemplateError {
                    detail: format!("header '{key}' template: {e}"),
                }
            })?;
            headers.push((key.clone(), rendered));
        }
    }

    let body = if let Some(ref body_value) = endpoint.body {
        let body_template = match body_value {
            serde_json::Value::String(raw) => raw.clone(),
            other => serde_json::to_string(other).map_err(|e| PluginApiError::TemplateError {
                detail: format!("body serialization: {e}"),
            })?,
        };
        let rendered = registry
            .render_template(&body_template, context)
            .map_err(|e| PluginApiError::TemplateError {
                detail: format!("body template: {e}"),
            })?;
        Some(rendered)
    } else {
        None
    };

    Ok(RenderedRequest {
        method: endpoint.method.clone(),
        url,
        headers,
        body,
    })
}

/// Render a response using the endpoint's response_template.
///
/// If `response_template` is Some, renders it with the response JSON as context.
/// If None, returns pretty-printed JSON.
pub fn render_response(
    registry: &Handlebars<'_>,
    endpoint: &EndpointDef,
    response_json: &serde_json::Value,
) -> Result<String, PluginApiError> {
    match &endpoint.response_template {
        Some(template) => registry
            .render_template(template, response_json)
            .map_err(|e| PluginApiError::TemplateError {
                detail: format!("response template: {e}"),
            }),
        None => {
            serde_json::to_string_pretty(response_json).map_err(|e| PluginApiError::TemplateError {
                detail: format!("response JSON serialization: {e}"),
            })
        }
    }
}

/// `{{urlencode value}}` -- percent-encode a string value.
fn helper_urlencode(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h
        .param(0)
        .ok_or_else(|| RenderError::new("urlencode: missing parameter"))?;
    let fallback = param.value().to_string();
    let s = param.value().as_str().unwrap_or(&fallback);
    let encoded =
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string();
    out.write(&encoded)?;
    Ok(())
}

/// `{{json value}}` -- serialize a value to a JSON string.
fn helper_json(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h
        .param(0)
        .ok_or_else(|| RenderError::new("json: missing parameter"))?;
    let json_str = serde_json::to_string(param.value())
        .map_err(|e| RenderError::new(format!("json: serialization error: {e}")))?;
    out.write(&json_str)?;
    Ok(())
}

/// `{{default value "fallback"}}` -- return fallback if value is null/undefined.
fn helper_default(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h
        .param(0)
        .ok_or_else(|| RenderError::new("default: missing parameter"))?;
    let fallback = h
        .param(1)
        .ok_or_else(|| RenderError::new("default: missing fallback parameter"))?;

    if param.value().is_null() {
        let fb_str = fallback.value().to_string();
        let fb = fallback.value().as_str().unwrap_or(&fb_str);
        out.write(fb)?;
    } else {
        let param_str = param.value().to_string();
        let s = param.value().as_str().unwrap_or(&param_str);
        out.write(s)?;
    }
    Ok(())
}

/// `{{truncate value N}}` -- truncate a string to N chars, appending "..." if truncated.
fn helper_truncate(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h
        .param(0)
        .ok_or_else(|| RenderError::new("truncate: missing parameter"))?;
    let max_len =
        h.param(1)
            .ok_or_else(|| RenderError::new("truncate: missing length parameter"))?
            .value()
            .as_u64()
            .ok_or_else(|| RenderError::new("truncate: length must be a number"))? as usize;

    let param_str = param.value().to_string();
    let s = param.value().as_str().unwrap_or(&param_str);

    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        out.write(&truncated)?;
        out.write("...")?;
    } else {
        out.write(s)?;
    }
    Ok(())
}

/// `{{join arr ", "}}` -- join array elements with a separator string.
fn helper_join(
    h: &Helper<'_>,
    _: &Handlebars<'_>,
    _: &Context,
    _: &mut RenderContext<'_, '_>,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h
        .param(0)
        .ok_or_else(|| RenderError::new("join: missing parameter"))?;
    let separator = h
        .param(1)
        .ok_or_else(|| RenderError::new("join: missing separator parameter"))?
        .value()
        .as_str()
        .unwrap_or(", ");

    let arr = param
        .value()
        .as_array()
        .ok_or_else(|| RenderError::new("join: first parameter must be an array"))?;

    let parts: Vec<String> = arr
        .iter()
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string())
        })
        .collect();

    out.write(&parts.join(separator))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn create_registry_returns_strict_mode() {
        let reg = create_registry();
        let result = reg.render_template("{{undefined_var}}", &serde_json::json!({}));
        assert!(
            result.is_err(),
            "strict mode should reject undefined variables"
        );
    }

    #[test]
    fn urlencode_helper_encodes_spaces() {
        let reg = create_registry();
        let result = reg
            .render_template(
                "{{urlencode value}}",
                &serde_json::json!({"value": "hello world"}),
            )
            .unwrap();
        assert_eq!(result, "hello%20world");
    }

    #[test]
    fn json_helper_serializes_object() {
        let reg = create_registry();
        let result = reg
            .render_template("{{json obj}}", &serde_json::json!({"obj": {"key": "val"}}))
            .unwrap();
        assert_eq!(result, r#"{"key":"val"}"#);
    }

    #[test]
    fn default_helper_returns_fallback_when_null() {
        let reg = create_registry();
        let result = reg
            .render_template(
                r#"{{default value "fallback"}}"#,
                &serde_json::json!({"value": null}),
            )
            .unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn default_helper_returns_value_when_present() {
        let reg = create_registry();
        let result = reg
            .render_template(
                r#"{{default value "fallback"}}"#,
                &serde_json::json!({"value": "actual"}),
            )
            .unwrap();
        assert_eq!(result, "actual");
    }

    #[test]
    fn truncate_helper_truncates_long_string() {
        let reg = create_registry();
        let result = reg
            .render_template(
                "{{truncate value 5}}",
                &serde_json::json!({"value": "hello world"}),
            )
            .unwrap();
        assert_eq!(result, "hello...");
    }

    #[test]
    fn truncate_helper_keeps_short_string() {
        let reg = create_registry();
        let result = reg
            .render_template(
                "{{truncate value 20}}",
                &serde_json::json!({"value": "hello"}),
            )
            .unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn join_helper_joins_array() {
        let reg = create_registry();
        let result = reg
            .render_template(
                r#"{{join arr ", "}}"#,
                &serde_json::json!({"arr": ["a", "b", "c"]}),
            )
            .unwrap();
        assert_eq!(result, "a, b, c");
    }

    #[test]
    fn build_context_produces_params_and_settings() {
        let ctx = build_context(
            serde_json::json!({"query": "test"}),
            vec![
                ("api_key".to_string(), "secret123".to_string(), true),
                (
                    "base_url".to_string(),
                    "https://api.example.com".to_string(),
                    false,
                ),
            ],
            None,
        );
        assert_eq!(ctx["params"]["query"], "test");
        assert_eq!(ctx["settings"]["api_key"], "secret123");
        assert_eq!(ctx["settings"]["base_url"], "https://api.example.com");
    }

    #[tokio::test]
    async fn render_request_renders_url_headers_body() {
        let reg = create_registry();
        let api = ApiSection {
            base_url: Some("https://api.example.com".to_string()),
            endpoints: HashMap::new(),
            rate_limit: None,
        };
        let endpoint = EndpointDef {
            method: "POST".to_string(),
            path: "/v1/users/{{params.user_id}}/messages".to_string(),
            params: None,
            headers: Some(HashMap::from([(
                "Authorization".to_string(),
                "Bearer {{settings.api_key}}".to_string(),
            )])),
            body: Some(serde_json::json!({"content": "{{params.message}}"})),
            response_template: None,
        };
        let ctx = build_context(
            serde_json::json!({"user_id": "123", "message": "hello"}),
            vec![("api_key".to_string(), "tok_abc".to_string(), true)],
            None,
        );

        let req = render_request(&reg, &api, &endpoint, &ctx).await.unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://api.example.com/v1/users/123/messages");
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer tok_abc"));
        assert_eq!(req.body.as_deref(), Some(r#"{"content":"hello"}"#));
    }

    #[tokio::test]
    async fn render_request_renders_raw_string_body_template() {
        let reg = create_registry();
        let api = ApiSection {
            base_url: Some("".to_string()),
            endpoints: HashMap::new(),
            rate_limit: None,
        };
        let endpoint = EndpointDef {
            method: "PUT".to_string(),
            path:
                "https://api.endpoints.huggingface.cloud/v2/endpoint/{{params.ns}}/{{params.name}}"
                    .to_string(),
            params: None,
            headers: None,
            body: Some(serde_json::Value::String("{{{params.patch}}}".to_string())),
            response_template: None,
        };
        let patch_json = r#"{"compute":{"scaling":{"minReplica":2,"maxReplica":4}}}"#;
        let ctx = build_context(
            serde_json::json!({"ns": "alice", "name": "ep1", "patch": patch_json}),
            vec![],
            None,
        );

        let req = render_request(&reg, &api, &endpoint, &ctx).await.unwrap();
        assert_eq!(req.method, "PUT");
        assert_eq!(
            req.url,
            "https://api.endpoints.huggingface.cloud/v2/endpoint/alice/ep1"
        );
        // Raw string body: triple-stash substitution lets the agent pass arbitrary JSON
        // (with numeric fields, nested objects, etc.) without going through the structured
        // body schema. Critical for HF endpoint update / AutoTrain config patches.
        assert_eq!(req.body.as_deref(), Some(patch_json));
    }

    #[tokio::test]
    async fn render_request_renders_github_repo_auth_and_path() {
        let reg = create_registry();
        let api = ApiSection {
            base_url: Some("https://api.github.com".to_string()),
            endpoints: HashMap::new(),
            rate_limit: None,
        };
        let endpoint = EndpointDef {
            method: "GET".to_string(),
            path: "/repos/{{params.owner}}/{{params.repo}}".to_string(),
            params: None,
            headers: Some(HashMap::from([
                (
                    "Authorization".to_string(),
                    "Bearer {{settings.token}}".to_string(),
                ),
                (
                    "Accept".to_string(),
                    "application/vnd.github+json".to_string(),
                ),
                ("X-GitHub-Api-Version".to_string(), "2022-11-28".to_string()),
            ])),
            body: None,
            response_template: None,
        };
        let ctx = build_context(
            serde_json::json!({"owner": "mkurman", "repo": "zorai"}),
            vec![("token".to_string(), "ghp_test_token".to_string(), true)],
            None,
        );

        let req = render_request(&reg, &api, &endpoint, &ctx).await.unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://api.github.com/repos/mkurman/zorai");
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer ghp_test_token"));
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Accept" && v == "application/vnd.github+json"));
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "X-GitHub-Api-Version" && v == "2022-11-28"));
        assert!(req.body.is_none());
    }

    #[tokio::test]
    async fn render_request_renders_github_issues_path_with_defaults() {
        let reg = create_registry();
        let api = ApiSection {
            base_url: Some("https://api.github.com".to_string()),
            endpoints: HashMap::new(),
            rate_limit: None,
        };
        let endpoint = EndpointDef {
            method: "GET".to_string(),
            path: r#"/repos/{{params.owner}}/{{params.repo}}/issues?state={{default params.state "open"}}&per_page={{default params.per_page "20"}}"#.to_string(),
            params: None,
            headers: Some(HashMap::from([
                (
                    "Authorization".to_string(),
                    "Bearer {{settings.token}}".to_string(),
                ),
                (
                    "Accept".to_string(),
                    "application/vnd.github+json".to_string(),
                ),
                (
                    "X-GitHub-Api-Version".to_string(),
                    "2022-11-28".to_string(),
                ),
            ])),
            body: None,
            response_template: None,
        };
        let ctx = build_context(
            serde_json::json!({"owner": "mkurman", "repo": "zorai"}),
            vec![("token".to_string(), "ghp_test_token".to_string(), true)],
            None,
        );

        let req = render_request(&reg, &api, &endpoint, &ctx).await.unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(
            req.url,
            "https://api.github.com/repos/mkurman/zorai/issues?state=open&per_page=20"
        );
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer ghp_test_token"));
        assert!(req.body.is_none());
    }

    #[tokio::test]
    async fn render_request_renders_github_pulls_path_with_overrides() {
        let reg = create_registry();
        let api = ApiSection {
            base_url: Some("https://api.github.com".to_string()),
            endpoints: HashMap::new(),
            rate_limit: None,
        };
        let endpoint = EndpointDef {
            method: "GET".to_string(),
            path: r#"/repos/{{params.owner}}/{{params.repo}}/pulls?state={{default params.state "open"}}&per_page={{default params.per_page "20"}}"#.to_string(),
            params: None,
            headers: Some(HashMap::from([
                (
                    "Authorization".to_string(),
                    "Bearer {{settings.token}}".to_string(),
                ),
                (
                    "Accept".to_string(),
                    "application/vnd.github+json".to_string(),
                ),
                (
                    "X-GitHub-Api-Version".to_string(),
                    "2022-11-28".to_string(),
                ),
            ])),
            body: None,
            response_template: None,
        };
        let ctx = build_context(
            serde_json::json!({
                "owner": "mkurman",
                "repo": "zorai",
                "state": "closed",
                "per_page": "50"
            }),
            vec![("token".to_string(), "ghp_test_token".to_string(), true)],
            None,
        );

        let req = render_request(&reg, &api, &endpoint, &ctx).await.unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(
            req.url,
            "https://api.github.com/repos/mkurman/zorai/pulls?state=closed&per_page=50"
        );
        assert!(req
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer ghp_test_token"));
        assert!(req.body.is_none());
    }

    #[test]
    fn render_response_with_template() {
        let reg = create_registry();
        let endpoint = EndpointDef {
            method: "GET".to_string(),
            path: "/test".to_string(),
            params: None,
            headers: None,
            body: None,
            response_template: Some("Subject: {{subject}}\nFrom: {{from}}".to_string()),
        };
        let response = serde_json::json!({
            "subject": "Hello",
            "from": "alice@example.com",
        });
        let result = render_response(&reg, &endpoint, &response).unwrap();
        assert!(result.contains("Subject: Hello"));
        assert!(result.contains("From: alice@example.com"));
    }

    #[test]
    fn render_response_without_template_returns_pretty_json() {
        let reg = create_registry();
        let endpoint = EndpointDef {
            method: "GET".to_string(),
            path: "/test".to_string(),
            params: None,
            headers: None,
            body: None,
            response_template: None,
        };
        let response = serde_json::json!({"key": "value"});
        let result = render_response(&reg, &endpoint, &response).unwrap();
        assert!(result.contains("\"key\""));
        assert!(result.contains("\"value\""));
        assert!(result.contains('\n'), "should be pretty-printed");
    }

    #[test]
    fn strict_mode_rejects_undefined_variables() {
        let reg = create_registry();
        let result =
            reg.render_template("{{undefined_var}}", &serde_json::json!({"other": "value"}));
        assert!(result.is_err());
    }
}
