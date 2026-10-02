//! LLM API client with SSE streaming support.
//!
//! Supports two API formats:
//! - OpenAI-compatible (`/chat/completions` with Bearer auth) — covers most providers
//! - Anthropic Messages API (`/v1/messages` with `x-api-key` header)

mod anthropic;
mod anthropic_batches;
mod anthropic_request_fields;
mod anthropic_response_types;
mod anthropic_stream_message_start;
mod anthropic_stream_stop;
mod anthropic_stream_upstream_message;
mod anthropic_stream_usage;
mod api_types;
mod claude_code_cli;
mod cursor_cli;
mod helpers;
mod native_assistant;
mod openai_responses_protocol;
mod openai_runtime;
mod openai_sse;
mod openai_transport;
mod prelude;
mod public_api;

pub(crate) use anthropic::*;
#[allow(unused_imports)]
pub(crate) use anthropic_batches::*;
pub(crate) use anthropic_request_fields::*;
pub(crate) use anthropic_response_types::*;
pub(crate) use anthropic_stream_message_start::*;
pub(crate) use anthropic_stream_stop::*;
pub(crate) use anthropic_stream_upstream_message::*;
pub(crate) use anthropic_stream_usage::*;
pub(crate) use api_types::*;
pub(crate) use claude_code_cli::*;
pub(crate) use cursor_cli::*;
pub(crate) use helpers::*;
pub(crate) use native_assistant::*;
pub(crate) use openai_responses_protocol::*;
pub(crate) use openai_runtime::*;
pub(crate) use openai_sse::*;
pub(crate) use openai_transport::*;
pub(crate) use prelude::*;
pub(crate) use public_api::*;

#[cfg(test)]
mod tests;
