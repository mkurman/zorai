use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

async fn test_engine() -> Arc<AgentEngine> {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await
}

async fn thread_with_reply(
    engine: &AgentEngine,
    thread_id: &str,
    title: &str,
    reply: &str,
) -> String {
    let (thread_id, _) = engine
        .get_or_create_thread_with_target(Some(thread_id), title, None)
        .await;
    let message = AgentMessage {
        role: MessageRole::Assistant,
        author_agent_name: Some("Svarog".to_string()),
        ..AgentMessage::user(reply, 1)
    };
    let message_id = message.id.clone();
    engine
        .threads
        .write()
        .await
        .get_mut(&thread_id)
        .expect("thread")
        .messages
        .push(message);
    message_id
}

fn done(thread_id: &str, message_id: &str) -> AgentEvent {
    AgentEvent::Done {
        thread_id: thread_id.to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        upstream_message: None,
        provider_final_result: None,
        message_id: Some(message_id.to_string()),
    }
}

#[tokio::test]
async fn operator_thread_reply_notifies_with_author_title_and_text() {
    let engine = test_engine().await;
    let message_id =
        thread_with_reply(&engine, "thread-op", "Release plan", "The build is green.").await;

    let notification = engine
        .desktop_notification_for_event(&done("thread-op", &message_id))
        .await
        .expect("operator thread reply should notify");

    assert_eq!(notification.title, "Svarog · Release plan");
    assert_eq!(notification.body, "The build is green.");
}

#[tokio::test]
async fn background_work_replies_do_not_notify_so_goal_and_subagent_runs_stay_quiet() {
    let engine = test_engine().await;
    let message_id = thread_with_reply(&engine, "thread-task", "Worker", "Step done.").await;
    let mut task = engine
        .enqueue_task(
            "Worker".to_string(),
            "do it".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            None,
            None,
            None,
        )
        .await;
    task.thread_id = Some("thread-task".to_string());
    engine.upsert_live_task(&task).await;
    let dm_message_id = thread_with_reply(&engine, "dm:svarog:weles", "Internal DM", "ack").await;

    assert!(engine
        .desktop_notification_for_event(&done("thread-task", &message_id))
        .await
        .is_none());
    assert!(engine
        .desktop_notification_for_event(&done("dm:svarog:weles", &dm_message_id))
        .await
        .is_none());
}

#[tokio::test]
async fn tool_only_turn_without_text_does_not_notify() {
    let engine = test_engine().await;
    let message_id = thread_with_reply(&engine, "thread-op", "Release plan", "   ").await;

    assert!(engine
        .desktop_notification_for_event(&done("thread-op", &message_id))
        .await
        .is_none());
}

#[tokio::test]
async fn approvals_and_questions_notify_from_any_thread_because_they_block_work() {
    let engine = test_engine().await;
    thread_with_reply(&engine, "dm:svarog:weles", "Internal DM", "ack").await;

    let approval = engine
        .desktop_notification_for_event(&AgentEvent::ApprovalRequired {
            thread_id: "dm:svarog:weles".to_string(),
            approval_id: "approval-1".to_string(),
            command: "rm -rf build".to_string(),
            rationale: None,
            reasons: Vec::new(),
            risk_level: "high".to_string(),
            blast_radius: "workspace".to_string(),
        })
        .await
        .expect("approval should notify");
    assert_eq!(approval.title, "Approval needed (high) · Internal DM");
    assert_eq!(approval.body, "rm -rf build");

    let question = engine
        .desktop_notification_for_event(&AgentEvent::OperatorQuestion {
            question_id: "q-1".to_string(),
            content: "Ship it?".to_string(),
            options: vec!["yes".to_string(), "no".to_string()],
            session_id: None,
            thread_id: None,
        })
        .await
        .expect("question should notify");
    assert_eq!(question.title, "Question from an agent");
    assert_eq!(question.body, "Ship it?");
}
