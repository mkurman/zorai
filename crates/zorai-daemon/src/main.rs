#![recursion_limit = "512"]

// jemalloc on Linux: the glibc per-thread arena behavior retained >13 GB of
// fragmented heap under multi-thread load (30+ tokio workers plus 14 rusqlite
// connection threads). See crates/zorai-daemon/Cargo.toml for the full note.
// The `jemalloc` cargo feature is kept as a no-op alias for compatibility.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// jemalloc reads this before main. Two arenas and an immediate dirty purge
// are what keep an idle daemon near 500 MiB. A busy daemon still has about
// 2 GiB of headroom before the working set itself is the limit.
#[cfg(target_os = "linux")]
#[allow(non_upper_case_globals)]
#[used]
#[export_name = "_rjem_malloc_conf"]
static _rjem_malloc_conf: &[u8] =
    b"narenas:2,dirty_decay_ms:0,muzzy_decay_ms:0,background_thread:true\0";

pub mod agent;
mod criu;
mod db_cli;
mod git;
mod governance;
mod history;
mod lsp_client;
mod network;
mod notifications;
mod orchestration;
mod osc;
pub mod plugin;
mod policy;
mod policy_external;
mod pty_session;
mod sandbox;
mod scrub;
mod server;
mod session_manager;
mod snapshot;
mod state;
#[cfg(test)]
mod test_support;
mod validation;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn daemon_log_filter_from_values(zorai_log: Option<&str>) -> EnvFilter {
    let base_spec = zorai_log
        .and_then(valid_env_filter_spec)
        .unwrap_or_else(|| "info".to_string());
    EnvFilter::new(base_spec)
}

fn valid_env_filter_spec(value: &str) -> Option<String> {
    // Whitespace in a log-filter spec is always wrong: a well-formed spec is
    // comma-separated directives where each directive is `level` or
    // `target=level` with no spaces. `EnvFilter::try_new` is unfortunately
    // lenient here — it will happily accept "not a valid filter" by
    // treating it as a target pattern, which silently subverts the
    // fall-back-to-info contract this function is supposed to enforce.
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return None;
    }
    EnvFilter::try_new(trimmed).ok()?;
    Some(trimmed.to_string())
}

fn daemon_log_filter_from_env() -> EnvFilter {
    daemon_log_filter_from_values(std::env::var("ZORAI_LOG").ok().as_deref())
}

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = zorai_protocol::DailyLogWriter::new("zorai-daemon.log")?;
    let log_path = file_appender.current_path()?;
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(daemon_log_filter_from_env())
        .with_writer(writer)
        .with_ansi(false)
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = %panic_info, "zorai-daemon panicked");
    }));

    tracing::info!(path = %log_path.display(), "daemon log file initialized");
    Ok(guard)
}

const DAEMON_TOKIO_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

async fn daemon_main() -> Result<()> {
    {
        let cfg = zorai_protocol::ZoraiConfig::load();
        if std::env::var_os("ZORAI_DB_BACKEND").is_none() {
            if let Some(backend) = cfg.db_backend.as_deref() {
                let backend = backend.trim();
                if !backend.is_empty() {
                    std::env::set_var("ZORAI_DB_BACKEND", backend);
                }
            }
        }
        if std::env::var_os("ZORAI_DB_SYNC_URL").is_none() {
            if let Some(url) = cfg.db_sync_url.as_deref() {
                let url = url.trim();
                if !url.is_empty() {
                    std::env::set_var("ZORAI_DB_SYNC_URL", url);
                }
            }
        }
    }

    if std::env::args().nth(1).as_deref() == Some("db") {
        return db_cli::run_db_subcommand().await;
    }

    if std::env::args().nth(1).as_deref()
        == Some(agent::skill_preflight::SKILL_DISCOVERY_WORKER_ARG)
    {
        return agent::skill_preflight::run_skill_discovery_worker_from_stdio().await;
    }
    if std::env::args().nth(1).as_deref() == Some(agent::ALINE_STARTUP_WORKER_ARG) {
        return agent::run_aline_startup_worker_from_stdio().await;
    }
    if let Some(kind) = agent::background_workers::resolve_background_worker_kind_arg(
        std::env::args().nth(1).as_deref(),
    ) {
        return agent::background_workers::run_background_worker_from_stdio(kind).await;
    }

    let _log_guard = init_logging()?;

    tracing::info!("zorai-daemon starting");

    let state_path = state::default_state_path();
    tracing::info!(?state_path, "state file location");
    match state::load_state(&state_path) {
        Ok(state) => {
            tracing::info!(
                previous_sessions = state.previous_sessions.len(),
                "loaded persisted daemon state"
            );
        }
        Err(error) => {
            tracing::warn!(error = %error, path = %state_path.display(), "failed to load persisted daemon state");
        }
    }

    server::run().await?;

    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(DAEMON_TOKIO_WORKER_STACK_BYTES)
        .build()?
        .block_on(daemon_main())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_log_filter_uses_valid_zorai_log_value() {
        let rendered = daemon_log_filter_from_values(Some("trace")).to_string();

        assert!(rendered.contains("trace"), "{rendered}");
    }

    #[test]
    fn daemon_log_filter_falls_back_when_zorai_log_is_invalid() {
        let rendered = daemon_log_filter_from_values(Some("not a valid filter")).to_string();

        assert!(rendered.contains("info"), "{rendered}");
    }

    #[test]
    fn background_worker_protocol_round_trips() {
        let command = agent::background_workers::protocol::BackgroundWorkerCommand::Ping;
        let encoded =
            serde_json::to_string(&command).expect("background worker command should serialize");
        let decoded = serde_json::from_str::<
            agent::background_workers::protocol::BackgroundWorkerCommand,
        >(&encoded)
        .expect("background worker command should deserialize");

        assert!(matches!(
            decoded,
            agent::background_workers::protocol::BackgroundWorkerCommand::Ping
        ));
    }

    #[test]
    fn daemon_main_dispatches_background_worker_args() {
        let kind = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__zorai-background-worker-safety",
        ))
        .expect("expected safety worker arg to resolve");
        assert_eq!(
            kind,
            agent::background_workers::protocol::BackgroundWorkerKind::Safety
        );
        let routing = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__zorai-background-worker-routing",
        ))
        .expect("expected routing worker arg to resolve");
        assert_eq!(
            routing,
            agent::background_workers::protocol::BackgroundWorkerKind::Routing
        );
        let memory = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__zorai-background-worker-memory",
        ))
        .expect("expected memory worker arg to resolve");
        assert_eq!(
            memory,
            agent::background_workers::protocol::BackgroundWorkerKind::Memory
        );
        assert!(
            agent::background_workers::resolve_background_worker_kind_arg(Some(
                agent::skill_preflight::SKILL_DISCOVERY_WORKER_ARG,
            ))
            .is_none(),
            "background worker resolver must ignore other daemon helper args"
        );
    }
}
