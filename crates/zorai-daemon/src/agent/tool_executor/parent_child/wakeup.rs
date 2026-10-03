use super::super::{task_by_id_for_tool_scope, AgentEngine, AgentTask};
use crate::agent::types::DeferredVisibleThreadContinuation;

async fn resolve_parent_thread_id(agent: &AgentEngine, child: &AgentTask) -> Option<String> {
    if let Some(thread_id) = child
        .parent_thread_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(thread_id.to_string());
    }
    if let Some(parent_task_id) = child.parent_task_id.as_deref() {
        return task_by_id_for_tool_scope(agent, parent_task_id)
            .await
            .and_then(|parent| parent.thread_id);
    }
    None
}

pub(super) async fn wake_parent_for_ask(
    agent: &AgentEngine,
    child: &AgentTask,
    ask_id: &str,
    ask_slug: &str,
    child_slug: &str,
    question: &str,
    options: &[String],
) {
    let Some(parent_thread_id) = resolve_parent_thread_id(agent, child).await else {
        tracing::warn!(
            child_task_id = %child.id,
            "ask_parent blocked the child but no parent thread exists to wake"
        );
        return;
    };
    let options_block = if options.is_empty() {
        String::new()
    } else {
        let listed = options
            .iter()
            .map(|option| format!("- {option}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("\nOptions:\n{listed}")
    };
    let content = format!(
        "Child `{child_slug}` asked a blocking question and is paused until you answer.\n\n\
         ask: `{ask_slug}`\n\
         Question: {question}{options_block}\n\n\
         Call `answer_child` with child_task_id `{child_slug}`, ask_id `{ask_slug}`, and your answer. \
         If multiple asks are open, answer each ask separately; the child stays blocked until every open ask is resolved."
    );
    let _ = agent
        .append_system_thread_message(&parent_thread_id, content.clone())
        .await;
    agent.emit_workflow_notice(
        &parent_thread_id,
        "child-ask-parent",
        format!("Child {child_slug} is awaiting an answer."),
        Some(
            serde_json::json!({
                "child_task_id": child.id,
                "child_slug": child_slug,
                "ask_id": ask_id,
                "ask_slug": ask_slug,
            })
            .to_string(),
        ),
    );
    let agent_id = agent
        .agent_scope_id_for_turn(
            Some(parent_thread_id.as_str()),
            child.parent_task_id.as_deref(),
        )
        .await;
    agent
        .enqueue_visible_thread_continuation(
            &parent_thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id: child.parent_task_id.clone(),
                preferred_session_hint: None,
                llm_user_content: content,
                queued_at_ms: 0,
                force_compaction: false,
                rerun_participant_observers_after_turn: false,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;
    if agent
        .thread_is_idle_for_subagent_wakeup(&parent_thread_id)
        .await
    {
        let _ = agent.stop_stream(&parent_thread_id).await;
    }
    if let Err(error) = agent
        .flush_deferred_visible_thread_continuations(&parent_thread_id)
        .await
    {
        tracing::warn!(
            thread_id = %parent_thread_id,
            child_task_id = %child.id,
            %error,
            "failed to flush parent continuation after ask_parent"
        );
    }
}

pub(super) async fn wake_child_thread(
    agent: &AgentEngine,
    child: &AgentTask,
    llm_user_content: String,
) {
    let Some(child_thread_id) = child.thread_id.as_deref() else {
        return;
    };
    let agent_id = agent
        .agent_scope_id_for_turn(Some(child_thread_id), Some(child.id.as_str()))
        .await;
    agent
        .enqueue_visible_thread_continuation(
            child_thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id: Some(child.id.clone()),
                preferred_session_hint: None,
                llm_user_content,
                queued_at_ms: 0,
                force_compaction: false,
                rerun_participant_observers_after_turn: false,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;
    if let Err(error) = agent
        .flush_deferred_visible_thread_continuations(child_thread_id)
        .await
    {
        tracing::warn!(
            thread_id = %child_thread_id,
            task_id = %child.id,
            %error,
            "failed to flush child continuation after parent-child messaging resolution"
        );
    }
}
