use super::persist_task_update;
use crate::agent::types::AgentConfig;
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::tempdir;

pub(super) use super::super::{now_millis, task_by_id_for_tool_scope, AgentEngine, AgentTask};
pub(super) use super::{
    ask_timeout_action, execute_answer_child, execute_ask_parent, execute_note_to_child,
    execute_retry_subagent,
    task_is_awaiting_parent, AskParentRecord, AskTimeoutAction, AWAITING_PARENT_BLOCKED_PREFIX,
};
pub(super) use crate::agent::types::TaskStatus;

mod ask;
mod notes;
mod retry;
mod timeout;
mod unit;

pub(super) async fn setup() -> (tempfile::TempDir, Arc<AgentEngine>) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    (root, engine)
}

pub(super) async fn spawn_child(engine: &AgentEngine) -> AgentTask {
    let parent = engine
        .enqueue_task(
            "Parent".into(),
            "coordination".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent".into()),
            None,
        )
        .await;
    let mut child = engine
        .enqueue_task(
            "Child".into(),
            "work".into(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent".into()),
            None,
        )
        .await;
    child.thread_id = Some("thread-child".into());
    persist_task_update(engine, &child, None)
        .await
        .expect("persist child");
    child
}

pub(super) fn ask_args(question: &str) -> serde_json::Value {
    serde_json::json!({ "question": question })
}
