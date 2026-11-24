use super::*;
use crate::test_support::repo_root;
#[test]
pub(crate) fn daemon_boxes_large_gateway_hot_path_futures() {
    let root = repo_root();
    let source = [
        root.join("crates/zorai-daemon/src/server.rs"),
        root.join("crates/zorai-daemon/src/server/post_tests.rs"),
        root.join("crates/zorai-daemon/src/server/dispatch_part3.rs"),
        root.join("crates/zorai-daemon/src/server/dispatch_part7.rs"),
    ]
    .into_iter()
    .map(|path| fs::read_to_string(path).expect("read split server source"))
    .collect::<Vec<_>>()
    .join("\n");

    // Each required boxing site is matched against a *normalized* form
    // of the source where contiguous whitespace collapses to one space.
    // This keeps the guard rail robust to rustfmt re-flowing lines
    // (e.g., joining `agent\n    .send_internal_delegate_message(` onto
    // a single line) while still checking the actual `Box::pin` is
    // wrapping the right call.
    let normalized_source = source.split_whitespace().collect::<Vec<_>>().join(" ");
    for required in [
        "Box::pin(startup_agent.run_loop(shutdown_rx)).await;",
        "Box::pin(handle_connection(",
        "if let Err(e) = Box::pin(agent.send_message_with_session_surface_and_target(",
        "match Box::pin(agent.send_direct_message(",
        "if let Err(error) = Box::pin(agent.send_internal_delegate_message(",
        "spawn_background_operation( BackgroundSubsystem::ConfigReconcile, Some(operation.operation_id.clone()), background_daemon_tx, background_daemon_pending, Box::pin(async move {",
    ] {
        let normalized_required = required
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            normalized_source.contains(&normalized_required),
            "server hot path should box oversized future: {required}"
        );
    }
}

#[test]
pub(crate) fn daemon_binds_ipc_listener_before_hydrate_starts() {
    let root = repo_root();
    let source = fs::read_to_string(root.join("crates/zorai-daemon/src/server/post_tests.rs"))
        .expect("read server startup source");

    let bind_idx = source
        .find("let listener = bind_unix_listener(&path)?;")
        .expect("startup should bind Unix listener");
    let hydrate_idx = source
        .find("hydrate_without_participant_observer_restore()")
        .expect("startup should hydrate agent state");

    assert!(
        bind_idx < hydrate_idx,
        "daemon must bind its IPC listener before hydrate work starts"
    );
}

#[test]
pub(crate) fn daemon_marks_startup_ready_before_scheduling_participant_observer_restore() {
    let root = repo_root();
    let source = fs::read_to_string(root.join("crates/zorai-daemon/src/server/post_tests.rs"))
        .expect("read server startup source");

    let ready_idx = source
        .find("startup_readiness_for_task.mark_ready();")
        .expect("startup should mark readiness");
    let restore_idx = source
        .find("startup_agent.schedule_participant_observer_restore_after_hydrate();")
        .expect("startup should defer participant observer restore");

    assert!(
        ready_idx < restore_idx,
        "daemon should defer participant observer restore until after startup readiness"
    );
}

#[test]
pub(crate) fn concierge_welcome_starts_before_persisted_history_hydration() {
    let root = repo_root();
    let source = fs::read_to_string(root.join("crates/zorai-daemon/src/server/dispatch_part7.rs"))
        .expect("read concierge dispatch source");

    assert!(
        !source.contains("agent.ensure_thread_messages_loaded(thread_id).await;"),
        "concierge welcome must not synchronously hydrate persisted thread messages"
    );
}

use crate::agent::types::SubAgentDefinition;

pub(crate) struct TestConnection {
    pub(crate) framed: Framed<DuplexStream, ZoraiCodec>,
    pub(crate) task: JoinHandle<anyhow::Result<()>>,
    pub(crate) root: PathBuf,
    pub(crate) agent: Arc<AgentEngine>,
    pub(crate) plugin_manager: Arc<PluginManager>,
}

impl TestConnection {
    pub(crate) async fn recv(&mut self) -> DaemonMessage {
        self.recv_with_timeout(Duration::from_millis(500)).await
    }

    pub(crate) async fn recv_with_timeout(&mut self, duration: Duration) -> DaemonMessage {
        timeout(duration, self.framed.next())
            .await
            .expect("timed out waiting for daemon message")
            .expect("connection closed")
            .expect("codec failure")
    }

    pub(crate) async fn shutdown(self) {
        let TestConnection {
            framed,
            task,
            root,
            agent: _,
            plugin_manager: _,
        } = self;
        drop(framed);
        let join = timeout(Duration::from_secs(2), task)
            .await
            .expect("server task did not shut down in time")
            .expect("server task join failed");
        if let Err(error) = join {
            if !is_expected_disconnect_error(&error) {
                panic!("server task returned error: {error}");
            }
        }
        let _ = std::fs::remove_dir_all(root);
    }
}

pub(crate) async fn spawn_test_connection_with_config(agent_config: AgentConfig) -> TestConnection {
    spawn_test_connection_with_config_and_startup_ready(agent_config, true).await
}

pub(crate) async fn spawn_test_connection_with_config_and_startup_ready(
    agent_config: AgentConfig,
    startup_ready: bool,
) -> TestConnection {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!(
            "server-whatsapp-link-test-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );
    let manager =
        SessionManager::new_with_history(history.clone(), agent_config.pty_channel_capacity);
    let agent =
        AgentEngine::new_with_shared_history(manager.clone(), agent_config, history.clone());
    let plugin_manager = Arc::new(PluginManager::new(history, root.join("plugins")));
    let startup_readiness = StartupReadiness::new(startup_ready);

    let (client_stream, server_stream) = tokio::io::duplex(128 * 1024);
    let server_task = tokio::spawn(handle_connection(
        server_stream,
        manager,
        agent.clone(),
        plugin_manager.clone(),
        startup_readiness,
    ));

    TestConnection {
        framed: Framed::new(client_stream, ZoraiCodec),
        task: server_task,
        root,
        agent,
        plugin_manager,
    }
}

pub(crate) async fn spawn_test_connection() -> TestConnection {
    spawn_test_connection_with_config(AgentConfig::default()).await
}

#[tokio::test]
pub(crate) async fn startup_readiness_blocks_history_requests_until_hydrate_finishes() {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!(
            "server-startup-readiness-test-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );
    history
        .create_thread(&zorai_protocol::AgentDbThread {
            id: "persisted-thread".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Persisted Thread".to_string(),
            created_at: 1_000,
            updated_at: 1_010,
            message_count: 1,
            total_tokens: 12,
            last_preview: "restored body".to_string(),
            metadata_json: None,
        })
        .await
        .expect("persist thread row");
    history
        .add_message(&zorai_protocol::AgentDbMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: "persisted-message".to_string(),
            thread_id: "persisted-thread".to_string(),
            created_at: 1_010,
            role: "assistant".to_string(),
            content: "restored body".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(4),
            output_tokens: Some(8),
            total_tokens: Some(12),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await
        .expect("persist thread message");

    let manager = SessionManager::new_with_history(
        history.clone(),
        AgentConfig::default().pty_channel_capacity,
    );
    let agent = AgentEngine::new_with_shared_history(
        manager.clone(),
        AgentConfig::default(),
        history.clone(),
    );
    let plugin_manager = Arc::new(PluginManager::new(history, root.join("plugins")));
    let startup_readiness = StartupReadiness::new(false);

    let (list_client_stream, list_server_stream) = tokio::io::duplex(128 * 1024);
    let list_task = tokio::spawn(handle_connection(
        list_server_stream,
        manager.clone(),
        agent.clone(),
        plugin_manager.clone(),
        startup_readiness.clone(),
    ));
    let mut list_conn = Framed::new(list_client_stream, ZoraiCodec);

    let (ping_client_stream, ping_server_stream) = tokio::io::duplex(128 * 1024);
    let ping_task = tokio::spawn(handle_connection(
        ping_server_stream,
        manager,
        agent.clone(),
        plugin_manager,
        startup_readiness.clone(),
    ));
    let mut ping_conn = Framed::new(ping_client_stream, ZoraiCodec);

    list_conn
        .send(ClientMessage::AgentListThreads {
            limit: None,
            offset: None,
            include_internal: false,
            agent_filter: None,
        })
        .await
        .expect("request thread list before startup is ready");

    let no_history_reply_before_ready = timeout(Duration::from_millis(100), list_conn.next())
        .await
        .is_err();
    assert!(
        no_history_reply_before_ready,
        "history-bearing requests should wait for startup readiness"
    );

    ping_conn
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while startup is blocked");
    assert!(
        matches!(
            timeout(Duration::from_millis(100), ping_conn.next())
                .await
                .expect("ping should not wait for startup readiness")
                .expect("ping connection closed")
                .expect("codec failure"),
            DaemonMessage::Pong
        ),
        "ping should remain available before hydrate completes"
    );

    agent.hydrate().await.expect("hydrate should succeed");
    startup_readiness.mark_ready();

    let DaemonMessage::AgentThreadList { threads_json } =
        timeout(Duration::from_secs(1), list_conn.next())
            .await
            .expect("thread list should resume after startup is ready")
            .expect("list connection closed")
            .expect("codec failure")
    else {
        panic!("expected thread list response after startup readiness");
    };
    assert!(
        threads_json.contains("persisted-thread"),
        "hydrated thread list should include persisted history"
    );

    drop(list_conn);
    drop(ping_conn);

    let list_join = timeout(Duration::from_secs(2), list_task)
        .await
        .expect("list server task did not shut down")
        .expect("list server join failed");
    if let Err(error) = list_join {
        if !is_expected_disconnect_error(&error) {
            panic!("list server task returned error: {error}");
        }
    }

    let ping_join = timeout(Duration::from_secs(2), ping_task)
        .await
        .expect("ping server task did not shut down")
        .expect("ping server join failed");
    if let Err(error) = ping_join {
        if !is_expected_disconnect_error(&error) {
            panic!("ping server task returned error: {error}");
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

pub(crate) fn test_user_sub_agent(id: &str, name: &str) -> SubAgentDefinition {
    SubAgentDefinition {
        generation_settings: None,
        base_url: None,
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("specialist".to_string()),
        system_prompt: Some("Handle delegated work.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_window_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        api_transport: None,
        claude_permission_mode: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        huggingface_provider: None,
        created_at: 1_712_000_010,
    }
}

pub(crate) async fn declare_async_command_capability(conn: &mut TestConnection) {
    conn.framed
        .send(ClientMessage::AgentDeclareAsyncCommandCapability {
            capability: zorai_protocol::AsyncCommandCapability {
                version: 1,
                supports_operation_acceptance: true,
            },
        })
        .await
        .expect("declare async command capability");

    match conn.recv().await {
        DaemonMessage::AgentAsyncCommandCapabilityAck { capability } => {
            assert_eq!(capability.version, 1);
            assert!(capability.supports_operation_acceptance);
        }
        other => panic!("expected async command capability ack, got {other:?}"),
    }
}

pub(crate) async fn register_test_oauth_plugin(conn: &TestConnection, name: &str) {
    let plugin_dir = conn.root.join("plugins").join(name);
    std::fs::create_dir_all(&plugin_dir).expect("create oauth plugin dir");
    let manifest = serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "schema_version": 1,
        "settings": {
            "client_id": {
                "type": "string",
                "label": "Client ID",
                "required": true
            }
        },
        "auth": {
            "type": "oauth2",
            "authorization_url": "https://example.com/oauth/authorize",
            "token_url": "http://127.0.0.1:9/token",
            "pkce": true
        }
    });
    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize oauth plugin manifest"),
    )
    .expect("write oauth plugin manifest");

    conn.plugin_manager
        .register_plugin(name, "test")
        .await
        .expect("register oauth plugin");
    conn.plugin_manager
        .update_setting(name, "client_id", "test-client", false)
        .await
        .expect("configure oauth plugin client id");
}

pub(crate) async fn register_test_api_plugin(conn: &TestConnection, name: &str) {
    let plugin_dir = conn.root.join("plugins").join(name);
    std::fs::create_dir_all(&plugin_dir).expect("create api plugin dir");
    let manifest = serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "schema_version": 1,
        "api": {
            "base_url": "https://example.com",
            "endpoints": {
                "slow": {
                    "method": "GET",
                    "path": "/slow"
                }
            }
        }
    });
    std::fs::write(
        plugin_dir.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize api plugin manifest"),
    )
    .expect("write api plugin manifest");

    conn.plugin_manager
        .register_plugin(name, "test")
        .await
        .expect("register api plugin");
}

#[tokio::test]
pub(crate) async fn tui_clients_cannot_execute_managed_terminal_commands() {
    let mut conn = spawn_test_connection().await;
    let (session_id, _rx) = conn
        .agent
        .session_manager
        .spawn(Some("/bin/bash".to_string()), None, None, None, 80, 24)
        .await
        .expect("spawn session");

    conn.framed
        .send(ClientMessage::ExecuteManagedCommand {
            id: session_id,
            request: zorai_protocol::ManagedCommandRequest {
                command: "pwd".to_string(),
                rationale: "test".to_string(),
                allow_network: false,
                sandbox_enabled: false,
                security_level: zorai_protocol::SecurityLevel::Lowest,
                cwd: None,
                language_hint: None,
                source: zorai_protocol::ManagedCommandSource::Human,
            },
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
        })
        .await
        .expect("send managed command");

    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("reserved for Electron"));
        }
        other => panic!("expected managed-command rejection, got {other:?}"),
    }

    conn.shutdown().await;
}

pub(crate) async fn register_publishable_skill_variant(
    conn: &TestConnection,
    skill_name: &str,
    status: &str,
) -> String {
    let skill_dir = conn
        .agent
        .history
        .data_dir()
        .join("skills")
        .join("community")
        .join(skill_name);
    std::fs::create_dir_all(&skill_dir).expect("create publishable skill dir");
    let skill_path = skill_dir.join("SKILL.md");
    std::fs::write(
        &skill_path,
        format!("---\nname: {skill_name}\ndescription: test skill\n---\nBody"),
    )
    .expect("write publishable skill");

    let record = conn
        .agent
        .history
        .register_skill_document(&skill_path)
        .await
        .expect("register publishable skill");
    conn.agent
        .history
        .update_skill_variant_status(&record.variant_id, status)
        .await
        .expect("set skill publish status");
    record.variant_id
}

pub(crate) async fn spawn_test_registry_publish_server(
    delay: Duration,
) -> (String, JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test registry listener");
    let addr = listener.local_addr().expect("test registry local addr");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept registry publish");
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        let header_end = loop {
            let n = stream.read(&mut buf).await.expect("read registry request");
            assert!(n > 0, "registry request closed before headers completed");
            request.extend_from_slice(&buf[..n]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let header_text = String::from_utf8_lossy(&request[..header_end]);
        let content_length = header_text
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        while request.len().saturating_sub(header_end) < content_length {
            let n = stream.read(&mut buf).await.expect("read registry body");
            assert!(n > 0, "registry request closed before body completed");
            request.extend_from_slice(&buf[..n]);
        }
        tokio::time::sleep(delay).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .expect("write registry response");
    });

    (format!("http://{}", addr), task)
}

pub(crate) fn build_test_skill_archive(skill_name: &str) -> Vec<u8> {
    use std::io::Cursor;

    let content = format!(
            "---\nname: {skill_name}\ndescription: imported test skill\nallowed_tools:\n  - read_file\n---\nImported body"
        );
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let bytes = content.into_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder
        .append_data(&mut header, "SKILL.md", Cursor::new(bytes))
        .expect("append skill file to archive");
    let encoder = builder.into_inner().expect("finish tar builder");
    encoder.finish().expect("finish skill archive")
}

pub(crate) async fn spawn_test_registry_fetch_server(
    skill_name: &str,
    delay: Duration,
) -> (String, JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let archive = build_test_skill_archive(skill_name);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test skill registry listener");
    let addr = listener
        .local_addr()
        .expect("test skill registry local addr");
    let expected_path = format!("/skills/{skill_name}.tar.gz");
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept registry fetch");
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream
                .read(&mut buf)
                .await
                .expect("read registry fetch request");
            assert!(
                n > 0,
                "registry fetch request closed before headers completed"
            );
            request.extend_from_slice(&buf[..n]);
            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let header_text = String::from_utf8_lossy(&request);
        let request_line = header_text
            .lines()
            .next()
            .expect("registry fetch request line");
        assert!(
            request_line.contains(&expected_path),
            "expected registry fetch path {expected_path}, got {request_line}"
        );

        tokio::time::sleep(delay).await;
        let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/gzip\r\nConnection: close\r\n\r\n",
                archive.len()
            );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("write registry fetch headers");
        stream
            .write_all(&archive)
            .await
            .expect("write registry fetch archive");
    });

    (format!("http://{}", addr), task)
}

pub(crate) async fn complete_test_oauth_callback(auth_url: &str) {
    let parsed = url::Url::parse(auth_url).expect("parse auth url");
    let redirect_uri = parsed
        .query_pairs()
        .find(|(key, _)| key == "redirect_uri")
        .map(|(_, value)| value.into_owned())
        .expect("redirect_uri query parameter");
    let state = parsed
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .expect("state query parameter");
    let redirect = url::Url::parse(&redirect_uri).expect("parse redirect uri");
    let host = redirect.host_str().expect("redirect host");
    let port = redirect.port_or_known_default().expect("redirect port");
    let path = redirect.path();

    let mut stream = tokio::net::TcpStream::connect((host, port))
        .await
        .expect("connect oauth callback listener");
    let request = format!(
            "GET {path}?code=test-auth-code&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        );
    use tokio::io::AsyncWriteExt;
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write oauth callback request");
}

pub(crate) async fn register_gateway(conn: &mut TestConnection) -> String {
    conn.framed
        .send(ClientMessage::GatewayRegister {
            registration: GatewayRegistration {
                gateway_id: "gateway-main".to_string(),
                instance_id: "instance-01".to_string(),
                protocol_version: GATEWAY_IPC_PROTOCOL_VERSION,
                supported_platforms: vec!["slack".to_string(), "discord".to_string()],
                process_id: Some(4242),
            },
        })
        .await
        .expect("send gateway register");
    match conn.recv().await {
        DaemonMessage::GatewayBootstrap { payload } => payload.bootstrap_correlation_id,
        other => panic!("expected GatewayBootstrap, got {other:?}"),
    }
}

pub(crate) async fn acknowledge_gateway_bootstrap(
    conn: &mut TestConnection,
    correlation_id: String,
) {
    conn.framed
        .send(ClientMessage::GatewayAck {
            ack: zorai_protocol::GatewayAck {
                correlation_id,
                accepted: true,
                detail: Some("bootstrap applied".to_string()),
            },
        })
        .await
        .expect("send gateway ack");
}
