use super::*;

#[tokio::test]
async fn execute_tool_carries_weles_review_metadata_on_error_result() {
    let review = crate::agent::types::WelesReviewMeta {
        weles_reviewed: true,
        verdict: crate::agent::types::WelesVerdict::Block,
        reasons: vec!["destructive command".to_string()],
        audit_id: Some("audit-weles-1".to_string()),
        security_override_mode: Some("yolo".to_string()),
    };
    let tool_call = ToolCall {
        id: "tool-call-1".to_string(),
        function: ToolFunction {
            name: "bash_command".to_string(),
            arguments: "{not-json".to_string(),
        },
        weles_review: Some(review.clone()),
    };
    let temp_dir = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(temp_dir.path()).await;
    let engine =
        AgentEngine::new_test(manager.clone(), AgentConfig::default(), temp_dir.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-1",
        None,
        &manager,
        None,
        &event_tx,
        temp_dir.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error);
    assert_eq!(result.weles_review, Some(review));
}

#[test]
fn tool_events_serialize_weles_review_metadata() {
    let review = crate::agent::types::WelesReviewMeta {
        weles_reviewed: false,
        verdict: crate::agent::types::WelesVerdict::FlagOnly,
        reasons: vec!["unreviewed fallback".to_string()],
        audit_id: Some("audit-weles-2".to_string()),
        security_override_mode: Some("operator_override".to_string()),
    };

    let tool_call_event = AgentEvent::ToolCall {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"rm -rf /tmp/demo\"}".to_string(),
        weles_review: Some(review.clone()),
        message_id: None,
        turn_input_tokens: None,
        turn_output_tokens: None,
        turn_cost_usd: None,
    };
    let tool_result_event = AgentEvent::ToolResult {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        content: "blocked by policy".to_string(),
        is_error: true,
        weles_review: Some(review),
        message_id: None,
    };

    let call_json = serde_json::to_value(&tool_call_event).expect("tool call should serialize");
    assert_eq!(call_json["weles_review"]["weles_reviewed"], false);
    assert_eq!(call_json["weles_review"]["verdict"], "flag_only");
    assert_eq!(call_json["weles_review"]["audit_id"], "audit-weles-2");
    assert_eq!(
        call_json["weles_review"]["security_override_mode"],
        "operator_override"
    );

    let result_json =
        serde_json::to_value(&tool_result_event).expect("tool result should serialize");
    assert_eq!(result_json["weles_review"]["weles_reviewed"], false);
    assert_eq!(result_json["weles_review"]["verdict"], "flag_only");
    assert_eq!(
        result_json["weles_review"]["reasons"][0],
        "unreviewed fallback"
    );
}

#[test]
fn tool_call_default_unreviewed_weles_review_is_explicit() {
    let tool_call = ToolCall::with_default_weles_review(
        "call-default".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: "{}".to_string(),
        },
    );

    let review = tool_call
        .weles_review
        .as_ref()
        .expect("default tool call should carry explicit unreviewed metadata");
    assert!(!review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert_eq!(review.reasons, vec!["governance_not_run".to_string()]);
    assert_eq!(review.audit_id, None);
    assert_eq!(review.security_override_mode, None);
}

#[test]
fn weles_governance_prompt_prepends_core_and_appends_operator_suffix() {
    let config = AgentConfig {
        system_prompt: "Main operator prompt".to_string(),
        builtin_sub_agents: crate::agent::types::BuiltinSubAgentOverrides {
            weles: crate::agent::types::WelesBuiltinOverrides {
                system_prompt: Some("Operator WELES override".to_string()),
                ..Default::default()
            },
            ..Default::default()
        },
        ..AgentConfig::default()
    };
    let task = crate::agent::types::AgentTask {
        id: "task-1".to_string(),
        title: "Review risky command".to_string(),
        description: "Inspect a suspicious tool call.".to_string(),
        status: crate::agent::types::TaskStatus::Queued,
        priority: crate::agent::types::TaskPriority::High,
        progress: 0,
        created_at: 123,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-1".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: Some("rm -rf /tmp/demo".to_string()),
        session_id: Some("session-1".to_string()),
        goal_run_id: Some("goal-1".to_string()),
        goal_run_title: Some("Keep workspace safe".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("Inspect tool call".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting policy review".to_string()),
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: Some("previous policy timeout".to_string()),
        logs: Vec::new(),
        completion_contract: None,
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_api_transport: None,
        override_system_prompt: None,
        sub_agent_def_id: Some("weles_builtin".to_string()),
    };

    let prompt = crate::agent::weles_governance::build_weles_governance_prompt(
        &config,
        "bash_command",
        &serde_json::json!({"command": "rm -rf /tmp/demo", "cwd": "/tmp"}),
        SecurityLevel::Moderate,
        &[
            "destructive command".to_string(),
            "workspace delete".to_string(),
        ],
        Some(&task),
        None,
    );

    let core_idx = prompt
        .find("## WELES Governance Core")
        .expect("governance core should be present");
    let inspect_idx = prompt
        .find("## Inspection Context")
        .expect("inspection context should be present");
    let suffix_idx = prompt
        .find("## Operator WELES Suffix")
        .expect("operator suffix should be present");
    assert!(
        core_idx < inspect_idx,
        "core should precede inspection context"
    );
    assert!(
        inspect_idx < suffix_idx,
        "operator override must stay suffix-only"
    );
    assert!(prompt.contains("Operator WELES override"));
    assert!(prompt.contains("tool_name: bash_command"));
    assert!(prompt.contains("security_level: moderate"));
    assert!(prompt.contains("destructive command"));
    assert!(prompt.contains("goal_run_id: goal-1"));
    assert!(prompt.contains("task_id: task-1"));
    assert!(prompt.contains("task_health_signals:"));
    assert!(prompt.contains("retry_count: 0"));
    assert!(prompt.contains("max_retries: 3"));
    assert!(prompt.contains("blocked_reason: awaiting policy review"));
    assert!(prompt.contains("last_error: previous policy timeout"));
}

#[test]
fn weles_governance_internal_bypass_marker_is_internal_only() {
    let governance_marker =
        crate::agent::weles_governance::internal_bypass_marker_for_scope("governance");
    let vitality_marker =
        crate::agent::weles_governance::internal_bypass_marker_for_scope("vitality");
    let normal_marker = crate::agent::weles_governance::internal_bypass_marker_for_scope("main");

    assert!(governance_marker.is_some());
    assert!(vitality_marker.is_some());
    assert!(normal_marker.is_none());

    let governance_marker = governance_marker.expect("governance marker missing");
    assert!(crate::agent::weles_governance::has_internal_bypass_marker(
        &governance_marker,
        "governance"
    ));
    assert!(!crate::agent::weles_governance::has_internal_bypass_marker(
        &governance_marker,
        "main"
    ));
}

#[test]
fn weles_persistence_ignores_attempts_to_weaken_core_fields_and_inspection_inputs() {
    let (config, collisions) =
        crate::agent::config::load_config_from_items_with_weles_cleanup(vec![
            (
                "/provider".to_string(),
                serde_json::Value::String("openai".to_string()),
            ),
            (
                "/model".to_string(),
                serde_json::Value::String("gpt-5.4-mini".to_string()),
            ),
            (
                "/system_prompt".to_string(),
                serde_json::Value::String("Main prompt".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/system_prompt".to_string(),
                serde_json::Value::String("Operator suffix".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/role".to_string(),
                serde_json::Value::String("assistant".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/enabled".to_string(),
                serde_json::Value::Bool(false),
            ),
            (
                "/builtin_sub_agents/weles/builtin".to_string(),
                serde_json::Value::Bool(false),
            ),
            (
                "/builtin_sub_agents/weles/immutable_identity".to_string(),
                serde_json::Value::Bool(false),
            ),
            (
                "/builtin_sub_agents/weles/disable_allowed".to_string(),
                serde_json::Value::Bool(true),
            ),
            (
                "/builtin_sub_agents/weles/delete_allowed".to_string(),
                serde_json::Value::Bool(true),
            ),
            (
                "/builtin_sub_agents/weles/protected_reason".to_string(),
                serde_json::Value::String("operator changed this".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/tool_name".to_string(),
                serde_json::Value::String("echo".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/security_level".to_string(),
                serde_json::Value::String("lowest".to_string()),
            ),
            (
                "/builtin_sub_agents/weles/suspicion_reasons".to_string(),
                serde_json::json!(["operator removed reasons"]),
            ),
        ])
        .expect("config should load");

    assert!(collisions.is_empty());
    assert_eq!(
        config.builtin_sub_agents.weles.system_prompt.as_deref(),
        Some("Operator suffix")
    );
    assert_eq!(config.builtin_sub_agents.weles.role, None);

    let effective = crate::agent::config::load_config_from_items(vec![
        (
            "/provider".to_string(),
            serde_json::Value::String("openai".to_string()),
        ),
        (
            "/model".to_string(),
            serde_json::Value::String("gpt-5.4-mini".to_string()),
        ),
        (
            "/system_prompt".to_string(),
            serde_json::Value::String("Main prompt".to_string()),
        ),
        (
            "/builtin_sub_agents/weles/system_prompt".to_string(),
            serde_json::Value::String("Operator suffix".to_string()),
        ),
        (
            "/builtin_sub_agents/weles/role".to_string(),
            serde_json::Value::String("assistant".to_string()),
        ),
        (
            "/builtin_sub_agents/weles/enabled".to_string(),
            serde_json::Value::Bool(false),
        ),
    ])
    .expect("config should load");
    let weles = crate::agent::config::effective_sub_agents_from_config(&effective)
        .0
        .into_iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("effective WELES should be present");
    assert!(weles.enabled);
    assert!(weles.builtin);
    assert!(weles.immutable_identity);
    assert!(!weles.disable_allowed);
    assert!(!weles.delete_allowed);
    assert_eq!(weles.role.as_deref(), Some("governance"));

    let governance_prompt = crate::agent::weles_governance::build_weles_governance_prompt(
        &effective,
        "bash_command",
        &serde_json::json!({"command": "rm -rf /tmp/demo"}),
        SecurityLevel::Moderate,
        &["destructive command".to_string()],
        None,
        None,
    );
    assert!(governance_prompt.contains("tool_name: bash_command"));
    assert!(governance_prompt.contains("security_level: moderate"));
    assert!(governance_prompt.contains("destructive command"));
    assert!(!governance_prompt.contains("operator removed reasons"));
}
