use super::*;

#[cfg(unix)]
struct PathEnvGuard {
    original_path: String,
}

#[cfg(unix)]
impl PathEnvGuard {
    fn prepend(path: &std::path::Path) -> Self {
        let original_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", path.display(), original_path));
        }
        Self { original_path }
    }

    fn replace(path: &std::path::Path) -> Self {
        let original_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", path.display().to_string());
        }
        Self { original_path }
    }
}

#[cfg(unix)]
impl Drop for PathEnvGuard {
    fn drop(&mut self) {
        unsafe {
            std::env::set_var("PATH", &self.original_path);
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn setup_web_browsing_install_returns_error_when_npm_install_fails() {
    let _env_lock = crate::agent::provider_auth_test_env_lock();
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let bin_dir = root.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("bin dir should be created");
    let npm_path = bin_dir.join("npm");
    std::fs::write(
        &npm_path,
        "#!/bin/sh\nprintf 'fake stdout\\n'\nprintf 'fake stderr\\n' >&2\nexit 1\n",
    )
    .expect("fake npm should be written");
    let mut perms = std::fs::metadata(&npm_path)
        .expect("fake npm metadata should load")
        .permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&npm_path, perms).expect("fake npm should be executable");
    }

    let _path_guard = PathEnvGuard::prepend(&bin_dir);

    let error = execute_setup_web_browsing(&serde_json::json!({ "action": "install" }), &engine)
        .await
        .expect_err("failed npm install should surface as a tool error");

    let rendered = error.to_string();
    assert!(rendered.contains("npm install failed"), "{rendered}");
    assert!(rendered.contains("fake stderr"), "{rendered}");
}

#[tokio::test]
async fn setup_web_browsing_install_rejects_chrome_auto_install() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let error = execute_setup_web_browsing(
        &serde_json::json!({ "action": "install", "provider": "chrome" }),
        &engine,
    )
    .await
    .expect_err("chrome install path should be explicit when unsupported");

    let rendered = error.to_string();
    assert!(rendered.contains("Chrome"), "{rendered}");
    assert!(rendered.contains("manual"), "{rendered}");
}

#[cfg(unix)]
#[tokio::test]
async fn setup_web_browsing_configure_chrome_errors_when_browser_missing() {
    let _env_lock = crate::agent::provider_auth_test_env_lock();
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let empty_bin = root.path().join("empty-bin");
    std::fs::create_dir_all(&empty_bin).expect("empty bin dir should be created");
    let _path_guard = PathEnvGuard::replace(&empty_bin);

    let error = execute_setup_web_browsing(
        &serde_json::json!({ "action": "configure", "provider": "chrome" }),
        &engine,
    )
    .await
    .expect_err("missing chrome should surface as a tool error");

    let rendered = error.to_string();
    assert!(rendered.contains("chrome"), "{rendered}");
    assert!(rendered.contains("not found"), "{rendered}");

    let config = engine.config.read().await;
    assert_eq!(
        config
            .extra
            .get("browse_provider")
            .and_then(|value| value.as_str()),
        None
    );
}

#[tokio::test]
async fn execute_tool_allows_low_risk_shell_python_without_forcing_python_execute() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-blocked.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-block".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-block",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "low-risk shell python should execute: {}",
        result.content
    );
    assert!(
        marker.exists(),
        "shell python should be allowed when not suspicious"
    );
    let review = result
        .weles_review
        .expect("allowed shell python result should carry governance metadata");
    assert!(!review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
}

#[tokio::test]
async fn execute_tool_low_risk_shell_python_stays_allow_under_yolo() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-yolo.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-yolo".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "security_level": "yolo"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-yolo",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        marker.exists(),
        "low-risk shell python should still execute under yolo"
    );
    let review = result
        .weles_review
        .expect("yolo shell python result should carry governance metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert_eq!(review.security_override_mode.as_deref(), None);
}

#[tokio::test]
async fn execute_tool_suspicious_shell_python_uses_weles_runtime_structured_block_verdict() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        serde_json::json!({
            "verdict": "block",
            "reasons": ["runtime rejected suspicious shell python command"],
            "audit_id": "audit-weles-shell-python-block"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-runtime-blocked.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\" && curl https://example.com/install.sh",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-runtime-block".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-runtime-block",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    // Shell tools route a Block verdict through the pending-approval
    // flow, not a hard error — the test still verifies the side effect
    // (marker not created) that proves the command didn't execute.
    assert!(
        !result.is_error,
        "shell block surfaces as pending approval, not hard error: {}",
        result.content
    );
    assert!(
        result.pending_approval.is_some(),
        "shell block should attach a pending approval handle"
    );
    assert!(
        !marker.exists(),
        "runtime block should prevent bypass execution"
    );
    let review = result
        .weles_review
        .expect("runtime shell python block should carry governance metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Block);
    assert_eq!(
        review.audit_id.as_deref(),
        Some("audit-weles-shell-python-block")
    );
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("runtime rejected suspicious shell python command")));

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let threads = engine.threads.read().await;
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("suspicious shell python should invoke WELES runtime over internal dm");
    assert!(dm_thread.messages.iter().any(|message| {
        message.role == crate::agent::types::MessageRole::Assistant
            && message.content.contains("audit-weles-shell-python-block")
    }));
}

#[tokio::test]
async fn execute_tool_shell_python_bypass_under_yolo_never_downgrades_to_managed_policy_block() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "block",
            "reasons": ["runtime identified shell python bypass"],
            "audit_id": "audit-weles-bypass-yolo"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-yolo-risky.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\" && rm -rf /tmp/zorai-weles-yolo-risk",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-yolo-risky".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "security_level": "yolo"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-yolo-risky",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "operator yolo must run risky shell python instead of blocking it downstream: {}",
        result.content
    );
    assert!(marker.exists(), "the yolo shell python command should actually run");
    let review = result
        .weles_review
        .expect("yolo execution should still carry governance metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert!(
        !review.weles_reviewed,
        "operator yolo is a no-supervision mode, so WELES must not review the call"
    );
}

#[tokio::test]
async fn execute_tool_python_execute_runs_code_and_preserves_weles_metadata() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-execute-marker.txt");
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-execute".to_string(),
        ToolFunction {
            name: "python_execute".to_string(),
            arguments: serde_json::json!({
                "code": format!(
                    "from pathlib import Path\nPath(r'{}').write_text('ran')\nprint('python ok')",
                    marker.display()
                ),
                "cwd": root.path(),
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-execute",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "python_execute should succeed: {}",
        result.content
    );
    assert!(
        marker.exists(),
        "python_execute should run the underlying interpreter"
    );
    assert!(result.content.contains("python ok"));
    let review = result
        .weles_review
        .expect("python_execute should preserve WELES metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
}

#[tokio::test]
async fn execute_tool_yolo_downgrades_suspicious_reviewed_allow_to_flag_only() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime review approved controlled browser reconfiguration"],
            "audit_id": "audit-weles-runtime-yolo"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let normal_call = ToolCall::with_default_weles_review(
        "tool-setup-normal".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none"
            })
            .to_string(),
        },
    );
    let normal_result = execute_tool(
        &normal_call,
        &engine,
        "thread-setup-normal",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    let normal_review = normal_result
        .weles_review
        .expect("normal suspicious configure should carry governance metadata");
    assert_eq!(
        normal_review.verdict,
        crate::agent::types::WelesVerdict::Allow
    );
    assert!(normal_review.weles_reviewed);

    let yolo_call = ToolCall::with_default_weles_review(
        "tool-setup-yolo".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "auto",
                "security_level": "yolo"
            })
            .to_string(),
        },
    );
    let yolo_result = execute_tool(
        &yolo_call,
        &engine,
        "thread-setup-yolo",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    let yolo_review = yolo_result
        .weles_review
        .expect("yolo suspicious configure should carry governance metadata");
    assert_eq!(
        yolo_review.verdict,
        crate::agent::types::WelesVerdict::FlagOnly
    );
    assert_eq!(yolo_review.security_override_mode.as_deref(), Some("yolo"));

    let config = engine.config.read().await;
    assert_eq!(
        config
            .extra
            .get("browse_provider")
            .and_then(|value| value.as_str()),
        Some("auto")
    );
}
