//! Plugin API proxy error types and request structure.
//!
//! Provides `PluginApiError` (structured error enum for all API proxy failures)
//! and `RenderedRequest` (output of template rendering, input to HTTP execution).

/// Structured error type for plugin API proxy operations.
/// Each variant produces an actionable error message for the agent/user.
#[derive(Debug, thiserror::Error)]
pub enum PluginApiError {
    #[error("SSRF blocked: request to {url} targets an internal/private IP range")]
    SsrfBlocked { url: String },

    #[error(
        "Rate limited: plugin '{plugin}' exceeded rate limit. Retry after {retry_after_secs}s and reduce connector polling or retry frequency."
    )]
    RateLimited {
        plugin: String,
        retry_after_secs: u64,
    },

    #[error("Template error: {detail}")]
    TemplateError { detail: String },

    #[error("HTTP {status}: {body}")]
    HttpError { status: u16, body: String },

    #[error("Request timed out (30s limit). Recovery: verify the service is reachable and retry.")]
    Timeout,

    #[error("Endpoint '{endpoint}' not found in plugin '{plugin}'")]
    EndpointNotFound { plugin: String, endpoint: String },

    #[error("Plugin '{name}' not found or not loaded")]
    PluginNotFound { name: String },

    #[error("Plugin '{name}' is disabled")]
    PluginDisabled { name: String },

    #[error("OAuth token expired for plugin '{plugin}'. User must re-authorize.")]
    AuthExpired { plugin: String },

    #[error("Plugin '{plugin}' {version} wants to run commands on this computer. Approve its local commands in zorai's Plugins settings, then retry.")]
    LocalRunNotApproved { plugin: String, version: String },

    #[error("Local command did not finish within {secs}s and was stopped.")]
    LocalRunTimeout { secs: u64 },

    #[error("Local command failed{}: {stderr}", .code.map(|code| format!(" with exit code {code}")).unwrap_or_default())]
    LocalRunFailed { code: Option<i32>, stderr: String },

    #[error("Local command could not start: {detail}")]
    LocalRunSpawn { detail: String },
}

/// Output of template rendering: a fully-resolved HTTP request ready to execute.
#[derive(Debug, Clone)]
pub struct RenderedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// HTTP request timeout for plugin API calls.
const HTTP_TIMEOUT_SECS: u64 = 30;

/// Maximum response body length to include in error messages.
const MAX_ERROR_BODY_CHARS: usize = 2000;

/// Default retry-after duration when upstream returns 429 without a header.
const DEFAULT_RETRY_AFTER_SECS: u64 = 60;

/// Execute an HTTP request from a RenderedRequest and return the parsed JSON response.
///
/// Handles:
/// - HTTP timeout (30s) -> `PluginApiError::Timeout`
/// - Upstream 429 -> `PluginApiError::RateLimited` with retry-after from header or default 60s
/// - HTTP error status -> `PluginApiError::HttpError` with truncated body
/// - Non-JSON response -> wraps raw text in `{"text": "..."}`
/// - Success -> parsed `serde_json::Value`
pub async fn execute_request(
    http_client: &reqwest::Client,
    rendered: &RenderedRequest,
) -> Result<serde_json::Value, PluginApiError> {
    let method: reqwest::Method =
        rendered
            .method
            .parse()
            .map_err(|e| PluginApiError::TemplateError {
                detail: format!("invalid HTTP method '{}': {e}", rendered.method),
            })?;

    let mut builder = http_client.request(method, &rendered.url);

    for (key, value) in &rendered.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if let Some(ref body) = rendered.body {
        builder = builder.body(body.clone());
    }

    let response = match tokio::time::timeout(
        std::time::Duration::from_secs(HTTP_TIMEOUT_SECS),
        builder.send(),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            return Err(PluginApiError::HttpError {
                status: 0,
                body: format!("request failed: {e}"),
            });
        }
        Err(_timeout) => {
            return Err(PluginApiError::Timeout);
        }
    };

    let status = response.status();

    if status.as_u16() == 429 {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_RETRY_AFTER_SECS);
        return Err(PluginApiError::RateLimited {
            plugin: String::new(),
            retry_after_secs: retry_after,
        });
    }

    let body_text = response.text().await.unwrap_or_default();

    if !status.is_success() {
        let truncated = if body_text.len() > MAX_ERROR_BODY_CHARS {
            format!("{}...", &body_text[..MAX_ERROR_BODY_CHARS])
        } else {
            body_text
        };
        return Err(PluginApiError::HttpError {
            status: status.as_u16(),
            body: truncated,
        });
    }

    match serde_json::from_str::<serde_json::Value>(&body_text) {
        Ok(json) => Ok(json),
        Err(_) => Ok(serde_json::json!({ "text": body_text })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssrf_blocked_display_contains_url() {
        let err = PluginApiError::SsrfBlocked {
            url: "http://127.0.0.1/secret".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("127.0.0.1/secret"), "msg: {msg}");
    }

    #[test]
    fn rate_limited_display_contains_retry_info() {
        let err = PluginApiError::RateLimited {
            plugin: "gmail".to_string(),
            retry_after_secs: 30,
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("gmail"), "msg: {msg}");
        assert!(msg.contains("30"), "msg: {msg}");
    }

    #[test]
    fn template_error_display() {
        let err = PluginApiError::TemplateError {
            detail: "missing variable".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("missing variable"), "msg: {msg}");
    }

    #[test]
    fn http_error_display() {
        let err = PluginApiError::HttpError {
            status: 404,
            body: "not found".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("404"), "msg: {msg}");
        assert!(msg.contains("not found"), "msg: {msg}");
    }

    #[test]
    fn timeout_display() {
        let err = PluginApiError::Timeout;
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("30s"), "msg: {msg}");
    }

    #[test]
    fn endpoint_not_found_display() {
        let err = PluginApiError::EndpointNotFound {
            plugin: "gmail".to_string(),
            endpoint: "send_email".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("send_email"), "msg: {msg}");
        assert!(msg.contains("gmail"), "msg: {msg}");
    }

    #[test]
    fn plugin_not_found_display_contains_name() {
        let err = PluginApiError::PluginNotFound {
            name: "missing-plugin".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("missing-plugin"), "msg: {msg}");
    }

    #[test]
    fn plugin_disabled_display() {
        let err = PluginApiError::PluginDisabled {
            name: "disabled-plugin".to_string(),
        };
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("disabled-plugin"), "msg: {msg}");
    }
}
