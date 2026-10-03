#![allow(dead_code)]

//! Context compaction — token-aware message compression for LLM requests.

use super::llm_client::{messages_to_api_format, ApiToolCall, ApiToolCallFunction};
use super::*;
use crate::agent::context::structural_memory::{StructuralContextEntry, ThreadStructuralMemory};
use crate::history::MemoryGraphNeighborRow;
use zorai_shared::providers::{PROVIDER_ID_GEMINI, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

mod artifact;
mod candidate;
mod checkpoint;
mod core;
mod llm_compaction;
mod persistence;
mod request;
mod rule_based;

#[cfg(test)]
pub(crate) use artifact::CompactionLlmFailureWithCapacity;
pub(crate) use candidate::*;
pub(crate) use checkpoint::*;
pub(crate) use core::*;
pub(crate) use llm_compaction::*;
pub(crate) use request::*;
pub(crate) use rule_based::*;

#[cfg(test)]
#[path = "compaction/tests.rs"]
mod tests;
