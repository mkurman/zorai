use super::super::{list_ask_records, persist_task_update, split_ask_key};
use super::*;

#[tokio::test]
async fn ask_parent_rejects_task_without_parent_and_leaves_task_unchanged() {
    let (_root, engine) = setup().await;
    let mut orphan = engine
        .enqueue_task(
            "Orphan".into(),
            "no parent here".into(),
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
    orphan.thread_id = Some("thread-orphan".into());
    persist_task_update(&engine, &orphan, None)
        .await
        .expect("persist orphan");

    let error = execute_ask_parent(
        &ask_args("Which schema should I target?"),
        &engine,
        "thread-orphan",
        Some(&orphan.id),
    )
    .await
    .expect_err("orphan ask_parent must be rejected");
    assert!(
        error.to_string().contains("no parent_task_id"),
        "actionable error expected, got: {error}"
    );
    let stored = task_by_id_for_tool_scope(&engine, &orphan.id)
        .await
        .expect("orphan still exists");
    assert_eq!(stored.status, TaskStatus::Queued, "task must be unchanged");
    let records = list_ask_records(&engine, &orphan.id)
        .await
        .expect("no records read failure");
    assert!(records.is_empty(), "no ask record may be written");
}

#[tokio::test]
async fn ask_parent_blocks_and_answers_unblock_with_verbatim_context_injection() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;

    let result = execute_ask_parent(
        &ask_args("Should I use plan A or plan B?"),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect("ask_parent succeeds for a parented child");

    assert!(result.contains("\"open\""));
    let blocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_eq!(blocked.status, TaskStatus::Blocked);
    assert_eq!(
        blocked.blocked_reason.as_deref(),
        Some("awaiting parent: Should I use plan A or plan B?")
    );
    let open: Vec<_> = list_ask_records(&engine, &child.id)
        .await
        .expect("records readable")
        .into_iter()
        .filter(|(_, record)| record.state == "open")
        .collect();
    assert_eq!(open.len(), 1, "one open ask recorded");
    assert!(blocked
        .logs
        .iter()
        .any(|log| log.phase == "parent_child_messaging"));

    let forbidden = execute_answer_child(
        &serde_json::json!({ "child_task_id": child.id, "answer": "plan B" }),
        &engine,
        "thread-someone-else",
        None,
    )
    .await
    .expect_err("non-parent answer must be rejected");
    assert!(forbidden.to_string().contains("only the child's parent"));

    execute_answer_child(
        &serde_json::json!({ "child_task_id": child.id, "answer": "Use plan B." }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("parent answers successfully");

    let unblocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_ne!(
        unblocked.status,
        TaskStatus::Blocked,
        "child must be unblocked"
    );
    assert!(unblocked.blocked_reason.is_none());
    let (_, answered) = list_ask_records(&engine, &child.id)
        .await
        .expect("records readable")
        .into_iter()
        .next()
        .expect("record still present");
    assert_eq!(answered.state, "answered");
    assert_eq!(answered.answer.as_deref(), Some("Use plan B."));

    let injected = engine
        .build_parent_child_prompt_context(Some(&child.id))
        .await
        .expect("answer block injected into next-turn context");
    assert!(
        injected.contains("[parent answer] Use plan B."),
        "verbatim answer expected, got: {injected}"
    );
    let replayed = engine
        .build_parent_child_prompt_context(Some(&child.id))
        .await;
    assert!(replayed.is_none(), "answer must be delivered exactly once");
}

#[tokio::test]
async fn sixth_open_ask_is_rejected_with_batching_guidance() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;

    for index in 0..5 {
        execute_ask_parent(
            &ask_args(&format!("Question number {index}?")),
            &engine,
            "thread-child",
            Some(&child.id),
        )
        .await
        .expect("first five asks are accepted");
    }
    let error = execute_ask_parent(
        &ask_args("One question too many?"),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect_err("sixth concurrent open ask must be rejected");
    let message = error.to_string();
    assert!(
        message.contains("batch"),
        "batching guidance expected, got: {message}"
    );
}

#[tokio::test]
async fn ask_parent_enqueues_a_parent_wakeup_so_the_parent_can_answer() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;
    engine.begin_stream_cancellation("thread-parent").await;

    let result = execute_ask_parent(
        &serde_json::json!({
            "question": "Should I use plan A or plan B?",
            "options": ["plan A", "plan B"],
        }),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect("ask_parent succeeds");
    let parsed: serde_json::Value = serde_json::from_str(&result).expect("json result");
    let ask_id = parsed["ask_id"].as_str().expect("ask_id returned");
    let ask_slug = parsed["ask_slug"].as_str().expect("ask_slug returned");
    assert!(!ask_id.is_empty());
    assert_eq!(ask_slug, "q1");

    let queued = engine
        .deferred_visible_thread_continuations_for("thread-parent")
        .await;
    assert_eq!(
        queued.len(),
        1,
        "parent must get a continuation so it can call answer_child instead of waiting for timeout"
    );
    assert!(queued[0].llm_user_content.contains("answer_child"));
    assert!(queued[0].llm_user_content.contains("child_task_id `child`"));
    assert!(queued[0].llm_user_content.contains("ask_id `q1`"));
    assert!(
        !queued[0].llm_user_content.contains(ask_id),
        "the parent prompt must not make the model copy the ask uuid"
    );
    assert!(
        !queued[0].llm_user_content.contains(&child.id),
        "the parent prompt must not make the model copy the task uuid"
    );
    assert!(queued[0]
        .llm_user_content
        .contains("Should I use plan A or plan B?"));
    assert_eq!(
        queued[0].task_id.as_deref(),
        child.parent_task_id.as_deref()
    );
}

#[tokio::test]
async fn answer_child_targets_one_open_ask_and_keeps_the_child_blocked_until_all_are_resolved() {
    let (_root, engine) = setup().await;
    let child = spawn_child(&engine).await;

    let first = execute_ask_parent(
        &ask_args("First?"),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect("first ask");
    let second = execute_ask_parent(
        &ask_args("Second?"),
        &engine,
        "thread-child",
        Some(&child.id),
    )
    .await
    .expect("second ask");
    let first_id = serde_json::from_str::<serde_json::Value>(&first).expect("json")["ask_id"]
        .as_str()
        .expect("ask_id")
        .to_string();
    let second_id = serde_json::from_str::<serde_json::Value>(&second).expect("json")["ask_id"]
        .as_str()
        .expect("ask_id")
        .to_string();

    let ambiguous = execute_answer_child(
        &serde_json::json!({ "child_task_id": child.id, "answer": "one answer for both" }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect_err("multiple open asks require ask_id");
    assert!(
        ambiguous.to_string().contains("ask_id"),
        "actionable ask_id guidance expected, got: {ambiguous}"
    );

    execute_answer_child(
        &serde_json::json!({
            "child_task_id": child.id,
            "ask_id": first_id,
            "answer": "answer first",
        }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("first ask answered");

    let still_blocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_eq!(still_blocked.status, TaskStatus::Blocked);
    let open: Vec<_> = list_ask_records(&engine, &child.id)
        .await
        .expect("records")
        .into_iter()
        .filter(|(_, record)| record.state == "open")
        .collect();
    assert_eq!(open.len(), 1);
    assert_eq!(
        split_ask_key(&open[0].0).map(|(_, id)| id),
        Some(second_id.as_str())
    );

    let stale = execute_answer_child(
        &serde_json::json!({
            "child_task_id": child.id,
            "ask_id": first_id,
            "answer": "duplicate stale answer",
        }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect_err("an already-answered ask id must be reported as stale");
    assert!(
        stale.to_string().contains("not open") || stale.to_string().contains("open ask"),
        "stale ask error should be actionable, got: {stale}"
    );

    execute_answer_child(
        &serde_json::json!({
            "child_task_id": child.id,
            "ask_id": second_id,
            "answer": "answer second",
        }),
        &engine,
        "thread-parent",
        None,
    )
    .await
    .expect("second ask answered");
    let unblocked = task_by_id_for_tool_scope(&engine, &child.id)
        .await
        .expect("child exists");
    assert_ne!(unblocked.status, TaskStatus::Blocked);
}

#[tokio::test]
async fn answer_child_accepts_short_handles_and_disambiguates_duplicate_titles() {
    let (_root, engine) = setup().await;
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
    let mut first = engine
        .enqueue_task(
            "Researcher".into(),
            "first".into(),
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
    first.thread_id = Some("thread-researcher-a".into());
    persist_task_update(&engine, &first, None)
        .await
        .expect("persist first");
    let mut second = engine
        .enqueue_task(
            "Researcher".into(),
            "second".into(),
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
    second.thread_id = Some("thread-researcher-b".into());
    persist_task_update(&engine, &second, None)
        .await
        .expect("persist second");

    let first_ask = execute_ask_parent(
        &ask_args("First question?"),
        &engine,
        "thread-researcher-a",
        Some(&first.id),
    )
    .await
    .expect("first ask");
    let second_ask = execute_ask_parent(
        &ask_args("Second question?"),
        &engine,
        "thread-researcher-b",
        Some(&second.id),
    )
    .await
    .expect("second ask");
    let first_slug = serde_json::from_str::<serde_json::Value>(&first_ask).expect("json")
        ["ask_slug"]
        .as_str()
        .expect("slug")
        .to_string();
    let second_slug = serde_json::from_str::<serde_json::Value>(&second_ask).expect("json")
        ["ask_slug"]
        .as_str()
        .expect("slug")
        .to_string();
    assert_eq!(first_slug, "q1");
    assert_eq!(second_slug, "q1");

    execute_answer_child(
        &serde_json::json!({
            "child_task_id": "researcher",
            "ask_id": "q1",
            "answer": "answer the first researcher",
        }),
        &engine,
        "thread-parent",
        Some(&parent.id),
    )
    .await
    .expect("first handle resolves");
    execute_answer_child(
        &serde_json::json!({
            "child_task_id": "researcher-2",
            "ask_id": "Q1",
            "answer": "answer the second researcher",
        }),
        &engine,
        "thread-parent",
        Some(&parent.id),
    )
    .await
    .expect("suffixed handle resolves");

    let first_record = list_ask_records(&engine, &first.id)
        .await
        .expect("records")
        .into_iter()
        .next()
        .expect("first ask stored");
    let second_record = list_ask_records(&engine, &second.id)
        .await
        .expect("records")
        .into_iter()
        .next()
        .expect("second ask stored");
    assert_eq!(
        first_record.1.answer.as_deref(),
        Some("answer the first researcher")
    );
    assert_eq!(
        second_record.1.answer.as_deref(),
        Some("answer the second researcher")
    );
}
