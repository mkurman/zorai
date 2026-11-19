use super::*;
use std::fs;
use uuid::Uuid;

async fn make_test_store() -> Result<(HistoryStore, PathBuf)> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let store = HistoryStore::new_test_store(&root).await?;
    Ok((store, root))
}

mod agent_wakeups;
mod browser_profiles;
mod cognitive_resonance;
mod command_log;
mod database_viewer;
mod dream_state;
mod elastic_context;
mod embedding_queue;
mod emergent_protocol;
mod event_log;
mod event_triggers;
mod external_runtime_profiles;
mod goal_runs;
mod governance;
mod harness;
mod metacognition;
mod misc;
mod notifications;
mod prompt_queue;
mod provider_auth;
mod routine_definitions;
mod semantic_documents;
mod skill_variants;
mod sqlite_audit;
mod statistics;
mod tasks;
mod temporal_foresight;
mod tombstones_gateway;
mod ui_colors;
#[cfg(feature = "lancedb-vector")]
mod vector_index;
mod workspaces;
