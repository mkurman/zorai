use super::super::openai_codex_auth::{
    extract_openai_codex_account_id, import_codex_cli_auth_if_present,
    read_stored_openai_codex_auth, write_stored_openai_codex_auth, StoredOpenAICodexAuth,
    OPENAI_AUTH_MODE, OPENAI_CODEX_AUTH_CLIENT_ID, OPENAI_CODEX_AUTH_PROVIDER,
    OPENAI_CODEX_AUTH_TOKEN_URL,
};
use super::*;
fn parse_retry_after_ms_from_value(value: &serde_json::Value) -> Option<u64> {
    if let Some(seconds) = value.as_f64() {
        return Some((seconds * 1000.0).ceil().max(1.0) as u64);
    }
    if let Some(raw) = value.as_str() {
        if let Ok(seconds) = raw.trim().parse::<f64>() {
            return Some((seconds * 1000.0).ceil().max(1.0) as u64);
        }
    }
    None
}

fn moonshot_client_onboarding_required(provider: &str, raw_message: &str) -> bool {
    if provider != zorai_shared::providers::PROVIDER_ID_KIMI_CODING_PLAN {
        return false;
    }

    let lower = raw_message.to_ascii_lowercase();
    lower.contains("only available for coding agents")
        || lower.contains("client onboarding")
        || lower.contains("allowlist")
        || lower.contains("user-agent")
}

fn auth_configuration_summary(provider: &str, raw_message: &str) -> String {
    if moonshot_client_onboarding_required(provider, raw_message) {
        return format!(
            "{provider} rejected the request because this client is not allowlisted by Moonshot yet: {raw_message}. Have the open-source maintainer or product owner email code@moonshot.ai with the client name and description, the repository URL or company use case, and the User-Agent to allowlist."
        );
    }

    format!(
        "{provider} rejected the request because authentication or provider configuration is invalid: {raw_message}"
    )
}

pub(crate) fn extract_retry_after_ms(
    headers: Option<&reqwest::header::HeaderMap>,
    body_text: &str,
) -> Option<u64> {
    if let Some(headers) = headers {
        if let Some(value) = headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<f64>().ok())
        {
            return Some((value * 1000.0).ceil().max(1.0) as u64);
        }
    }

    serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|value| {
            value
                .get("retry_after")
                .or_else(|| value.pointer("/error/retry_after"))
                .and_then(parse_retry_after_ms_from_value)
        })
}

pub(crate) fn classify_http_failure_with_retry_after(
    status: reqwest::StatusCode,
    provider: &str,
    body_text: &str,
    retry_after_ms: Option<u64>,
) -> anyhow::Error {
    let parsed_body = serde_json::from_str::<serde_json::Value>(body_text).ok();
    let raw_message = raw_upstream_message(body_text);
    let lower = raw_message.to_ascii_lowercase();
    let upstream_error_type = parsed_body
        .as_ref()
        .and_then(|value| {
            value
                .pointer("/error/type")
                .and_then(|value| value.as_str())
        })
        .map(str::to_string);
    let upstream_request_id = parsed_body
        .as_ref()
        .and_then(|value| {
            value
                .get("request_id")
                .or_else(|| value.get("request-id"))
                .and_then(|value| value.as_str())
        })
        .map(str::to_string);
    let request_invalid_like = lower.contains("invalid '")
        || lower.contains("invalid request")
        || lower.contains("request body")
        || lower.contains("malformed")
        || lower.contains("empty string")
        || lower.contains("tool")
        || lower.contains("required")
        || lower.contains("missing");
    let context_window_exceeded_like = lower.contains("context_length_exceeded")
        || lower.contains("context window")
        || lower.contains("context length")
        || (lower.contains("input") && lower.contains("exceeds") && lower.contains("context"));
    let transport_incompatible_like = lower.contains("not supported")
        || lower.contains("does not support")
        || lower.contains("incompatible");
    let nested_upstream_overload_like = lower.contains("too many concurrent requests")
        || ["status=500", "status=502", "status=503", "status=504"]
            .iter()
            .any(|marker| lower.contains(marker));
    let class = if matches!(upstream_error_type.as_deref(), Some("rate_limit_error"))
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        UpstreamFailureClass::RateLimit
    } else if nested_upstream_overload_like {
        UpstreamFailureClass::TemporaryUpstream
    } else if matches!(
        upstream_error_type.as_deref(),
        Some("authentication_error" | "billing_error" | "permission_error")
    ) || matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::PAYMENT_REQUIRED
            | reqwest::StatusCode::FORBIDDEN
    ) {
        UpstreamFailureClass::AuthConfiguration
    } else if context_window_exceeded_like {
        UpstreamFailureClass::ContextWindowExceeded
    } else if matches!(
        upstream_error_type.as_deref(),
        Some("invalid_request_error" | "request_too_large")
    ) || status == reqwest::StatusCode::PAYLOAD_TOO_LARGE
    {
        UpstreamFailureClass::RequestInvalid
    } else if matches!(upstream_error_type.as_deref(), Some("not_found_error")) {
        UpstreamFailureClass::TransportIncompatible
    } else if matches!(
        upstream_error_type.as_deref(),
        Some("api_error" | "overloaded_error")
    ) {
        UpstreamFailureClass::TemporaryUpstream
    } else if (matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::UNPROCESSABLE_ENTITY
    ) && request_invalid_like)
        || (status == reqwest::StatusCode::UNPROCESSABLE_ENTITY && !transport_incompatible_like)
    {
        UpstreamFailureClass::RequestInvalid
    } else if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND
            | reqwest::StatusCode::METHOD_NOT_ALLOWED
            | reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
    ) || lower.contains("not supported")
        || lower.contains("does not support")
        || lower.contains("incompatible")
    {
        UpstreamFailureClass::TransportIncompatible
    } else if matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::CONFLICT
            | reqwest::StatusCode::TOO_EARLY
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    ) || status.as_u16() == 529
        || lower.contains("service unavailable")
        || lower.contains("temporarily unavailable")
        || lower.contains("overloaded")
        || lower.contains("try again later")
        || lower.contains("while retrying upstream")
    {
        UpstreamFailureClass::TemporaryUpstream
    } else {
        UpstreamFailureClass::Unknown
    };

    let summary = match class {
        UpstreamFailureClass::RequestInvalid => {
            format!("{provider} rejected the daemon request as invalid: {raw_message}")
        }
        UpstreamFailureClass::ContextWindowExceeded => {
            format!("{provider} rejected the request because the input exceeds the model context window: {raw_message}")
        }
        UpstreamFailureClass::AuthConfiguration => {
            auth_configuration_summary(provider, &raw_message)
        }
        UpstreamFailureClass::TransportIncompatible => {
            format!("The selected provider/transport combination is incompatible for {provider}: {raw_message}")
        }
        UpstreamFailureClass::TemporaryUpstream => {
            format!("{provider} is temporarily unavailable upstream: {raw_message}")
        }
        UpstreamFailureClass::RateLimit => format!("{provider} API returned 429: {raw_message}"),
        UpstreamFailureClass::TransientTransport => {
            format!("{provider} transport error: {raw_message}")
        }
        UpstreamFailureClass::Unknown => format!("{provider} API returned {status}: {raw_message}"),
    };

    let mut diagnostics = serde_json::json!({
        "provider": provider,
        "status": status.as_u16(),
        "raw_message": raw_message,
        "body": summarize_upstream_body(body_text),
    });
    if let Some(error_type) = upstream_error_type {
        diagnostics["error_type"] = serde_json::json!(error_type);
    }
    if let Some(request_id) = upstream_request_id {
        diagnostics["request_id"] = serde_json::json!(request_id);
    }
    if let Some(retry_after_ms) = retry_after_ms {
        diagnostics["retry_after_ms"] = serde_json::json!(retry_after_ms);
    }

    UpstreamFailureError::new(class, summary, diagnostics).into()
}

#[allow(dead_code)]
fn classify_http_failure(
    status: reqwest::StatusCode,
    provider: &str,
    body_text: &str,
) -> anyhow::Error {
    classify_http_failure_with_retry_after(status, provider, body_text, None)
}

pub(crate) fn transport_incompatibility_error(
    provider: &str,
    details: impl Into<String>,
) -> anyhow::Error {
    let details = details.into();
    UpstreamFailureError::new(
        UpstreamFailureClass::TransportIncompatible,
        format!(
            "The selected provider/transport combination is incompatible for {provider}: {details}"
        ),
        serde_json::json!({
            "provider": provider,
            "details": details,
        }),
    )
    .into()
}

pub(crate) fn classify_openai_responses_stream_failure(
    provider: &str,
    event_type: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
    diagnostics: serde_json::Value,
) -> anyhow::Error {
    let lower_code = error_code.unwrap_or_default().to_ascii_lowercase();
    let lower_message = error_message.unwrap_or_default().to_ascii_lowercase();
    let stale_tool_output_mismatch = lower_message.contains("no tool call found")
        && lower_message.contains("function call output");
    let context_window_exceeded_like = lower_code.contains("context_length_exceeded")
        || lower_code.contains("context_window")
        || lower_message.contains("context window")
        || lower_message.contains("context length")
        || (lower_message.contains("input")
            && lower_message.contains("exceeds")
            && lower_message.contains("context"));

    let class = if lower_code.contains("rate_limit") || lower_message.contains("rate limit") {
        UpstreamFailureClass::RateLimit
    } else if lower_code.contains("auth")
        || lower_code.contains("billing")
        || lower_code.contains("permission")
        || lower_message.contains("unauthorized")
        || lower_message.contains("forbidden")
        || lower_message.contains("authentication")
    {
        UpstreamFailureClass::AuthConfiguration
    } else if context_window_exceeded_like {
        UpstreamFailureClass::ContextWindowExceeded
    } else if lower_code.contains("invalid")
        || lower_code.contains("malformed")
        || lower_code.contains("request_too_large")
        || lower_message.contains("invalid request")
        || lower_message.contains("missing")
        || lower_message.contains("required")
        || stale_tool_output_mismatch
    {
        UpstreamFailureClass::RequestInvalid
    } else if lower_code.contains("server")
        || lower_code.contains("overloaded")
        || lower_code.contains("tempor")
        || lower_message.contains("over capacity")
        || lower_message.contains("try again later")
        || lower_message.contains("temporarily unavailable")
    {
        UpstreamFailureClass::TemporaryUpstream
    } else {
        UpstreamFailureClass::Unknown
    };

    let message = error_message.unwrap_or("Responses API stream error");
    let summary = match class {
        UpstreamFailureClass::RateLimit => {
            format!("{provider} Responses stream hit a rate limit: {message}")
        }
        UpstreamFailureClass::AuthConfiguration => auth_configuration_summary(provider, message),
        UpstreamFailureClass::RequestInvalid => {
            format!("{provider} Responses stream rejected the daemon request as invalid: {message}")
        }
        UpstreamFailureClass::ContextWindowExceeded => {
            format!("{provider} Responses stream input exceeds the model context window: {message}")
        }
        UpstreamFailureClass::TemporaryUpstream => {
            format!("{provider} Responses stream failed upstream: {message}")
        }
        UpstreamFailureClass::TransportIncompatible => format!(
            "The selected provider/transport combination is incompatible for {provider}: {message}"
        ),
        UpstreamFailureClass::TransientTransport => {
            format!("{provider} Responses stream transport error: {message}")
        }
        UpstreamFailureClass::Unknown => {
            format!("{provider} Responses stream {event_type} error: {message}")
        }
    };

    UpstreamFailureError::new(class, summary, diagnostics).into()
}

pub(crate) fn openai_responses_stream_parse_error(
    provider: &str,
    details: impl Into<String>,
    diagnostics: serde_json::Value,
) -> anyhow::Error {
    let details = details.into();
    UpstreamFailureError::new(
        UpstreamFailureClass::TransientTransport,
        format!("{provider} Responses stream parse error: {details}"),
        diagnostics,
    )
    .into()
}

pub(crate) fn upstream_failure_error(err: &anyhow::Error) -> Option<&UpstreamFailureError> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<UpstreamFailureError>())
}

pub(crate) fn is_timeout_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout() {
                return true;
            }
        }

        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            if io_error.kind() == std::io::ErrorKind::TimedOut {
                return true;
            }
        }
    }

    err.to_string().to_ascii_lowercase().contains("timed out")
}

pub(crate) fn is_transient_transport_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout()
                || reqwest_error.is_connect()
                || reqwest_error.is_request()
                || reqwest_error.is_body()
                || reqwest_error.is_decode()
            {
                return true;
            }
        }

        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;

            if matches!(
                io_error.kind(),
                ErrorKind::TimedOut
                    | ErrorKind::Interrupted
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::WouldBlock
            ) {
                return true;
            }
        }
    }

    let message = err.to_string().to_ascii_lowercase();
    message.contains("error sending request for url")
        || message.contains("connection reset")
        || message.contains("connection refused")
        || message.contains("timed out")
        || message.contains("unexpected eof")
}

pub(crate) fn summarize_transport_error(err: &anyhow::Error) -> String {
    let chain = err.chain().map(ToString::to_string).collect::<Vec<_>>();
    if chain.is_empty() {
        "unknown transport error".to_string()
    } else {
        chain.join(": ")
    }
}

pub(crate) fn is_temporary_upstream_error(err: &anyhow::Error) -> bool {
    if upstream_failure_error(err)
        .map(|failure| failure.class == UpstreamFailureClass::TemporaryUpstream)
        .unwrap_or(false)
    {
        return true;
    }
    let message = err.to_string().to_ascii_lowercase();
    message.contains(" 408")
        || message.contains(" 409")
        || message.contains(" 425")
        || message.contains(" 500")
        || message.contains(" 502")
        || message.contains(" 503")
        || message.contains(" 504")
        || message.contains("service unavailable")
        || message.contains("temporarily unavailable")
        || message.contains("server overloaded")
        || message.contains("overloaded")
        || message.contains("try again later")
}

impl Stream for CompletionStream {
    type Item = Result<CompletionChunk>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub(crate) fn has_openai_chatgpt_subscription_auth() -> bool {
    super::super::openai_codex_auth::has_openai_chatgpt_subscription_auth()
}

async fn refresh_openai_codex_auth(
    client: &reqwest::Client,
    auth: &StoredOpenAICodexAuth,
) -> Result<StoredOpenAICodexAuth> {
    let response = client
        .post(OPENAI_CODEX_AUTH_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", auth.refresh_token.as_str()),
            ("client_id", OPENAI_CODEX_AUTH_CLIENT_ID),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI token refresh failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let access_token = payload
        .get("access_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no access_token")?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no refresh_token")?
        .to_string();
    let account_id = extract_openai_codex_account_id(&access_token)
        .context("OpenAI token refresh returned no ChatGPT account id")?;
    let expires_in_ms = payload
        .get("expires_in")
        .and_then(|value| value.as_i64())
        .unwrap_or(3600)
        .saturating_mul(1000);
    let now = now_millis() as i64;
    let refreshed = StoredOpenAICodexAuth {
        provider: Some(OPENAI_CODEX_AUTH_PROVIDER.to_string()),
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        access_token,
        refresh_token,
        account_id: Some(account_id),
        expires_at: Some(now.saturating_add(expires_in_ms)),
        source: auth.source.clone().or_else(|| Some("zorai".to_string())),
        updated_at: Some(now),
        created_at: auth.created_at.or(Some(now)),
    };
    write_stored_openai_codex_auth(&refreshed)?;
    Ok(refreshed)
}

pub(crate) async fn resolve_openai_codex_request_auth(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
) -> Result<Option<OpenAICodexRequestAuth>> {
    if provider != zorai_shared::providers::PROVIDER_ID_OPENAI
        || config.auth_source != AuthSource::ChatgptSubscription
    {
        return Ok(None);
    }

    let auth = match read_stored_openai_codex_auth() {
        Some(auth) => Some(auth),
        None => import_codex_cli_auth_if_present()?,
    };
    let mut auth = auth.context(
        "No ChatGPT subscription auth found. Authenticate in the frontend or import ~/.codex/auth.json.",
    )?;
    let now = now_millis() as i64;
    if auth.expires_at.unwrap_or(0) <= now.saturating_add(60_000) {
        auth = refresh_openai_codex_auth(client, &auth).await?;
    }

    let account_id = auth
        .account_id
        .clone()
        .or_else(|| extract_openai_codex_account_id(&auth.access_token))
        .context("ChatGPT subscription auth is missing chatgpt_account_id")?;

    Ok(Some(OpenAICodexRequestAuth {
        access_token: auth.access_token,
        account_id,
    }))
}
