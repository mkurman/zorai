pub(crate) use anyhow::{Context, Result};
pub(crate) use futures::Stream;
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use std::collections::HashMap;
pub(crate) use std::fmt;
pub(crate) use std::pin::Pin;
pub(crate) use std::sync::Arc;
pub(crate) use std::task::Poll;
pub(crate) use std::time::Duration;
pub(crate) use tokio::sync::mpsc;

pub(crate) use crate::agent::task_prompt::now_millis;
pub(crate) use crate::agent::types::{
    get_provider_api_type, get_provider_definition,
    reload_custom_provider_catalog_from_default_path, ApiTransport, ApiType, AuthMethod,
    AuthSource, CompletionChunk, ProviderConfig, ToolCall, ToolDefinition, ToolFunction,
};

pub(crate) const UPSTREAM_DIAGNOSTICS_MARKER: &str = "\n\n[zorai-upstream-diagnostics]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamFailureClass {
    RequestInvalid,
    ContextWindowExceeded,
    AuthConfiguration,
    TransportIncompatible,
    TemporaryUpstream,
    TransientTransport,
    RateLimit,
    Unknown,
}

impl UpstreamFailureClass {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RequestInvalid => "request_invalid",
            Self::ContextWindowExceeded => "context_window_exceeded",
            Self::AuthConfiguration => "auth_configuration",
            Self::TransportIncompatible => "transport_incompatible",
            Self::TemporaryUpstream => "temporary_upstream",
            Self::TransientTransport => "transient_transport",
            Self::RateLimit => "rate_limit",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StructuredUpstreamFailure {
    pub class: String,
    pub summary: String,
    pub diagnostics: serde_json::Value,
}

#[derive(Debug)]
pub(crate) struct UpstreamFailureError {
    pub(crate) class: UpstreamFailureClass,
    pub(crate) summary: String,
    pub(crate) diagnostics: serde_json::Value,
}

impl UpstreamFailureError {
    pub(crate) fn new(
        class: UpstreamFailureClass,
        summary: impl Into<String>,
        diagnostics: serde_json::Value,
    ) -> Self {
        Self {
            class,
            summary: summary.into(),
            diagnostics,
        }
    }

    pub(crate) fn structured(&self) -> StructuredUpstreamFailure {
        StructuredUpstreamFailure {
            class: self.class.as_str().to_string(),
            summary: self.summary.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub(crate) fn operator_message(&self) -> String {
        format!(
            "{}{}{}",
            self.summary,
            UPSTREAM_DIAGNOSTICS_MARKER,
            serde_json::to_string(&self.structured())
                .unwrap_or_else(|_| "{\"class\":\"unknown\"}".to_string())
        )
    }
}

impl fmt::Display for UpstreamFailureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.summary)
    }
}

impl std::error::Error for UpstreamFailureError {}

pub(crate) fn is_exhausted_provider_usage_limit(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("usage limit has been reached")
        || lower.contains("usage limit reached")
        || lower.contains("insufficient_quota")
        || lower.contains("insufficient quota")
        || lower.contains("quota exceeded")
        || lower.contains("quota exhausted")
        || lower.contains("billing hard limit")
        || lower.contains("exceeded your current quota")
        || lower.contains("weekly/monthly limit exhausted")
}

pub(crate) fn parse_structured_upstream_failure(
    message: &str,
) -> Option<StructuredUpstreamFailure> {
    let (_, diagnostics) = message.split_once(UPSTREAM_DIAGNOSTICS_MARKER)?;
    serde_json::from_str(diagnostics).ok()
}

pub(crate) fn sanitize_upstream_failure_message(message: &str) -> String {
    parse_structured_upstream_failure(message)
        .map(|structured| structured.summary)
        .unwrap_or_else(|| message.to_string())
}

pub(crate) fn normalize_openai_usage_tokens(
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: Option<u64>,
) -> (u64, u64) {
    let output_tokens = total_tokens
        .and_then(|total| total.checked_sub(input_tokens))
        .filter(|total_derived_output| *total_derived_output > output_tokens)
        .unwrap_or(output_tokens);
    (input_tokens, output_tokens)
}

#[derive(Debug, Clone, Copy)]
pub enum RetryStrategy {
    Bounded {
        max_retries: u32,
        retry_delay_ms: u64,
    },
    DurableRateLimited,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: ApiContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ApiContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ApiToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

pub struct CompletionStream {
    pub(crate) rx: mpsc::Receiver<Result<CompletionChunk>>,
}

#[derive(Debug)]
pub(crate) struct RateLimitError {
    pub(crate) provider: String,
    pub(crate) details: String,
}

impl fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} API returned 429: {}", self.provider, self.details)
    }
}

impl std::error::Error for RateLimitError {}

#[derive(Debug, Clone)]
pub(crate) struct OpenAICodexRequestAuth {
    pub(crate) access_token: String,
    pub(crate) account_id: String,
}

pub(crate) fn summarize_upstream_body(body_text: &str) -> String {
    let trimmed = body_text.trim();
    if trimmed.is_empty() {
        "upstream returned an empty error body".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn raw_upstream_message(body_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body_text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/metadata/raw")
                .and_then(|value| value.as_str())
                .or_else(|| {
                    value
                        .pointer("/metadata/raw")
                        .and_then(|value| value.as_str())
                })
                .or_else(|| {
                    value
                        .pointer("/error/message")
                        .and_then(|value| value.as_str())
                })
                .or_else(|| value.get("message").and_then(|value| value.as_str()))
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| summarize_upstream_body(body_text))
}
