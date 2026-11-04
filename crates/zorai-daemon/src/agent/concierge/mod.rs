//! Concierge agent — proactive welcome greetings and lightweight ops assistant.

use super::circuit_breaker::CircuitBreakerRegistry;
use super::llm_client::{
    self, ApiContent, ApiMessage, ApiToolCall, ApiToolCallFunction, RetryStrategy,
};
use super::provider_resolution::resolve_provider_config_for;
use super::types::*;
use super::{execute_tool, get_available_tools, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME};
use anyhow::Result;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

mod context;
mod onboarding;
mod profile;
mod runtime;
mod state;
mod triage;
mod welcome;

pub const CONCIERGE_THREAD_ID: &str = "concierge";
const WELCOME_REUSE_WINDOW_MS: u64 = 60 * 60 * 1000;
const GATEWAY_TRIAGE_MAX_TOOL_ROUNDS: usize = 3;
pub(in crate::agent) const CONCIERGE_CHAT_MAX_TOOL_ROUNDS: usize = 3;
const GATEWAY_TRIAGE_SAFE_TOOL_NAMES: &[&str] = &[
    zorai_protocol::tool_names::SEARCH_HISTORY,
    zorai_protocol::tool_names::FETCH_GATEWAY_HISTORY,
    zorai_protocol::tool_names::MESSAGE_AGENT,
    zorai_protocol::tool_names::SESSION_SEARCH,
    zorai_protocol::tool_names::AGENT_QUERY_MEMORY,
    zorai_protocol::tool_names::ONECONTEXT_SEARCH,
    zorai_protocol::tool_names::LIST_GUIDELINES,
    zorai_protocol::tool_names::DISCOVER_GUIDELINES,
    zorai_protocol::tool_names::READ_GUIDELINE,
    zorai_protocol::tool_names::LIST_SKILLS,
    zorai_protocol::tool_names::SEMANTIC_QUERY,
    zorai_protocol::tool_names::DISCOVER_SKILLS,
    zorai_protocol::tool_names::READ_SKILL,
    zorai_protocol::tool_names::WEB_SEARCH,
];

pub enum GatewayTriage {
    Simple(String),
    Complex,
}

pub struct ConciergeEngine {
    config: Arc<RwLock<AgentConfig>>,
    event_tx: broadcast::Sender<AgentEvent>,
    http_client: reqwest::Client,
    circuit_breakers: Arc<CircuitBreakerRegistry>,
    welcome_cache: Arc<RwLock<Option<WelcomeCacheEntry>>>,
    recovery_investigations: Arc<Mutex<HashMap<String, String>>>,
}

#[allow(unused_imports)]
pub(crate) use context::*;
#[allow(unused_imports)]
pub(super) use onboarding::*;
pub use profile::*;
#[allow(unused_imports)]
pub(super) use runtime::*;
#[allow(unused_imports)]
pub(super) use state::*;
#[allow(unused_imports)]
pub(super) use triage::*;
#[allow(unused_imports)]
pub(super) use welcome::*;

#[cfg(test)]
mod tests;
