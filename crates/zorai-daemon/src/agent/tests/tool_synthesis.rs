use super::*;
use tempfile::tempdir;

#[test]
fn parse_cli_help_extracts_long_flags() {
    let help = "\
Usage: demo [OPTIONS]\n\
\n\
Options:\n\
  -n, --namespace <NAMESPACE>  Namespace to inspect\n\
      --all                    Include everything\n";
    let params = parse_cli_help_parameters(help);
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "namespace");
    assert_eq!(params[0].param_type, "string");
    assert_eq!(params[1].name, "all");
    assert_eq!(params[1].param_type, "boolean");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_maps_safe_unknown_tool_name() {
    let proposal = super::detect_cli_wrapper_synthesis_proposal("cargo_check")
        .expect("safe cargo subcommand should produce a CLI wrapper proposal");

    assert_eq!(proposal.tool_name, "cargo_check");
    assert_eq!(proposal.target, "cargo check");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_from_command_maps_safe_readonly_shell_command() {
    let proposal = super::detect_cli_wrapper_synthesis_proposal_from_command("git status --short")
        .expect("safe readonly shell command should produce a CLI wrapper proposal");

    assert_eq!(proposal.tool_name, "git_status");
    assert_eq!(proposal.target, "git status");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_from_command_accepts_safe_flag_values() {
    let proposal =
        super::detect_cli_wrapper_synthesis_proposal_from_command("cargo test -p zorai-daemon")
            .expect(
            "safe readonly shell command with flag values should produce a CLI wrapper proposal",
        );

    assert_eq!(proposal.tool_name, "cargo_test");
    assert_eq!(proposal.target, "cargo test");
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_rejects_mutating_tokens() {
    assert!(super::detect_cli_wrapper_synthesis_proposal("cargo_install").is_none());
}

#[test]
fn detect_cli_wrapper_synthesis_proposal_from_command_rejects_complex_or_mutating_shell() {
    assert!(
        super::detect_cli_wrapper_synthesis_proposal_from_command("cargo install ripgrep")
            .is_none()
    );
    assert!(
        super::detect_cli_wrapper_synthesis_proposal_from_command("git status | cat").is_none()
    );
}

#[test]
fn equivalent_generated_cli_tool_matches_target_and_ignores_archived_records() -> Result<()> {
    let agent_data_dir = std::env::temp_dir().join(format!(
        "zorai-generated-tools-existing-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&agent_data_dir)?;

    let proposal = super::detect_cli_wrapper_synthesis_proposal_from_command("git status --short")
        .expect("proposal should parse");

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "git_status_existing".to_string(),
            name: "git_status_existing".to_string(),
            description: "existing tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["git".to_string(), "status".to_string()],
                help_source: "git status".to_string(),
            }),
            openapi: None,
        },
    )?;

    assert!(super::has_equivalent_generated_cli_tool(
        &agent_data_dir,
        &proposal
    )?);
    let existing = super::find_equivalent_generated_cli_tool(&agent_data_dir, &proposal)?
        .expect("existing equivalent generated tool metadata");
    assert_eq!(
        existing.get("status").and_then(|value| value.as_str()),
        Some("active")
    );

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "git_status_archived".to_string(),
            name: "git_status_archived".to_string(),
            description: "archived tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "archived".to_string(),
            created_at: 2,
            updated_at: 2,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["git".to_string(), "status".to_string()],
                help_source: "git status".to_string(),
            }),
            openapi: None,
        },
    )?;

    let _ = std::fs::remove_dir_all(&agent_data_dir);
    Ok(())
}

#[test]
fn prune_generated_tools_keeps_active_and_promoted_records() -> Result<()> {
    let agent_data_dir = std::env::temp_dir().join(format!(
        "zorai-generated-tools-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&agent_data_dir)?;
    for (id, status, updated_at) in [
        ("tool-new-old", "new", 10),
        ("tool-active", "active", 11),
        ("tool-promoted", "promoted", 12),
        ("tool-new-fresh", "new", 13),
    ] {
        save_generated_tool(
            &agent_data_dir,
            &GeneratedToolRecord {
                id: id.to_string(),
                name: id.to_string(),
                description: id.to_string(),
                kind: GeneratedToolKind::Cli,
                parameters: Vec::new(),
                status: status.to_string(),
                created_at: updated_at,
                updated_at,
                last_used_at: None,
                calls_total: 0,
                calls_success: 0,
                calls_failure: 0,
                calls_timeout: 0,
                sessions_used: 0,
                last_session_key: None,
                promoted_skill_path: None,
                cli: Some(GeneratedCliSpec {
                    invocation: vec!["echo".to_string()],
                    help_source: "help".to_string(),
                }),
                openapi: None,
            },
        )?;
    }

    prune_generated_tools(&agent_data_dir, 3)?;
    let remaining = load_generated_tools(&agent_data_dir)?
        .into_iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    assert!(remaining.iter().any(|id| id == "tool-active"));
    assert!(remaining.iter().any(|id| id == "tool-promoted"));
    assert!(!remaining.iter().any(|id| id == "tool-new-old"));
    let _ = std::fs::remove_dir_all(&agent_data_dir);
    Ok(())
}

#[tokio::test]
async fn synthesize_openapi_tool_reuses_equivalent_existing_generated_tool_record() -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let spec_body = serde_json::json!({
        "openapi": "3.0.0",
        "info": { "title": "Demo API", "version": "1.0.0" },
        "servers": [{ "url": "https://api.example.test" }],
        "paths": {
            "/status": {
                "get": {
                    "operationId": "getStatus",
                    "summary": "Get current status",
                    "responses": {
                        "200": { "description": "ok" }
                    }
                }
            }
        }
    })
    .to_string();

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind openapi server");
    let addr = listener.local_addr().expect("openapi addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let spec_body = spec_body.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let _ = socket.read(&mut buffer).await.expect("read spec request");
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    spec_body.len(),
                    spec_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write spec response");
            });
        }
    });

    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.tool_synthesis.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let spec_url = format!("http://{addr}/openapi.json");
    let first = engine
        .synthesize_tool_json(
            &serde_json::json!({
                "kind": "openapi",
                "target": spec_url,
                "operation_id": "getStatus",
                "name": "getstatus",
                "activate": false,
            })
            .to_string(),
        )
        .await
        .expect("first synthesize generated OpenAPI tool");
    let first_record: serde_json::Value =
        serde_json::from_str(&first).expect("parse first synth record");

    let second = engine
        .synthesize_tool_json(
            &serde_json::json!({
                "kind": "openapi",
                "target": spec_url,
                "operation_id": "getStatus",
                "name": "getstatus_duplicate",
                "activate": false,
            })
            .to_string(),
        )
        .await
        .expect("second synthesize generated OpenAPI tool");
    let second_record: serde_json::Value =
        serde_json::from_str(&second).expect("parse second synth record");

    assert_eq!(
        second_record.get("id").and_then(|value| value.as_str()),
        first_record.get("id").and_then(|value| value.as_str())
    );
    assert_eq!(
        second_record
            .get("openapi")
            .and_then(|value| value.get("operation_id"))
            .and_then(|value| value.as_str()),
        Some("getStatus")
    );

    let tools: Vec<serde_json::Value> = serde_json::from_str(
        &engine
            .list_generated_tools_json()
            .await
            .expect("list generated tools json"),
    )
    .expect("parse generated tools list");
    let matching = tools
        .iter()
        .filter(|tool| {
            tool.get("openapi")
                .and_then(|value| value.get("spec_url"))
                .and_then(|value| value.as_str())
                == Some(spec_url.as_str())
                && tool
                    .get("openapi")
                    .and_then(|value| value.get("operation_id"))
                    .and_then(|value| value.as_str())
                    == Some("getStatus")
        })
        .count();
    assert_eq!(
        matching, 1,
        "equivalent direct OpenAPI synthesis requests should not create duplicate generated tools"
    );

    Ok(())
}

#[tokio::test]
async fn cli_generated_tool_execution_is_blocked_when_filesystem_disabled() {
    let record = GeneratedToolRecord {
        id: "tool-echo".to_string(),
        name: "tool-echo".to_string(),
        description: "Echo test tool".to_string(),
        kind: GeneratedToolKind::Cli,
        parameters: Vec::new(),
        status: "active".to_string(),
        created_at: 1,
        updated_at: 1,
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: Some(GeneratedCliSpec {
            invocation: vec!["ls".to_string()],
            help_source: "help".to_string(),
        }),
        openapi: None,
    };

    let mut sandbox = ToolSynthesisSandboxConfig::default();
    sandbox.allow_filesystem = false;

    let error = run_cli_generated_tool(&record, &serde_json::json!({}), &sandbox, None)
        .await
        .expect_err("CLI generated tools should be blocked when filesystem access is disabled");

    assert!(
        error.to_string().contains("filesystem access"),
        "expected filesystem guard error, got: {error}"
    );
}

#[tokio::test]
async fn sandboxed_cli_generated_tool_cannot_read_files_outside_its_folder() -> Result<()> {
    if !std::process::Command::new("which")
        .arg("bwrap")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let outside = tempdir()?;
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "zorai-secret-value")?;
    let sandbox_root = tempdir()?;
    let record = GeneratedToolRecord {
        id: "tool-cat".to_string(),
        name: "tool-cat".to_string(),
        description: "Cat test tool".to_string(),
        kind: GeneratedToolKind::Cli,
        parameters: Vec::new(),
        status: "active".to_string(),
        created_at: 1,
        updated_at: 1,
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: Some(GeneratedCliSpec {
            invocation: vec!["cat".to_string(), secret.to_string_lossy().to_string()],
            help_source: "help".to_string(),
        }),
        openapi: None,
    };
    let mut sandbox = ToolSynthesisSandboxConfig::default();
    sandbox.allow_filesystem = true;

    let unsandboxed =
        run_cli_generated_tool(&record, &serde_json::json!({}), &sandbox, None).await?;
    assert!(unsandboxed.contains("zorai-secret-value"));
    let sandboxed = run_cli_generated_tool(
        &record,
        &serde_json::json!({}),
        &sandbox,
        Some(sandbox_root.path()),
    )
    .await?;
    assert!(
        !sandboxed.contains("zorai-secret-value"),
        "a sandboxed generated tool must not see files outside its folder: {sandboxed}"
    );
    Ok(())
}

#[test]
fn promote_generated_tool_requires_reviewed_status() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "zorai-generated-tools-promote-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-new".to_string(),
            name: "tool-new".to_string(),
            description: "Fresh generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "new".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let error = promote_generated_tool(&agent_data_dir, "tool-new")
        .expect_err("new generated tools should not be promotable before review");

    assert!(
        error.to_string().contains("not ready for promotion"),
        "expected promotion gate error, got: {error}"
    );

    assert!(
        generated_tools_dir(&agent_data_dir)
            .join("tool-new")
            .join("tool.json")
            .exists(),
        "rejected promotion should not remove the generated tool record"
    );
    assert!(
        !super::skills_dir(&agent_data_dir)
            .join("generated")
            .join("use-tool-new.md")
            .exists(),
        "rejected promotion should not create a promoted skill artifact"
    );

    let _ = std::fs::remove_dir_all(&agent_data_dir);
    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}

#[test]
fn retire_generated_tool_marks_tool_archived_and_removes_promoted_skill_artifact() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "zorai-generated-tools-retire-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    let promoted_skill_path = super::skills_dir(&agent_data_dir)
        .join("generated")
        .join("use-tool-retire.md");
    std::fs::create_dir_all(
        promoted_skill_path
            .parent()
            .expect("generated skill parent"),
    )?;
    std::fs::write(&promoted_skill_path, "# generated tool skill\n")?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-retire".to_string(),
            name: "tool-retire".to_string(),
            description: "Retirable generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "promoted".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: Some(promoted_skill_path.display().to_string()),
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let retired_json = retire_generated_tool(&agent_data_dir, "tool-retire")?;
    let retired: serde_json::Value = serde_json::from_str(&retired_json)?;

    assert_eq!(retired["status"], "archived");
    assert!(retired["promoted_skill_path"].is_null());
    assert!(
        !promoted_skill_path.exists(),
        "retiring a promoted generated tool should remove its promoted skill artifact"
    );

    let saved = load_generated_tool(&agent_data_dir, "tool-retire")?
        .expect("retired tool should remain in registry");
    assert_eq!(saved.status, "archived");
    assert!(saved.promoted_skill_path.is_none());

    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}
#[test]
fn restore_generated_tool_reactivates_archived_tool_without_repromoting() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "zorai-generated-tools-restore-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-restore".to_string(),
            name: "tool-restore".to_string(),
            description: "Archived generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "archived".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let restored_json = restore_generated_tool(&agent_data_dir, "tool-restore")?;
    let restored: serde_json::Value = serde_json::from_str(&restored_json)?;
    assert_eq!(restored["status"], "active");
    assert!(restored["promoted_skill_path"].is_null());

    let saved = load_generated_tool(&agent_data_dir, "tool-restore")?
        .expect("restored tool should remain in registry");
    assert_eq!(saved.status, "active");
    assert!(saved.promoted_skill_path.is_none());

    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}

#[test]
fn restore_generated_tool_rejects_non_archived_status() -> Result<()> {
    let test_root = std::env::temp_dir().join(format!(
        "zorai-generated-tools-restore-invalid-test-{}",
        uuid::Uuid::new_v4()
    ));
    let agent_data_dir = test_root.join("agent");
    std::fs::create_dir_all(&agent_data_dir)?;

    save_generated_tool(
        &agent_data_dir,
        &GeneratedToolRecord {
            id: "tool-active".to_string(),
            name: "tool-active".to_string(),
            description: "Active generated tool".to_string(),
            kind: GeneratedToolKind::Cli,
            parameters: Vec::new(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_used_at: None,
            calls_total: 0,
            calls_success: 0,
            calls_failure: 0,
            calls_timeout: 0,
            sessions_used: 0,
            last_session_key: None,
            promoted_skill_path: None,
            cli: Some(GeneratedCliSpec {
                invocation: vec!["echo".to_string()],
                help_source: "help".to_string(),
            }),
            openapi: None,
        },
    )?;

    let error = restore_generated_tool(&agent_data_dir, "tool-active")
        .expect_err("non-archived generated tools should not be restorable");
    assert!(error
        .to_string()
        .contains("only archived generated tools can be restored"));

    let _ = std::fs::remove_dir_all(&test_root);
    Ok(())
}
