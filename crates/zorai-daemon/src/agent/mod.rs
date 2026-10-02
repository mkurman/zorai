//! Always-on autonomous agent engine.
//!
//! The agent lives in the daemon process and handles:
//! - LLM inference with streaming (OpenAI-compatible + Anthropic)
//! - Tool execution via SessionManager
//! - Persistent task queue with automatic processing
//! - Heartbeat system for periodic checks
//! - Persistent identity (SOUL.md, MEMORY.md, USER.md)

pub mod external_runner;
pub mod gateway;
pub mod llm_client;
pub mod tool_executor;
pub mod types;

pub(crate) mod agent_identity;
mod agent_loop;
mod agent_wakeup;
mod aline_startup;
mod anticipatory;
mod anticipatory_support;
mod authorship;
mod autonomy;
pub(crate) mod background_workers;
mod behavioral_events;
pub(crate) mod capability_tier;
mod causal_traces;
mod circuit_breaker;
pub(crate) mod claude_code_auth;
mod cognitive_resonance_runtime;
mod collaboration;
mod compaction;
mod config;
mod consensus;
mod consolidation;
pub(crate) mod copilot_auth;
mod critique;
pub(crate) mod cursor_auth;
mod debate;
mod dispatcher;
mod dream_state_runtime;
mod embedding_indexer;
mod emergent_protocol;
mod engine;
mod engine_runtime;
mod events;
mod explainability;
mod explanation;
mod external_messaging;
pub(crate) mod external_runtime_import;
mod forge;
mod gateway_health;
mod gateway_loop;
pub(crate) mod gene_pool;
mod goal_llm;
mod goal_parsing;
mod goal_review;
#[cfg(test)]
mod goal_routing;
mod goal_run_health;
mod goal_supervision_routing;
mod goal_support;
pub(crate) mod harness;
mod heartbeat;
mod heartbeat_checks;
mod honcho;
mod import_report;
mod internal_event;
mod memory;
mod memory_context;
mod memory_distillation;
mod memory_flush;
mod memory_palace;
mod messaging;
mod metadata;
pub(crate) mod mlflow_tracing;
pub(crate) mod morphogenesis;
mod notifications;
pub(crate) mod openai_codex_auth;
mod operation_wakeup;
mod operational_context;
mod operator_model;
mod operator_questions;
mod orchestrator_policy;
mod persistence;
mod prompt_inspection;
mod prompt_queue;
mod provenance;
mod provider_auth_store;
mod provider_preflight;
mod provider_resolution;
pub mod rate_limiter;
mod resonance;
mod routines;
mod runtime_continuity;
mod semantic_env;
mod session_recall;
pub(crate) mod skill_community;
mod skill_discovery;
mod skill_evolution;
pub(crate) mod skill_mesh;
pub(crate) mod skill_preflight;
mod skill_recommendation;
pub(crate) mod skill_registry;
mod skill_security;
mod stalled_turns;
mod system_prompt;
mod task_crud;
mod task_prompt;
mod task_scheduler;
mod task_worktree;
mod temporal_foresight_runtime;
pub(crate) mod terminal_leases;
pub(crate) mod thread_crud;
mod thread_handoffs;
mod thread_participant_runner;
pub(crate) mod thread_participants;
mod thread_pins;
mod thread_title;
mod tool_synthesis;
mod webhook_listener;
pub(crate) mod weles_governance;
mod weles_health;
mod whatsapp_link;
mod whatsapp_native;
mod work_context;
mod workflow_packs;
mod workspace;
mod workspace_control;
#[cfg(test)]
mod workspace_control_tests;
#[cfg(test)]
mod workspace_event_tests;
mod workspace_events;
mod workspace_goal;
#[cfg(test)]
mod workspace_operator_tests;
mod workspace_reconcile;
mod workspace_review;
mod workspace_runtime;
mod workspace_support;
#[cfg(test)]
mod workspace_tests;
mod workspace_thread;

pub mod awareness;
pub mod concierge;
pub mod context;
pub mod cost;
pub mod embodied;
pub mod episodic;
pub mod handoff;
pub mod learning;
pub mod liveness;
pub mod metacognitive;
pub mod operator_profile;
pub mod subagent;
pub mod uncertainty;

use agent_identity::*;
use anticipatory::*;
use anticipatory_support::*;
pub(crate) use authorship::AuthorshipTag;
pub(crate) use autonomy::AutonomyLevel;
use behavioral_events::*;
use compaction::*;
pub(crate) use config::ConfigRuntimeProjection;
pub(crate) use explanation::*;
pub(crate) use gateway_health::GatewayConnectionStatus as RuntimeGatewayConnectionStatus;
use goal_parsing::*;
pub(in crate::agent) use goal_review::{
    parse_goal_supervisor_verdict, AWAITING_SUPERVISOR_BLOCKED_PREFIX,
};
use honcho::*;
use memory::*;
use metadata::*;
use operator_model::*;
use operator_questions::*;
use provider_resolution::*;
use runtime_continuity::*;
use system_prompt::*;
use task_prompt::*;
use task_scheduler::*;
use thread_handoffs::*;
pub(crate) use thread_participants::*;
use thread_pins::*;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::history::HistoryStore;
use crate::session_manager::SessionManager;

use self::llm_client::{send_completion_request, ApiContent, ApiMessage, RetryStrategy};
use self::tool_executor::{execute_tool, get_available_tools};
use self::types::*;

fn goal_run_apply_thread_routing(goal_run: &mut GoalRun, thread_id: Option<String>) {
    let Some(thread_id) = thread_id.map(|value| value.trim().to_string()) else {
        return;
    };
    if thread_id.is_empty() {
        return;
    }
    if goal_run.root_thread_id.is_none() {
        goal_run.root_thread_id = Some(thread_id.clone());
    }
    goal_run.execution_thread_ids = vec![thread_id.clone()];
    goal_run.active_thread_id = Some(thread_id.clone());
    goal_run.thread_id = Some(thread_id);
}

#[cfg(test)]
pub(crate) use aline_startup::AlineStartupShortCircuitReason;
pub(crate) use aline_startup::{
    run_aline_startup_worker_from_stdio, WatcherState, ALINE_STARTUP_WORKER_ARG,
};
pub(crate) use config::canonicalize_weles_client_update;
#[cfg(test)]
pub(crate) use config::{ConfigEffectiveRuntimeState, ConfigReconcileState};
#[allow(unused_imports)]
pub use engine::*;
#[cfg(test)]
pub(crate) use provider_auth_store::provider_auth_test_env_lock;
pub use task_prompt::load_config_from_history;
#[cfg(test)]
pub(crate) use whatsapp_link::transport::PersistedState as WhatsAppPersistedState;
pub(crate) use whatsapp_link::{
    clear_persisted_provider_state, persist_transport_session_update, WHATSAPP_LINK_PROVIDER_ID,
};
#[cfg(test)]
pub(crate) use whatsapp_link::{load_persisted_provider_state, save_persisted_provider_state};
#[allow(unused_imports)]
pub(crate) use whatsapp_native::{
    disconnect_native_whatsapp_client, send_native_whatsapp_message, start_whatsapp_link_native,
    whatsapp_native_store_path,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
