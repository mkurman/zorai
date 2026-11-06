use super::*;
pub(crate) fn should_forward_agent_event(
    event: &crate::agent::types::AgentEvent,
    client_threads: &HashSet<String>,
) -> bool {
    if matches!(event, crate::agent::types::AgentEvent::BrowserRequest { .. }) {
        return true;
    }
    if let crate::agent::types::AgentEvent::TaskUpdate {
        task: Some(task), ..
    } = event
    {
        return task
            .thread_id
            .iter()
            .chain(task.parent_thread_id.iter())
            .any(|thread_id| client_threads.contains(thread_id));
    }

    match agent_event_thread_id(event) {
        Some(thread_id) => client_threads.contains(thread_id),
        None => matches!(
            event,
            crate::agent::types::AgentEvent::HeartbeatResult { .. }
                | crate::agent::types::AgentEvent::HeartbeatDigest { .. }
                | crate::agent::types::AgentEvent::Notification { .. }
                | crate::agent::types::AgentEvent::NotificationInboxUpsert { .. }
                | crate::agent::types::AgentEvent::AnticipatoryUpdate { .. }
                | crate::agent::types::AgentEvent::ConciergeWelcome { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitOpen { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitRecovered { .. }
                | crate::agent::types::AgentEvent::AuditAction { .. }
                | crate::agent::types::AgentEvent::EscalationUpdate { .. }
                | crate::agent::types::AgentEvent::GatewayStatus { .. }
                | crate::agent::types::AgentEvent::GatewayIncoming { .. }
                | crate::agent::types::AgentEvent::BudgetAlert { .. }
                | crate::agent::types::AgentEvent::TrajectoryUpdate { .. }
                | crate::agent::types::AgentEvent::EpisodeRecorded { .. }
                | crate::agent::types::AgentEvent::OperatorQuestion { .. }
                | crate::agent::types::AgentEvent::OperatorQuestionResolved { .. }
        ),
    }
}

pub(crate) fn lag_recovery_thread_reload_events(
    client_threads: &HashSet<String>,
) -> Vec<crate::agent::types::AgentEvent> {
    client_threads
        .iter()
        .map(
            |thread_id| crate::agent::types::AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.clone(),
            },
        )
        .collect()
}

pub(crate) fn concierge_welcome_fingerprint(
    event: &crate::agent::types::AgentEvent,
) -> Option<String> {
    match event {
        crate::agent::types::AgentEvent::ConciergeWelcome {
            thread_id,
            content,
            detail_level,
            actions,
        } => serde_json::to_string(&(thread_id, content, detail_level, actions)).ok(),
        _ => None,
    }
}

/// Socket path / pipe name for IPC.
#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("zorai-daemon.sock")
}

/// Run the IPC server until a shutdown signal is received.
pub async fn run() -> Result<()> {
    let history = crate::history::HistoryStore::new()
        .await
        .context("failed to initialize shared history store")?;
    let history = Arc::new(history);

    let agent_config = crate::agent::load_config_from_history(&history)
        .await
        .unwrap_or_default();

    let manager =
        SessionManager::new_with_history(history.clone(), agent_config.pty_channel_capacity);
    manager
        .hydrate_pending_approvals()
        .await
        .context("failed to hydrate approval inbox")?;
    let reaper_manager = manager.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            reaper_manager.reap_dead().await;
        }
    });

    let agent =
        AgentEngine::new_with_shared_history(manager.clone(), agent_config, history.clone());

    let plugins_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".zorai")
        .join("plugins");
    let plugin_manager = Arc::new(crate::plugin::PluginManager::new(
        history.clone(),
        plugins_dir,
    ));
    let (pm_loaded, pm_skipped) = plugin_manager.load_all_from_disk().await;
    tracing::info!(
        loaded = pm_loaded,
        skipped = pm_skipped,
        "plugin loader complete"
    );

    let _ = agent.plugin_manager.set(plugin_manager.clone());

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let startup_readiness = StartupReadiness::new(false);

    #[cfg(unix)]
    let result = {
        let path = socket_path();
        let listener = bind_unix_listener(&path)?;
        tracing::info!(?path, "daemon listening on Unix socket");
        let startup_agent = agent.clone();
        let startup_readiness_for_task = startup_readiness.clone();
        tokio::spawn(async move {
            if let Err(e) = startup_agent
                .hydrate_without_participant_observer_restore()
                .await
            {
                tracing::warn!("failed to hydrate agent engine: {e}");
            }

            startup_agent
                .concierge
                .initialize(&startup_agent.threads)
                .await;

            startup_readiness_for_task.mark_ready();
            startup_agent.schedule_participant_observer_restore_after_hydrate();

            #[cfg(not(test))]
            maybe_autostart_whatsapp_link(startup_agent.clone()).await;

            Box::pin(startup_agent.run_loop(shutdown_rx)).await;
        });
        run_unix(
            listener,
            path,
            manager,
            agent.clone(),
            plugin_manager.clone(),
            startup_readiness.clone(),
        )
        .await
    };

    #[cfg(windows)]
    let result = {
        use tokio::net::TcpListener;

        let addr = zorai_protocol::default_tcp_addr();
        let listener = TcpListener::bind(&addr).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                anyhow::anyhow!(
                    "daemon is already running on {addr}; stop the existing process before starting another instance"
                )
            } else {
                anyhow::Error::new(error)
                    .context(format!("failed to bind daemon TCP listener on {addr}"))
            }
        })?;
        tracing::info!(%addr, "daemon listening on TCP");
        let startup_agent = agent.clone();
        let startup_readiness_for_task = startup_readiness.clone();
        tokio::spawn(async move {
            if let Err(e) = startup_agent
                .hydrate_without_participant_observer_restore()
                .await
            {
                tracing::warn!("failed to hydrate agent engine: {e}");
            }

            startup_agent
                .concierge
                .initialize(&startup_agent.threads)
                .await;

            startup_readiness_for_task.mark_ready();
            startup_agent.schedule_participant_observer_restore_after_hydrate();

            #[cfg(not(test))]
            maybe_autostart_whatsapp_link(startup_agent.clone()).await;

            Box::pin(startup_agent.run_loop(shutdown_rx)).await;
        });
        run_windows(
            listener,
            addr,
            manager,
            agent.clone(),
            plugin_manager.clone(),
            startup_readiness.clone(),
        )
        .await
    };

    let _ = shutdown_tx.send(true);
    agent.mlflow_tracing.shutdown().await;

    result
}

#[cfg(unix)]
async fn run_unix(
    listener: tokio::net::UnixListener,
    path: std::path::PathBuf,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
    startup_readiness: StartupReadiness,
) -> Result<()> {
    let shutdown = await_shutdown_signal_unix();

    let owner_uid = daemon_socket_owner_uid(&path);

    tokio::select! {
        _ = accept_loop_unix(listener, owner_uid, manager, agent, plugin_manager, startup_readiness) => {}
        _ = shutdown => {}
    }

    let _ = std::fs::remove_file(&path);
    tracing::info!("daemon shut down");
    Ok(())
}

#[cfg(unix)]
async fn await_shutdown_signal_unix() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = match signal(SignalKind::terminate()) {
        Ok(stream) => Some(stream),
        Err(error) => {
            tracing::warn!(error = %error, "SIGTERM listener unavailable");
            None
        }
    };
    let mut sighup = match signal(SignalKind::hangup()) {
        Ok(stream) => Some(stream),
        Err(error) => {
            tracing::warn!(error = %error, "SIGHUP listener unavailable");
            None
        }
    };

    loop {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                match result {
                    Ok(()) => tracing::info!("shutdown signal received signal=SIGINT"),
                    Err(error) => tracing::warn!(error = %error, "shutdown signal listener unavailable"),
                }
                return;
            }
            _ = async {
                match sigterm.as_mut() {
                    Some(stream) => {
                        let _ = stream.recv().await;
                    }
                    None => std::future::pending::<()>().await,
                }
            } => {
                tracing::info!("shutdown signal received signal=SIGTERM");
                return;
            }
            _ = async {
                match sighup.as_mut() {
                    Some(stream) => {
                        let _ = stream.recv().await;
                    }
                    None => std::future::pending::<()>().await,
                }
            } => {
                tracing::warn!("ignoring SIGHUP to keep daemon alive");
            }
        }
    }
}

#[cfg(unix)]
fn bind_unix_listener(path: &std::path::Path) -> Result<tokio::net::UnixListener> {
    use std::io::ErrorKind;
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::PermissionsExt;

    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_socket() {
            anyhow::bail!(
                "refusing to remove non-socket daemon path {}; delete it manually",
                path.display()
            );
        }

        match std::os::unix::net::UnixStream::connect(path) {
            Ok(stream) => {
                drop(stream);
                anyhow::bail!(
                    "daemon is already running on {}; stop the existing process before starting another instance",
                    path.display()
                );
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(path).with_context(|| {
                    format!("failed to remove stale daemon socket {}", path.display())
                })?;
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "failed to probe existing daemon socket {}",
                    path.display()
                )));
            }
        }
    }

    let listener = tokio::net::UnixListener::bind(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "daemon is already running on {}; stop the existing process before starting another instance",
                path.display()
            )
        } else {
            anyhow::Error::new(error)
                .context(format!("failed to bind daemon Unix socket {}", path.display()))
        }
    })?;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).with_context(|| {
        format!(
            "failed to restrict permissions on daemon socket {}",
            path.display()
        )
    })?;

    Ok(listener)
}

/// The uid that owns the daemon socket file is the uid that bound it, i.e. the
/// daemon's own effective uid. We compare incoming peer credentials against it
/// so a different local user cannot drive the control channel even if the socket
/// mode is ever loosened. Returns `None` if the owner can't be determined, in
/// which case peer filtering is skipped (the 0600 mode is still in force).
#[cfg(unix)]
fn daemon_socket_owner_uid(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|metadata| metadata.uid())
}

#[cfg(unix)]
async fn accept_loop_unix(
    listener: tokio::net::UnixListener,
    owner_uid: Option<u32>,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
    startup_readiness: StartupReadiness,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                if let Some(expected_uid) = owner_uid {
                    match stream.peer_cred() {
                        Ok(cred) if cred.uid() != expected_uid => {
                            tracing::warn!(
                                peer_uid = cred.uid(),
                                expected_uid,
                                "rejecting daemon connection from foreign uid"
                            );
                            continue;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(
                                error = %error,
                                "rejecting daemon connection: peer credentials unavailable"
                            );
                            continue;
                        }
                    }
                }
                let manager = manager.clone();
                let agent = agent.clone();
                let plugin_manager = plugin_manager.clone();
                let startup_readiness = startup_readiness.clone();
                tokio::spawn(async move {
                    if let Err(e) = Box::pin(handle_connection(
                        stream,
                        manager,
                        agent,
                        plugin_manager,
                        startup_readiness,
                    ))
                    .await
                    {
                        if is_expected_disconnect_error(&e) {
                            tracing::debug!(error = %e, "client disconnected");
                        } else {
                            tracing::error!(error = %e, "client connection error");
                        }
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "accept error");
            }
        }
    }
}

#[cfg(all(test, unix))]
mod unix_socket_tests {
    use super::bind_unix_listener;
    use std::os::unix::net::UnixListener;

    fn with_io_runtime<T>(f: impl FnOnce() -> T) -> T {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("build tokio runtime");
        runtime.block_on(async move { f() })
    }

    #[test]
    fn bind_unix_listener_rejects_second_live_daemon() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("zorai-daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind first daemon socket");

        let error = with_io_runtime(|| {
            bind_unix_listener(&path).expect_err("second daemon should be rejected")
        });
        let message = error.to_string();
        assert!(
            message.contains("already running"),
            "expected already-running error, got: {message}"
        );
        assert!(
            path.exists(),
            "live daemon socket path should remain intact"
        );

        drop(listener);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bind_unix_listener_replaces_stale_socket_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("zorai-daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind initial socket");
        drop(listener);

        let rebound =
            with_io_runtime(|| bind_unix_listener(&path).expect("stale socket should be replaced"));
        assert!(path.exists(), "rebound daemon socket should exist");

        drop(rebound);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bind_unix_listener_restricts_socket_to_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("zorai-daemon.sock");

        let listener = with_io_runtime(|| bind_unix_listener(&path).expect("bind daemon socket"));
        let mode = std::fs::metadata(&path)
            .expect("socket metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "daemon control socket must be owner-only; the channel has no authentication"
        );

        drop(listener);
        let _ = std::fs::remove_file(&path);
    }
}

async fn await_shutdown_signal<F>(signal: F, keep_running_on_error: bool)
where
    F: std::future::Future<Output = std::io::Result<()>>,
{
    match signal.await {
        Ok(()) => tracing::info!("shutdown signal received"),
        Err(error) if keep_running_on_error => {
            tracing::warn!(
                error = %error,
                "shutdown signal listener unavailable; keeping daemon alive in headless mode"
            );
            std::future::pending::<()>().await;
        }
        Err(error) => {
            tracing::warn!(error = %error, "shutdown signal listener unavailable");
        }
    }
}

#[cfg(test)]
mod shutdown_signal_tests {
    use super::await_shutdown_signal;
    use std::io;
    use std::time::Duration;

    #[tokio::test]
    async fn shutdown_signal_completes_when_signal_arrives() {
        tokio::time::timeout(
            Duration::from_millis(50),
            await_shutdown_signal(async { Ok::<(), io::Error>(()) }, false),
        )
        .await
        .expect("shutdown future should complete when the signal arrives");
    }

    #[tokio::test]
    async fn shutdown_signal_stays_pending_when_headless_mode_has_no_signal_source() {
        let timeout = tokio::time::timeout(
            Duration::from_millis(50),
            await_shutdown_signal(
                async { Err::<(), io::Error>(io::Error::other("no console")) },
                true,
            ),
        )
        .await;

        assert!(
            timeout.is_err(),
            "headless shutdown handling should stay pending instead of exiting immediately"
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_unix_shutdown_path_handles_sigterm_and_sighup() {
        let root = crate::test_support::repo_root();
        let source =
            std::fs::read_to_string(root.join("crates/zorai-daemon/src/server/post_tests.rs"))
                .expect("read server startup source");

        assert!(
            source.contains("SignalKind::terminate()"),
            "unix daemon shutdown should listen for SIGTERM"
        );
        assert!(
            source.contains("SignalKind::hangup()"),
            "unix daemon shutdown should install a SIGHUP handler"
        );
        assert!(
            source.contains("ignoring SIGHUP to keep daemon alive"),
            "unix daemon should log ignored SIGHUP events"
        );
    }
}

#[cfg(windows)]
async fn run_windows(
    listener: tokio::net::TcpListener,
    addr: String,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
    startup_readiness: StartupReadiness,
) -> Result<()> {
    let shutdown = await_shutdown_signal(tokio::signal::ctrl_c(), true);

    tokio::select! {
        _ = accept_loop_tcp(listener, manager, agent, plugin_manager, startup_readiness) => {}
        _ = shutdown => {}
    }

    tracing::info!("daemon shut down");
    Ok(())
}

#[allow(dead_code)]
async fn accept_loop_tcp(
    listener: tokio::net::TcpListener,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
    startup_readiness: StartupReadiness,
) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!(%addr, "client connected");
                let manager = manager.clone();
                let agent = agent.clone();
                let plugin_manager = plugin_manager.clone();
                let startup_readiness = startup_readiness.clone();
                tokio::spawn(async move {
                    if let Err(e) = Box::pin(handle_connection(
                        stream,
                        manager,
                        agent,
                        plugin_manager,
                        startup_readiness,
                    ))
                    .await
                    {
                        if is_expected_disconnect_error(&e) {
                            tracing::debug!(%addr, error = %e, "client disconnected");
                        } else {
                            tracing::error!(%addr, error = %e, "client connection error");
                        }
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "accept error");
            }
        }
    }
}
