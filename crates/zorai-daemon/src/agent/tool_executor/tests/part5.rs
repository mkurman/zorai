use super::super::*;
use super::super::{
    execute_gateway_message, execute_get_debate_session, execute_get_divergent_session,
    execute_headless_shell_command, execute_run_debate, get_available_tools, resolve_skill_path,
    wait_for_managed_command_outcome,
};
use crate::agent::{types::AgentConfig, AgentEngine};
use crate::history::SkillVariantRecord;
use crate::session_manager::SessionManager;
use std::fs;
use tempfile::tempdir;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use zorai_protocol::{DaemonMessage, GatewaySendResult, SessionId};

#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

#[tokio::test]
async fn managed_command_wait_can_be_cancelled() {
    let (_tx, mut rx) = broadcast::channel(4);
    let token = CancellationToken::new();
    token.cancel();

    let error =
        wait_for_managed_command_outcome(&mut rx, SessionId::nil(), "exec-1", 30, Some(&token))
            .await
            .err()
            .expect("managed wait should abort when cancellation is requested");

    assert!(error.to_string().contains("cancelled"));
}

#[tokio::test]
async fn managed_command_wait_fails_when_session_exits() {
    let (tx, mut rx) = broadcast::channel(4);
    tx.send(DaemonMessage::SessionExited {
        id: SessionId::nil(),
        exit_code: Some(1),
    })
    .expect("session exit should broadcast");

    let error = wait_for_managed_command_outcome(&mut rx, SessionId::nil(), "exec-1", 30, None)
        .await
        .expect_err("managed wait should fail when the session exits");

    assert!(error.to_string().contains("session exited"));
}

#[tokio::test]
async fn headless_shell_command_can_be_cancelled() {
    let root = tempfile::tempdir().unwrap();
    let session_manager = SessionManager::new_test(root.path()).await;
    let token = CancellationToken::new();
    let cancel = token.clone();

    let join = tokio::spawn(async move {
        execute_headless_shell_command(
            &serde_json::json!({
                "command": "sleep 30",
                "timeout_seconds": 30
            }),
            &session_manager,
            None,
            "bash_command",
            Some(token),
            None,
        )
        .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();

    let error = join
        .await
        .expect("headless shell join should complete")
        .expect_err("headless shell should abort when cancellation is requested");

    assert!(error.to_string().contains("cancelled"));
}

#[tokio::test]
async fn cancel_headless_operation_kills_backgrounded_command_by_operation_id() {
    let root = tempfile::tempdir().unwrap();
    let session_manager = SessionManager::new_test(root.path()).await;

    let (output, _) = execute_headless_shell_command(
        &serde_json::json!({
            "command": "sleep 30",
            "timeout_seconds": 30,
            "wait_for_completion": false
        }),
        &session_manager,
        None,
        "bash_command",
        None,
        None,
    )
    .await
    .expect("background headless command should spawn");

    let operation_id = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("operation_id: "))
        .expect("background spawn should report an operation_id")
        .to_string();

    assert!(!cancel_headless_operation("missing-operation"));
    assert!(cancel_headless_operation(&operation_id));

    let snapshot = timeout(Duration::from_secs(5), async {
        loop {
            if let Some(snapshot) = crate::server::operation_registry().snapshot(&operation_id) {
                if matches!(
                    snapshot.state,
                    zorai_protocol::OperationLifecycleState::Failed
                ) {
                    return snapshot;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("cancelled operation should reach a terminal failed state instead of sleeping out");

    let terminal = crate::server::operation_registry()
        .terminal_result(&snapshot.operation_id)
        .expect("cancelled operation should record a terminal result");
    assert_eq!(terminal.get("cancelled"), Some(&serde_json::json!(true)));
}

#[test]
fn resolve_skill_path_finds_generated_skill_by_stem() {
    let root = std::env::temp_dir().join(format!("zorai-skill-test-{}", uuid::Uuid::new_v4()));
    let generated = root.join("generated");
    fs::create_dir_all(&generated).expect("skill test directory should be created");
    let skill_path = generated.join("build-release.md");
    fs::write(&skill_path, "# Build release\n").expect("skill file should be written");

    let resolved =
        resolve_skill_path(&root, "build-release", None).expect("generated skill should resolve");
    assert_eq!(resolved, skill_path);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn resolve_skill_path_prefers_selected_variant() {
    let root = std::env::temp_dir().join(format!("zorai-skill-test-{}", uuid::Uuid::new_v4()));
    let generated = root.join("generated");
    fs::create_dir_all(&generated).expect("skill test directory should be created");
    let canonical = generated.join("build-release.md");
    let frontend = generated.join("build-release--frontend.md");
    fs::write(&canonical, "# Build release\n").expect("canonical skill file should be written");
    fs::write(&frontend, "# Frontend build release\n")
        .expect("variant skill file should be written");

    let resolved = resolve_skill_path(
        &root,
        "build-release",
        Some(&SkillVariantRecord {
            variant_id: "variant-1".to_string(),
            skill_name: "build-release".to_string(),
            variant_name: "frontend".to_string(),
            relative_path: "generated/build-release--frontend.md".to_string(),
            parent_variant_id: Some("parent-1".to_string()),
            version: "v2.0".to_string(),
            context_tags: vec!["frontend".to_string()],
            use_count: 0,
            success_count: 0,
            failure_count: 0,
            fitness_score: 0.0,
            status: "active".to_string(),
            last_used_at: None,
            created_at: 0,
            updated_at: 0,
        }),
    )
    .expect("selected variant should resolve");
    assert_eq!(resolved, frontend);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn resolve_skill_path_maps_builtin_variant_record_to_seeded_superpowers_path() {
    let root = std::env::temp_dir().join(format!("zorai-skill-test-{}", uuid::Uuid::new_v4()));
    let canonical = root
        .join("development")
        .join("superpowers")
        .join("subagent-driven-development")
        .join("SKILL.md");
    fs::create_dir_all(canonical.parent().expect("skill parent"))
        .expect("skill test directory should be created");
    fs::write(&canonical, "# Subagent-Driven Development\n")
        .expect("canonical skill file should be written");

    let resolved = resolve_skill_path(
        &root,
        "subagent-driven-development",
        Some(&SkillVariantRecord {
            variant_id: "variant-1".to_string(),
            skill_name: "subagent-driven-development".to_string(),
            variant_name: "canonical".to_string(),
            relative_path: "builtin/superpowers/subagent-driven-development/SKILL.md".to_string(),
            parent_variant_id: None,
            version: "v1.0".to_string(),
            context_tags: vec!["rust".to_string()],
            use_count: 0,
            success_count: 0,
            failure_count: 0,
            fitness_score: 0.0,
            status: "active".to_string(),
            last_used_at: None,
            created_at: 0,
            updated_at: 0,
        }),
    )
    .expect("seeded superpowers skill should resolve from builtin variant path");
    assert_eq!(resolved, canonical);

    let _ = fs::remove_dir_all(&root);
}

fn named_tool(name: &str) -> crate::agent::types::ToolDefinition {
    crate::agent::types::ToolDefinition {
        tool_type: "function".to_string(),
        function: crate::agent::types::ToolFunctionDef {
            name: name.to_string(),
            description: format!("tool {name}"),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    }
}

#[test]
fn reorder_tools_by_heuristics_promotes_preferred_fallbacks_without_overriding_scores() {
    let mut tools = vec![
        named_tool("highest_score"),
        named_tool("second_score"),
        named_tool("unscored_a"),
        named_tool("preferred_fallback"),
        named_tool("unscored_b"),
    ];
    let store = crate::agent::learning::heuristics::HeuristicStore {
        tool_heuristics: vec![
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "highest_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.95,
                avg_duration_ms: 10,
                usage_count: 5,
            },
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "second_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.75,
                avg_duration_ms: 10,
                usage_count: 5,
            },
        ],
        ..Default::default()
    };

    super::reorder_tools_by_heuristics(
        &mut tools,
        &store,
        "coding",
        &["preferred_fallback".to_string()],
        false,
    );

    let ordered = tools
        .iter()
        .map(|tool| tool.function.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        vec![
            "highest_score",
            "second_score",
            "preferred_fallback",
            "unscored_a",
            "unscored_b",
        ]
    );
}

#[test]
fn reorder_tools_by_heuristics_keeps_order_stable_when_no_preferred_match_exists() {
    let mut tools = vec![
        named_tool("highest_score"),
        named_tool("second_score"),
        named_tool("unscored_a"),
        named_tool("unscored_b"),
    ];
    let store = crate::agent::learning::heuristics::HeuristicStore {
        tool_heuristics: vec![
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "highest_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.95,
                avg_duration_ms: 10,
                usage_count: 5,
            },
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "second_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.75,
                avg_duration_ms: 10,
                usage_count: 5,
            },
        ],
        ..Default::default()
    };

    super::reorder_tools_by_heuristics(
        &mut tools,
        &store,
        "coding",
        &["missing_tool".to_string()],
        false,
    );

    let ordered = tools
        .iter()
        .map(|tool| tool.function.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        vec!["highest_score", "second_score", "unscored_a", "unscored_b",]
    );
}

#[test]
fn reorder_tools_promotes_ask_questions_when_prioritize_clarification_is_set_even_against_scored_tools(
) {
    let mut tools = vec![
        named_tool("highest_score"),
        named_tool("other_unscored"),
        named_tool(zorai_protocol::tool_names::ASK_QUESTIONS),
    ];
    let store = crate::agent::learning::heuristics::HeuristicStore {
        tool_heuristics: vec![crate::agent::learning::heuristics::ToolHeuristic {
            tool_name: "highest_score".to_string(),
            task_type: "coding".to_string(),
            effectiveness_score: 0.95,
            avg_duration_ms: 10,
            usage_count: 5,
        }],
        ..Default::default()
    };

    super::reorder_tools_by_heuristics(&mut tools, &store, "coding", &[], true);

    assert_eq!(
        tools.first().map(|tool| tool.function.name.as_str()),
        Some(zorai_protocol::tool_names::ASK_QUESTIONS),
        "prioritize_clarification must move ASK_QUESTIONS to the top even when other tools have heuristic scores"
    );
}

#[test]
fn reorder_tools_keeps_score_order_among_non_clarification_tools_when_prioritize_clarification_is_set(
) {
    let mut tools = vec![
        named_tool("second_score"),
        named_tool(zorai_protocol::tool_names::ASK_QUESTIONS),
        named_tool("highest_score"),
    ];
    let store = crate::agent::learning::heuristics::HeuristicStore {
        tool_heuristics: vec![
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "highest_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.95,
                avg_duration_ms: 10,
                usage_count: 5,
            },
            crate::agent::learning::heuristics::ToolHeuristic {
                tool_name: "second_score".to_string(),
                task_type: "coding".to_string(),
                effectiveness_score: 0.75,
                avg_duration_ms: 10,
                usage_count: 5,
            },
        ],
        ..Default::default()
    };

    super::reorder_tools_by_heuristics(&mut tools, &store, "coding", &[], true);

    let ordered = tools
        .iter()
        .map(|tool| tool.function.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        vec![
            zorai_protocol::tool_names::ASK_QUESTIONS,
            "highest_score",
            "second_score",
        ],
        "ASK_QUESTIONS leads; remaining tools keep heuristic-score ordering"
    );
}

#[test]
fn list_sessions_tool_requires_workspace_topology() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();

    let no_topology = get_available_tools(&config, &temp_dir, false);
    assert!(no_topology
        .iter()
        .all(|tool| tool.function.name != "list_sessions"));
    assert!(no_topology
        .iter()
        .any(|tool| tool.function.name == "list_terminals"));

    let with_topology = get_available_tools(&config, &temp_dir, true);
    assert!(with_topology
        .iter()
        .any(|tool| tool.function.name == "list_sessions"));
}

#[test]
fn python_tool_is_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let python = tools
        .iter()
        .find(|tool| tool.function.name == "python")
        .expect("python tool should be available");

    assert!(
        !tools
            .iter()
            .any(|tool| tool.function.name == "python_execute"),
        "legacy python_execute alias must stay out of the catalog"
    );

    let properties = python
        .function
        .parameters
        .get("properties")
        .expect("python schema should expose properties");

    assert!(
        properties.get("code").is_some(),
        "schema should include code"
    );
    assert!(properties.get("cwd").is_some(), "schema should include cwd");
    assert!(
        properties.get("wait_for_completion").is_some(),
        "schema should include wait_for_completion"
    );
    assert!(
        properties.get("timeout_seconds").is_some(),
        "schema should include timeout_seconds"
    );
    assert_eq!(
        python
            .function
            .parameters
            .get("required")
            .and_then(|value| value.as_array())
            .map(|items| items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()),
        Some(vec!["code"])
    );
}

#[test]
fn current_datetime_tool_is_exposed() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let current_datetime = tools
        .iter()
        .find(|tool| tool.function.name == "get_current_datetime")
        .expect("get_current_datetime tool should be available");

    let properties = current_datetime
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("get_current_datetime schema should expose properties object");

    assert!(
        properties.is_empty(),
        "datetime tool should not require arguments"
    );
    assert!(current_datetime
        .function
        .parameters
        .get("required")
        .is_none());
}

#[test]
fn routine_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    let create_routine = tools
        .iter()
        .find(|tool| tool.function.name == "create_routine")
        .expect("create_routine tool should be available");
    let create_properties = create_routine
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("create_routine schema should expose properties object");
    for required_property in [
        "title",
        "description",
        "schedule_expression",
        "target_kind",
        "target_payload",
    ] {
        assert!(
            create_properties.contains_key(required_property),
            "create_routine should expose {required_property}"
        );
    }

    let preview_routine = tools
        .iter()
        .find(|tool| tool.function.name == "preview_routine")
        .expect("preview_routine tool should be available");
    let preview_properties = preview_routine
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("preview_routine schema should expose properties object");
    assert!(preview_properties.contains_key("routine_id"));
    assert!(preview_properties.contains_key("fire_count"));

    let update_routine = tools
        .iter()
        .find(|tool| tool.function.name == "update_routine")
        .expect("update_routine tool should be available");
    let update_properties = update_routine
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("update_routine schema should expose properties object");
    assert!(update_properties.contains_key("routine_id"));
    assert!(update_properties.contains_key("target_payload"));

    let list_routines = tools
        .iter()
        .find(|tool| tool.function.name == "list_routines")
        .expect("list_routines tool should be available");
    let list_properties = list_routines
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_routines schema should expose properties object");
    assert!(list_properties.is_empty());

    for tool_name in [
        "get_routine",
        "run_routine_now",
        "list_routine_history",
        "pause_routine",
        "resume_routine",
        "delete_routine",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.function.name == tool_name)
            .unwrap_or_else(|| panic!("{tool_name} tool should be available"));
        let properties = tool
            .function
            .parameters
            .get("properties")
            .and_then(|value| value.as_object())
            .unwrap_or_else(|| panic!("{tool_name} schema should expose properties object"));
        assert!(
            properties.contains_key("routine_id"),
            "{tool_name} should expose routine_id"
        );
    }

    let rerun_routine = tools
        .iter()
        .find(|tool| tool.function.name == "rerun_routine")
        .expect("rerun_routine tool should be available");
    let rerun_properties = rerun_routine
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("rerun_routine schema should expose properties object");
    assert!(rerun_properties.contains_key("run_id"));
}

#[test]
fn whatsapp_control_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    for tool_name in [
        "whatsapp_link_start",
        "whatsapp_link_stop",
        "whatsapp_link_reset",
        "whatsapp_link_status",
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.function.name == tool_name)
            .unwrap_or_else(|| panic!("{tool_name} tool should be available"));
        let properties = tool
            .function
            .parameters
            .get("properties")
            .and_then(|value| value.as_object())
            .unwrap_or_else(|| panic!("{tool_name} schema should expose properties object"));
        assert!(
            properties.is_empty(),
            "{tool_name} should not require arguments"
        );
        assert!(tool.function.parameters.get("required").is_none());
    }
}

#[test]
fn event_trigger_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    let list_triggers = tools
        .iter()
        .find(|tool| tool.function.name == "list_triggers")
        .expect("list_triggers tool should be available");
    assert!(list_triggers
        .function
        .description
        .contains("packaged defaults"));
    assert!(list_triggers.function.description.contains("fresh engine"));
    let list_triggers_properties = list_triggers
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_triggers schema should expose properties object");
    assert!(list_triggers_properties.is_empty());

    let add_trigger = tools
        .iter()
        .find(|tool| tool.function.name == "add_trigger")
        .expect("add_trigger tool should be available");
    assert!(add_trigger.function.description.contains("Pack 1 defaults"));
    assert!(add_trigger.function.description.contains("source: custom"));
    let properties = add_trigger
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("add_trigger schema should expose properties object");
    for required_property in [
        "event_family",
        "event_kind",
        "notification_kind",
        "title_template",
        "body_template",
    ] {
        assert!(
            properties.contains_key(required_property),
            "add_trigger should expose {required_property}"
        );
    }
    assert!(properties
        .get("event_family")
        .and_then(|value| value.get("description"))
        .and_then(|value| value.as_str())
        .is_some_and(
            |description| description.contains("filesystem") && description.contains("system")
        ));
    assert!(properties
        .get("event_kind")
        .and_then(|value| value.get("description"))
        .and_then(|value| value.as_str())
        .is_some_and(|description| description.contains("file_changed")
            && description.contains("disk_pressure")));

    let ingest_webhook = tools
        .iter()
        .find(|tool| tool.function.name == "ingest_webhook_event")
        .expect("ingest_webhook_event tool should be available");
    assert!(ingest_webhook.function.description.contains("fresh engine"));
    assert!(ingest_webhook
        .function
        .description
        .contains("packaged defaults"));
    let ingest_properties = ingest_webhook
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("ingest_webhook_event schema should expose properties object");
    for required_property in ["event_family", "event_kind"] {
        assert!(
            ingest_properties.contains_key(required_property),
            "ingest_webhook_event should expose {required_property}"
        );
    }
    for optional_property in ["state", "thread_id", "payload"] {
        assert!(
            ingest_properties.contains_key(optional_property),
            "ingest_webhook_event should expose {optional_property}"
        );
    }

    tools
        .iter()
        .find(|tool| tool.function.name == "show_dreams")
        .expect("show_dreams tool should be available");

    let show_harness_state = tools
        .iter()
        .find(|tool| tool.function.name == "show_harness_state")
        .expect("show_harness_state tool should be available");
    let harness_properties = show_harness_state
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("show_harness_state schema should expose properties object");
    for expected in ["thread_id", "goal_run_id", "task_id", "limit"] {
        assert!(
            harness_properties.contains_key(expected),
            "show_harness_state should expose {expected}"
        );
    }

    let import_external_runtime = tools
        .iter()
        .find(|tool| tool.function.name == "import_external_runtime")
        .expect("import_external_runtime tool should be available");
    assert!(import_external_runtime
        .function
        .description
        .contains("dry-run"));
    let import_external_runtime_properties = import_external_runtime
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("import_external_runtime schema should expose properties object");
    for expected in ["runtime", "config_path", "dry_run", "conflict_policy"] {
        assert!(
            import_external_runtime_properties.contains_key(expected),
            "import_external_runtime should expose {expected}"
        );
    }

    let show_import_report = tools
        .iter()
        .find(|tool| tool.function.name == "show_import_report")
        .expect("show_import_report tool should be available");
    assert!(show_import_report
        .function
        .description
        .contains("Hermes/OpenClaw"));
    let import_report_properties = show_import_report
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("show_import_report schema should expose properties object");
    for expected in ["runtime", "limit"] {
        assert!(
            import_report_properties.contains_key(expected),
            "show_import_report should expose {expected}"
        );
    }

    let preview_shadow_run = tools
        .iter()
        .find(|tool| tool.function.name == "preview_shadow_run")
        .expect("preview_shadow_run tool should be available");
    assert!(preview_shadow_run
        .function
        .description
        .contains("read-only"));
    let preview_shadow_run_properties = preview_shadow_run
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("preview_shadow_run schema should expose properties object");
    assert!(preview_shadow_run_properties.contains_key("runtime"));
}

#[test]
fn memory_read_tools_are_exposed_with_injection_aware_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    for (tool_name, expected_layers) in [
        (
            "read_memory",
            vec![
                "include_base_markdown",
                "include_operator_profile_json",
                "include_operator_model_summary",
                "include_thread_structural_memory",
            ],
        ),
        (
            "read_user",
            vec![
                "include_base_markdown",
                "include_operator_profile_json",
                "include_operator_model_summary",
                "include_thread_structural_memory",
            ],
        ),
        (
            "read_soul",
            vec![
                "include_base_markdown",
                "include_operator_profile_json",
                "include_operator_model_summary",
                "include_thread_structural_memory",
            ],
        ),
    ] {
        let tool = tools
            .iter()
            .find(|tool| tool.function.name == tool_name)
            .unwrap_or_else(|| panic!("{tool_name} should be available"));
        let properties = tool
            .function
            .parameters
            .get("properties")
            .and_then(|value| value.as_object())
            .unwrap_or_else(|| panic!("{tool_name} schema should expose properties"));

        assert!(
            properties.contains_key("include_already_injected"),
            "{tool_name} should expose include_already_injected"
        );
        assert!(
            properties.contains_key("limit_per_layer"),
            "{tool_name} should expose limit_per_layer"
        );
        for layer in expected_layers {
            assert!(
                properties.contains_key(layer),
                "{tool_name} should expose {layer}"
            );
            assert_eq!(
                properties
                    .get(layer)
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_str()),
                Some("boolean"),
                "{tool_name} {layer} should be a boolean toggle"
            );
        }
    }
}

#[test]
fn get_background_task_status_alias_is_hidden_from_catalog() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    assert!(!tools
        .iter()
        .any(|tool| tool.function.name == "get_background_task_status"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "get_operation_status"));
}

#[test]
fn get_operation_status_tool_is_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let status_tool = tools
        .iter()
        .find(|tool| tool.function.name == "get_operation_status")
        .expect("get_operation_status tool should be available");

    let description = status_tool.function.description.as_str();
    assert!(
        description.contains("Do not poll"),
        "get_operation_status should forbid polling: {description}"
    );
    assert!(
        description.contains("wait=true"),
        "get_operation_status should guide agents to blocking wait instead of polling: {description}"
    );

    let properties = status_tool
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("get_operation_status schema should expose properties");

    assert!(
        properties.get("operation_id").is_some(),
        "schema should include operation_id"
    );
    assert!(
        properties.get("wait").is_some(),
        "schema should include wait"
    );
    assert!(
        properties.get("timeout_seconds").is_some(),
        "schema should include timeout_seconds"
    );
    let required = status_tool
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("get_operation_status should define required fields");
    assert_eq!(required, vec!["operation_id"]);
}

#[test]
fn generate_image_is_available_without_vision_for_image_generation_models() {
    let temp_dir = std::env::temp_dir();

    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-image-1".to_string();
    config.tools.vision = false;

    let tools = get_available_tools(&config, &temp_dir, false);
    assert!(tools
        .iter()
        .all(|tool| tool.function.name != "analyze_image"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "generate_image"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "speech_to_text"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "text_to_speech"));
}

#[test]
fn analyze_image_is_available_when_active_model_has_vision_even_if_toggle_is_off() {
    let temp_dir = std::env::temp_dir();
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4".to_string();
    config.tools.vision = false;

    let tools = get_available_tools(&config, &temp_dir, false);
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "analyze_image"));
}

#[test]
fn generate_image_is_hidden_when_active_model_context_lacks_image_generation_capability() {
    let temp_dir = std::env::temp_dir();
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_XAI.to_string();
    config.model = "grok-code-fast-1".to_string();
    config.tools.vision = true;

    let tools = get_available_tools(&config, &temp_dir, false);
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "analyze_image"));
    assert!(tools
        .iter()
        .all(|tool| tool.function.name != "generate_image"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "speech_to_text"));
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "text_to_speech"));
}

#[test]
fn configured_image_generation_model_keeps_generate_image_available() {
    let temp_dir = std::env::temp_dir();
    let mut config = AgentConfig::default();
    config.provider = zorai_shared::providers::PROVIDER_ID_XAI.to_string();
    config.model = "grok-code-fast-1".to_string();
    config.tools.vision = false;
    config.extra.insert(
        "image".to_string(),
        serde_json::json!({
            "generation": {
                "provider": zorai_shared::providers::PROVIDER_ID_OPENROUTER,
                "model": "google/gemini-3-pro-image-preview"
            }
        }),
    );

    let tools = get_available_tools(&config, &temp_dir, false);
    assert!(tools
        .iter()
        .any(|tool| tool.function.name == "generate_image"));
    assert!(tools
        .iter()
        .all(|tool| tool.function.name != "analyze_image"));
}

#[test]
fn media_tools_expose_expected_core_parameters() {
    let mut config = AgentConfig::default();
    config.tools.vision = true;
    config.extra.insert(
        "image".to_string(),
        serde_json::json!({
            "generation": {
                "provider": zorai_shared::providers::PROVIDER_ID_OPENAI,
                "model": "gpt-image-1"
            }
        }),
    );
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    let generate_image = tools
        .iter()
        .find(|tool| tool.function.name == "generate_image")
        .expect("generate_image tool should be available when generation is configured");
    let generate_properties = generate_image
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("generate_image schema should expose properties");
    let generate_timeout = generate_properties
        .get("timeout_seconds")
        .expect("generate_image should expose timeout_seconds");
    assert_eq!(
        generate_timeout
            .get("type")
            .and_then(|value| value.as_str()),
        Some("integer")
    );
    assert_eq!(
        generate_timeout
            .get("maximum")
            .and_then(|value| value.as_u64()),
        Some(600)
    );
    assert!(generate_timeout
        .get("description")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.contains("default: 600") && value.contains("max: 600")));

    let analyze_image = tools
        .iter()
        .find(|tool| tool.function.name == "analyze_image")
        .expect("analyze_image tool should be available when vision is enabled");
    let analyze_properties = analyze_image
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("analyze_image schema should expose properties");
    for property in ["path", "url", "base64", "data_url", "mime_type", "prompt"] {
        assert!(
            analyze_properties.contains_key(property),
            "analyze_image should expose {property}"
        );
    }
    let analyze_timeout = analyze_properties
        .get("timeout_seconds")
        .expect("analyze_image should expose timeout_seconds");
    assert_eq!(
        analyze_timeout.get("type").and_then(|value| value.as_str()),
        Some("integer")
    );
    assert_eq!(
        analyze_timeout
            .get("maximum")
            .and_then(|value| value.as_u64()),
        Some(600)
    );
    assert!(analyze_timeout
        .get("description")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.contains("default: 600") && value.contains("max: 600")));

    let speech_to_text = tools
        .iter()
        .find(|tool| tool.function.name == "speech_to_text")
        .expect("speech_to_text tool should be available");
    let stt_properties = speech_to_text
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("speech_to_text schema should expose properties");
    let stt_timeout = stt_properties
        .get("timeout_seconds")
        .expect("speech_to_text should expose timeout_seconds");
    assert_eq!(
        stt_timeout.get("type").and_then(|value| value.as_str()),
        Some("integer")
    );
    assert_eq!(
        stt_timeout.get("maximum").and_then(|value| value.as_u64()),
        Some(600)
    );
    assert!(stt_timeout
        .get("description")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.contains("default: 600") && value.contains("max: 600")));
    let stt_required = speech_to_text
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("speech_to_text should define required fields");
    assert_eq!(stt_required, vec!["path"]);

    let text_to_speech = tools
        .iter()
        .find(|tool| tool.function.name == "text_to_speech")
        .expect("text_to_speech tool should be available");
    assert!(
        text_to_speech
            .function
            .description
            .contains("say something aloud"),
        "text_to_speech description should guide read-aloud requests"
    );
    assert!(
        text_to_speech
            .function
            .description
            .contains("temporary file path"),
        "text_to_speech description should discourage path-only follow-up replies"
    );
    let tts_properties = text_to_speech
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("text_to_speech schema should expose properties");
    let tts_timeout = tts_properties
        .get("timeout_seconds")
        .expect("text_to_speech should expose timeout_seconds");
    assert_eq!(
        tts_timeout.get("type").and_then(|value| value.as_str()),
        Some("integer")
    );
    assert_eq!(
        tts_timeout.get("maximum").and_then(|value| value.as_u64()),
        Some(600)
    );
    assert!(tts_timeout
        .get("description")
        .and_then(|value| value.as_str())
        .is_some_and(|value| value.contains("default: 600") && value.contains("max: 600")));
    let tts_required = text_to_speech
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("text_to_speech should define required fields");
    assert_eq!(tts_required, vec!["input"]);
}

#[test]
fn ask_questions_tool_is_exposed_with_compact_option_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let ask_questions = tools
        .iter()
        .find(|tool| tool.function.name == "ask_questions")
        .expect("ask_questions tool should be available");

    let properties = ask_questions
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("ask_questions schema should expose properties object");

    assert!(
        properties.get("content").is_some(),
        "schema should include content"
    );
    assert!(
        properties.get("options").is_some(),
        "schema should include options"
    );
    assert!(
        properties.get("session").is_some(),
        "schema should include session"
    );

    let required = ask_questions
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("ask_questions should define required fields");
    assert_eq!(required, vec!["content", "options"]);
}

#[test]
fn discover_skills_tool_is_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let discover_skills = tools
        .iter()
        .find(|tool| tool.function.name == "discover_skills")
        .expect("discover_skills tool should be available");
    assert_eq!(
        discover_skills.function.description,
        "Find matching local skills when a playbook would help. Optional; skip unless this turn needs one."
    );

    let properties = discover_skills
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("discover_skills schema should expose properties object");

    assert!(
        properties.get("query").is_some(),
        "schema should include query"
    );
    assert!(
        properties.get("limit").is_some(),
        "schema should include limit"
    );
    assert!(
        properties.get("session").is_some(),
        "schema should include session"
    );
    assert_eq!(
        properties
            .get("query")
            .and_then(|value| value.get("description"))
            .and_then(|value| value.as_str()),
        Some("Brief intent query, 3-6 words.")
    );

    let required = discover_skills
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("discover_skills should define required fields");
    assert_eq!(required, vec!["query"]);
}

#[test]
fn guideline_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    for name in ["list_guidelines", "discover_guidelines", "read_guideline"] {
        assert!(
            tools.iter().any(|tool| tool.function.name == name),
            "{name} tool should be available"
        );
    }

    let discover_guidelines = tools
        .iter()
        .find(|tool| tool.function.name == "discover_guidelines")
        .expect("discover_guidelines tool should be available");
    let properties = discover_guidelines
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("discover_guidelines schema should expose properties object");
    assert!(
        properties.get("query").is_some(),
        "schema should include query"
    );
    assert!(
        properties.get("limit").is_some(),
        "schema should include limit"
    );
    assert!(
        properties.get("session").is_some(),
        "schema should include session"
    );

    let required = discover_guidelines
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("discover_guidelines should define required fields");
    assert_eq!(required, vec!["query"]);
}

#[test]
fn list_tools_tool_is_exposed_with_paging_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let list_tools = tools
        .iter()
        .find(|tool| tool.function.name == "list_tools")
        .expect("list_tools tool should be available");

    let properties = list_tools
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_tools schema should expose properties object");

    assert!(
        properties.get("limit").is_some(),
        "schema should include limit"
    );
    assert!(
        properties.get("offset").is_some(),
        "schema should include offset"
    );
    assert!(list_tools.function.parameters.get("required").is_none());
}

#[test]
fn tool_search_tool_is_exposed_with_query_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let tool_search = tools
        .iter()
        .find(|tool| tool.function.name == "tool_search")
        .expect("tool_search tool should be available");

    let properties = tool_search
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("tool_search schema should expose properties object");

    assert!(
        properties.get("query").is_some(),
        "schema should include query"
    );
    assert!(
        properties.get("limit").is_some(),
        "schema should include limit"
    );
    assert!(
        properties.get("offset").is_some(),
        "schema should include offset"
    );

    let required = tool_search
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("tool_search should define required fields");
    assert_eq!(required, vec!["query"]);
}

#[test]
fn provider_and_agent_listing_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    let list_providers = tools
        .iter()
        .find(|tool| tool.function.name == "list_providers")
        .expect("list_providers tool should be available");
    let list_providers_properties = list_providers
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_providers schema should expose properties object");
    assert!(
        list_providers_properties.is_empty(),
        "list_providers should not require arguments"
    );

    let list_models = tools
        .iter()
        .find(|tool| tool.function.name == "list_models")
        .expect("list_models tool should be available");
    let list_models_properties = list_models
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_models schema should expose properties object");
    assert!(
        list_models_properties.contains_key("provider"),
        "list_models should require a provider"
    );
    let list_models_required = list_models
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("list_models should define required fields");
    assert_eq!(list_models_required, vec!["provider"]);

    let list_agents = tools
        .iter()
        .find(|tool| tool.function.name == "list_agents")
        .expect("list_agents tool should be available");
    let list_agents_properties = list_agents
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("list_agents schema should expose properties object");
    assert!(
        list_agents_properties.is_empty(),
        "list_agents should not require arguments"
    );
}

#[tokio::test]
async fn switch_model_tool_is_only_exposed_to_svarog_scope() {
    let config = AgentConfig::default();
    let temp_dir = tempfile::tempdir().expect("tempdir");

    let svarog_tools = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        async { get_available_tools(&config, temp_dir.path(), false) },
    )
    .await;
    let switch_model = svarog_tools
        .iter()
        .find(|tool| tool.function.name == "switch_model")
        .expect("switch_model should be available to svarog");
    let switch_model_properties = switch_model
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("switch_model schema should expose properties object");
    for field in ["agent", "provider", "model"] {
        assert!(
            switch_model_properties.contains_key(field),
            "switch_model schema should include {field}"
        );
    }
    let switch_model_required = switch_model
        .function
        .parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .collect::<Vec<_>>()
        })
        .expect("switch_model should define required fields");
    assert_eq!(switch_model_required, vec!["agent", "provider", "model"]);

    let rarog_tools = crate::agent::agent_identity::run_with_agent_scope(
        crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
        async { get_available_tools(&config, temp_dir.path(), false) },
    )
    .await;
    assert!(
        rarog_tools
            .iter()
            .all(|tool| tool.function.name != "switch_model"),
        "switch_model should be hidden outside svarog scope"
    );
}

#[test]
fn apply_patch_tool_uses_top_level_object_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);
    let apply_patch = tools
        .iter()
        .find(|tool| tool.function.name == "apply_patch")
        .expect("apply_patch tool should be available");

    assert_eq!(
        apply_patch.function.parameters.get("type"),
        Some(&serde_json::json!("object"))
    );
    for forbidden_key in ["oneOf", "anyOf", "allOf", "enum", "not"] {
        assert!(
            apply_patch.function.parameters.get(forbidden_key).is_none(),
            "apply_patch schema must not use top-level {forbidden_key}"
        );
    }

    let properties = apply_patch
        .function
        .parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("apply_patch schema should expose top-level properties");

    assert!(
        properties.get("input").is_some(),
        "schema should include harness patch input"
    );
    assert!(
        properties.get("patch").is_some(),
        "schema should include patch alias"
    );
    assert!(
        properties.get("path").is_none(),
        "apply_patch schema should not advertise legacy exact-replacement path mode"
    );
    assert!(
        properties.get("edits").is_none(),
        "apply_patch schema should not advertise legacy exact-replacement edits mode"
    );
}

#[test]
fn scrub_sensitive_redacts_common_api_key_lines() {
    let input = "openai api_key=sk-live-secret\nAuthorization: Bearer abc123secret";
    let scrubbed = crate::scrub::scrub_sensitive(input);
    assert!(!scrubbed.contains("sk-live-secret"));
    assert!(!scrubbed.contains("abc123secret"));
    assert!(scrubbed.contains("***REDACTED***"));
}

#[tokio::test]
async fn divergent_tool_get_session_serializes_completion_fields() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let session_id = engine
        .start_divergent_session("pick caching strategy", None, "thread-tool-div", None)
        .await
        .expect("start divergent session");
    let labels = {
        let sessions = engine.divergent_sessions.read().await;
        sessions
            .get(&session_id)
            .expect("session exists")
            .framings
            .iter()
            .map(|f| f.label.clone())
            .collect::<Vec<_>>()
    };
    for (idx, label) in labels.iter().enumerate() {
        engine
            .record_divergent_contribution(
                &session_id,
                label,
                if idx == 0 {
                    "Prefer deterministic correctness-first approach"
                } else {
                    "Prefer lower-latency pragmatic approach"
                },
            )
            .await
            .expect("record contribution");
    }
    engine
        .complete_divergent_session(&session_id)
        .await
        .expect("complete divergent session");

    let response =
        execute_get_divergent_session(&serde_json::json!({ "session_id": session_id }), &engine)
            .await
            .expect("tool execution should succeed");
    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("tool payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("complete")
    );
    assert!(payload
        .get("tensions_markdown")
        .and_then(|v| v.as_str())
        .is_some_and(|value| !value.is_empty()));
    assert!(payload
        .get("mediator_prompt")
        .and_then(|v| v.as_str())
        .is_some_and(|value| !value.is_empty()));
}

#[tokio::test]
async fn divergent_tool_get_session_in_progress_omits_completion_output() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let session_id = engine
        .start_divergent_session("evaluate rollout sequence", None, "thread-tool-div-2", None)
        .await
        .expect("start divergent session");

    let response =
        execute_get_divergent_session(&serde_json::json!({ "session_id": session_id }), &engine)
            .await
            .expect("tool execution should succeed");
    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("tool payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("running")
    );
    assert!(
        payload
            .get("tensions_markdown")
            .is_some_and(|v| v.is_null()),
        "in-progress session should not report tensions output"
    );
    assert!(
        payload.get("mediator_prompt").is_some_and(|v| v.is_null()),
        "in-progress session should not report mediator output"
    );
    let progress = payload
        .get("framing_progress")
        .expect("framing_progress should exist");
    assert_eq!(progress.get("completed").and_then(|v| v.as_u64()), Some(0));
}

#[tokio::test]
async fn debate_tools_are_exposed_with_expected_schema() {
    let config = AgentConfig::default();
    let temp_dir = std::env::temp_dir();
    let tools = get_available_tools(&config, &temp_dir, false);

    for tool_name in [
        "run_debate",
        "get_debate_session",
        "append_debate_argument",
        "advance_debate_round",
        "complete_debate_session",
    ] {
        assert!(
            tools.iter().any(|tool| tool.function.name == tool_name),
            "{tool_name} tool should be available"
        );
    }
}

#[tokio::test]
async fn debate_tool_run_and_get_session_round_trip() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let response = execute_run_debate(
        &serde_json::json!({ "topic": "choose rollout strategy" }),
        &engine,
        "thread-tool-debate",
        None,
    )
    .await
    .expect("run_debate should succeed");
    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("run_debate payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("started")
    );
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .expect("session_id should exist")
        .to_string();

    let get_response =
        execute_get_debate_session(&serde_json::json!({ "session_id": session_id }), &engine)
            .await
            .expect("get_debate_session should succeed");
    let get_payload: serde_json::Value = serde_json::from_str(&get_response)
        .expect("get_debate_session payload should be valid JSON");
    assert_eq!(
        get_payload.get("status").and_then(|v| v.as_str()),
        Some("in_progress")
    );
    assert_eq!(
        get_payload.get("current_round").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert!(get_payload
        .get("arguments")
        .and_then(|v| v.as_array())
        .is_some_and(|arguments| {
            arguments.len() == 3
                && arguments
                    .iter()
                    .any(|argument| argument["role"] == "proponent")
                && arguments
                    .iter()
                    .any(|argument| argument["role"] == "skeptic")
                && arguments
                    .iter()
                    .any(|argument| argument["role"] == "synthesizer")
        }));
    assert!(get_payload
        .get("roles")
        .and_then(|v| v.as_array())
        .is_some_and(|roles| !roles.is_empty()));

    let complete_response =
        execute_complete_debate_session(&serde_json::json!({ "session_id": session_id }), &engine)
            .await
            .expect("complete_debate_session should succeed");
    let complete_payload: serde_json::Value = serde_json::from_str(&complete_response)
        .expect("complete_debate_session payload should be valid JSON");
    assert_eq!(
        complete_payload
            .get("completion_reason")
            .and_then(|v| v.as_str()),
        Some("manual_completion")
    );
}

#[tokio::test]
async fn run_divergent_mode_debate_starts_debate_session() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let response = execute_run_divergent(
        &serde_json::json!({
            "problem_statement": "decide rollout strategy",
            "mode": "debate"
        }),
        &engine,
        "thread-run-divergent-debate",
        None,
    )
    .await
    .expect("run_divergent(mode=debate) should succeed");

    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("started")
    );
    assert_eq!(payload.get("mode").and_then(|v| v.as_str()), Some("debate"));

    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .expect("session_id should exist")
        .to_string();

    let debate_payload = engine
        .get_debate_session_payload(&session_id)
        .await
        .expect("debate session should be retrievable");
    assert_eq!(
        debate_payload.get("status").and_then(|v| v.as_str()),
        Some("in_progress")
    );
    assert_eq!(
        debate_payload.get("topic").and_then(|v| v.as_str()),
        Some("decide rollout strategy")
    );
}

#[tokio::test]
async fn run_divergent_default_mode_remains_divergent() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let response = execute_run_divergent(
        &serde_json::json!({
            "problem_statement": "compare indexing plans"
        }),
        &engine,
        "thread-run-divergent-default",
        None,
    )
    .await
    .expect("run_divergent default mode should succeed");

    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("started")
    );
    assert_eq!(
        payload.get("mode").and_then(|v| v.as_str()),
        Some("divergent")
    );

    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .expect("session_id should exist");
    let divergent_payload = engine
        .get_divergent_session(session_id)
        .await
        .expect("divergent session should exist");
    assert_eq!(
        divergent_payload.get("status").and_then(|v| v.as_str()),
        Some("running")
    );
}

#[tokio::test]
async fn route_to_specialist_surfaces_routing_rationale_in_dispatch_result() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.routing.enabled = true;
    config.routing.method = crate::agent::types::RoutingMode::Probabilistic;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let response = execute_route_to_specialist(
        &serde_json::json!({
            "task_description": "Investigate the regression and summarize likely causes.",
            "capability_tags": ["research", "analysis"],
            "acceptance_criteria": "non_empty"
        }),
        &engine,
        "thread-routing-rationale",
        None,
    )
    .await
    .expect("route_to_specialist should succeed");

    let payload: serde_json::Value =
        serde_json::from_str(&response).expect("route_to_specialist payload should be valid JSON");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("dispatched")
    );
    assert!(payload
        .get("routing_rationale")
        .and_then(|v| v.as_str())
        .is_some_and(|text| {
            text.contains("routing") || text.contains("fallback") || text.contains("score")
        }));
    assert!(payload
        .get("specialization_diagnostics")
        .and_then(|v| v.as_object())
        .is_some_and(|diag| {
            diag.get("matched_capability_tags")
                .and_then(|v| v.as_array())
                .is_some_and(|tags| !tags.is_empty())
        }));
    assert!(payload
        .get("specialization_diagnostics")
        .and_then(|v| v.as_object())
        .and_then(|diag| diag.get("routing_confidence"))
        .and_then(|v| v.as_object())
        .is_some_and(|confidence| {
            confidence.get("score").and_then(|v| v.as_f64()).is_some()
                && confidence
                    .get("threshold")
                    .and_then(|v| v.as_f64())
                    .is_some()
                && confidence
                    .get("cleared_threshold")
                    .and_then(|v| v.as_bool())
                    .is_some()
        }));
}

#[tokio::test]
async fn send_slack_message_emits_gateway_ipc_request() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.init_gateway().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_slack_message",
            &serde_json::json!({
                "channel": "C123",
                "message": "hello from daemon",
                "thread_ts": "1712345678.000100"
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "slack");
    assert_eq!(request.channel_id, "C123");
    assert_eq!(request.thread_id.as_deref(), Some("1712345678.000100"));
    assert_eq!(request.content, "hello from daemon");

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            requested_channel_id: Some("C123".to_string()),
            delivery_id: Some("1712345678.000200".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Slack message sent to #C123");
}

#[tokio::test]
async fn send_slack_message_uses_auto_injected_reply_context_for_channel() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.init_gateway().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    {
        let mut gw_guard = engine.gateway_state.lock().await;
        let gw = gw_guard.as_mut().expect("gateway state should exist");
        gw.reply_contexts.insert(
            "Slack:C123".to_string(),
            crate::agent::gateway::ThreadContext {
                slack_thread_ts: Some("1712345678.000100".to_string()),
                ..Default::default()
            },
        );
    }

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_slack_message",
            &serde_json::json!({
                "channel": "C123",
                "message": "hello from daemon"
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "slack");
    assert_eq!(request.channel_id, "C123");
    assert_eq!(request.thread_id.as_deref(), Some("1712345678.000100"));

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            requested_channel_id: Some("C123".to_string()),
            delivery_id: Some("1712345678.000200".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Slack message sent to #C123");
}

#[tokio::test]
async fn send_slack_message_blocks_duplicate_after_successful_send() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.init_gateway().await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let first_engine = engine.clone();
    let first_send = tokio::spawn(async move {
        execute_gateway_message(
            "send_slack_message",
            &serde_json::json!({
                "channel": "C123",
                "message": "Still working on it."
            }),
            &first_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let first_request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("first gateway send request should be emitted")
        .expect("first gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: first_request.correlation_id.clone(),
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            requested_channel_id: Some("C123".to_string()),
            delivery_id: Some("1712345678.000200".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    first_send
        .await
        .expect("first send task should join")
        .expect("first send should succeed");

    let duplicate = execute_gateway_message(
        "send_slack_message",
        &serde_json::json!({
            "channel": "C123",
            "message": "Still working on it."
        }),
        &engine,
        &reqwest::Client::new(),
    )
    .await
    .expect_err("duplicate gateway send should be blocked");

    assert!(
        duplicate
            .to_string()
            .contains("same message you already sent successfully"),
        "duplicate block should explain the prior successful send: {duplicate}"
    );
    assert!(
        timeout(Duration::from_millis(100), rx.recv())
            .await
            .is_err(),
        "blocked duplicate should not emit a second gateway request"
    );
}

#[tokio::test]
async fn send_discord_message_emits_gateway_ipc_request() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let send_engine = engine.clone();
    let send_task = tokio::spawn(async move {
        execute_gateway_message(
            "send_discord_message",
            &serde_json::json!({
                "user_id": "123456789",
                "message": "discord reply",
                "reply_to_message_id": "987654321"
            }),
            &send_engine,
            &reqwest::Client::new(),
        )
        .await
    });

    let request = match timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert_eq!(request.thread_id.as_deref(), Some("987654321"));
    assert_eq!(request.content, "discord reply");

    engine
        .complete_gateway_send_result(GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: 1,
        })
        .await;

    let result = send_task
        .await
        .expect("send task should join")
        .expect("send should succeed");
    assert_eq!(result, "Discord message sent to user:123456789");
}
