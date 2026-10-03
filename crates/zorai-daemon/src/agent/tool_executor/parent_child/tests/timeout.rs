use super::super::{list_ask_records, persist_task_update};
use super::*;

#[tokio::test]
async fn timeout_sweep_defaults_and_unanswers_without_failing_the_child() {
    let (_root, engine) = setup().await;
    let with_default = spawn_child(&engine).await;
    let without_default = spawn_child(&engine).await;

    let now = now_millis();
    let seed = |_task_id: &str, default: Option<&str>| AskParentRecord {
        question: "Which endpoint?".to_string(),
        options: Vec::new(),
        asked_at: now.saturating_sub(10 * 60_000),
        timeout_minutes: 1,
        default: default.map(ToOwned::to_owned),
        state: "open".to_string(),
        answer: None,
        answer_delivered: false,
        slug: String::new(),
    };
    let defaulted_key = format!("ask_parent:{}:seed-defaulted", with_default.id);
    let unanswered_key = format!("ask_parent:{}:seed-unanswered", without_default.id);
    engine
        .history
        .set_consolidation_state(
            &defaulted_key,
            &serde_json::to_string(&seed(&with_default.id, Some("POST /v2"))).unwrap(),
            now,
        )
        .await
        .expect("seed defaulted ask");
    engine
        .history
        .set_consolidation_state(
            &unanswered_key,
            &serde_json::to_string(&seed(&without_default.id, None)).unwrap(),
            now,
        )
        .await
        .expect("seed unanswered ask");
    for target in [&with_default, &without_default] {
        let mut blocked = task_by_id_for_tool_scope(&engine, &target.id)
            .await
            .expect("child exists");
        blocked.status = TaskStatus::Blocked;
        blocked.blocked_reason = Some(format!(
            "{} Which endpoint?",
            AWAITING_PARENT_BLOCKED_PREFIX
        ));
        persist_task_update(&engine, &blocked, None)
            .await
            .expect("persist blocked child");
    }

    engine
        .sweep_ask_parent_timeouts()
        .await
        .expect("sweep succeeds");

    let (_, defaulted_record) = list_ask_records(&engine, &with_default.id)
        .await
        .expect("readable")
        .into_iter()
        .next()
        .expect("record present");
    assert_eq!(defaulted_record.state, "timeout_defaulted");
    assert_eq!(defaulted_record.answer.as_deref(), Some("POST /v2"));

    let (_, unanswered_record) = list_ask_records(&engine, &without_default.id)
        .await
        .expect("readable")
        .into_iter()
        .next()
        .expect("record present");
    assert_eq!(unanswered_record.state, "timeout_unanswered");

    for target in [&with_default, &without_default] {
        let swept = task_by_id_for_tool_scope(&engine, &target.id)
            .await
            .expect("child exists");
        assert_ne!(
            swept.status,
            TaskStatus::Blocked,
            "timeout must never leave or fail the child as Blocked/Failed"
        );
        assert_ne!(swept.status, TaskStatus::Failed);
        assert!(swept.blocked_reason.is_none());
    }

    let defaulted_context = engine
        .build_parent_child_prompt_context(Some(&with_default.id))
        .await
        .expect("default injected");
    assert!(defaulted_context.contains("[parent answer] POST /v2"));
    let unanswered_context = engine
        .build_parent_child_prompt_context(Some(&without_default.id))
        .await
        .expect("proceed-with-judgment note injected");
    assert!(unanswered_context.contains(
        "[ask timeout] no answer available; proceed with best judgment and state assumptions"
    ));
}

#[tokio::test]
async fn timeout_sweep_leaves_child_blocked_while_other_asks_are_still_open() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;
    let now = now_millis();
    let expired = AskParentRecord {
        question: "Expired?".to_string(),
        options: Vec::new(),
        asked_at: now.saturating_sub(10 * 60_000),
        timeout_minutes: 1,
        default: None,
        state: "open".to_string(),
        answer: None,
        answer_delivered: false,
        slug: String::new(),
    };
    let live = AskParentRecord {
        question: "Still waiting?".to_string(),
        options: Vec::new(),
        asked_at: now,
        timeout_minutes: 240,
        default: None,
        state: "open".to_string(),
        answer: None,
        answer_delivered: false,
        slug: String::new(),
    };
    engine
        .history
        .set_consolidation_state(
            &format!("ask_parent:{}:expired", child.id),
            &serde_json::to_string(&expired).unwrap(),
            now,
        )
        .await
        .expect("seed expired");
    engine
        .history
        .set_consolidation_state(
            &format!("ask_parent:{}:live", child.id),
            &serde_json::to_string(&live).unwrap(),
            now,
        )
        .await
        .expect("seed live");
    let mut blocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    blocked.status = TaskStatus::Blocked;
    blocked.blocked_reason = Some(format!("{} Still waiting?", AWAITING_PARENT_BLOCKED_PREFIX));
    persist_task_update(&engine, &blocked, None)
        .await
        .expect("persist blocked");

    engine
        .sweep_ask_parent_timeouts()
        .await
        .expect("sweep succeeds");

    let records = list_ask_records(&engine, &child.id).await.expect("records");
    let expired_state = records
        .iter()
        .find(|(key, _)| key.ends_with(":expired"))
        .map(|(_, record)| record.state.as_str());
    let live_state = records
        .iter()
        .find(|(key, _)| key.ends_with(":live"))
        .map(|(_, record)| record.state.as_str());
    assert_eq!(expired_state, Some("timeout_unanswered"));
    assert_eq!(live_state, Some("open"));
    let still_blocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_eq!(still_blocked.status, TaskStatus::Blocked);
}
