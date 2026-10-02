#![recursion_limit = "256"]

mod app;
mod auth;
mod client;
mod projection;
mod providers;
mod state;
mod system_monitor;
mod terminal_graphics;
#[cfg(test)]
mod test_support;
mod theme;
mod update;
mod widgets;
mod wire;

use std::collections::{HashSet, VecDeque};
use std::io;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use tokio::sync::mpsc as tokio_mpsc;
use tracing_subscriber::EnvFilter;

use crate::app::TuiModel;
use crate::client::DaemonClient;
use crate::state::DaemonCommand;

const MAX_TRANSIENT_TERMINAL_ERRORS: usize = 5;
const TERMINAL_ERROR_RETRY_DELAY_MS: u64 = 150;
const MAX_DAEMON_EVENTS_PER_FRAME: usize = 256;
const IDLE_TICK_RATE_MULTIPLIER: u32 = 5;
const MIN_REDRAW_INTERVAL_MS: u128 = 33;

fn build_log_filter(tui_log: Option<&str>, zorai_log: Option<&str>) -> EnvFilter {
    tui_log
        .and_then(parse_log_filter)
        .or_else(|| zorai_log.and_then(parse_log_filter))
        .unwrap_or_else(|| EnvFilter::new("warn,zorai_tui=info"))
}

fn parse_log_filter(value: &str) -> Option<EnvFilter> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    EnvFilter::try_new(trimmed).ok()
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn should_retry_terminal_error(consecutive_errors: usize) -> bool {
    consecutive_errors < MAX_TRANSIENT_TERMINAL_ERRORS
}

fn idle_loop_tick_rate(base_tick: Duration) -> Duration {
    base_tick.saturating_mul(IDLE_TICK_RATE_MULTIPLIER)
}

fn loop_tick_rate(model: &TuiModel, base_tick: Duration) -> Duration {
    if model.wants_fast_tick() {
        base_tick
    } else {
        idle_loop_tick_rate(base_tick)
    }
}

fn elapsed_model_ticks(elapsed: Duration, base_tick: Duration) -> u64 {
    let base_nanos = base_tick.as_nanos().max(1);
    (elapsed.as_nanos() / base_nanos).max(1) as u64
}

fn tui_runtime_marker_path() -> io::Result<PathBuf> {
    Ok(zorai_protocol::ensure_zorai_data_dir()?.join("zorai-tui.last-state"))
}

fn write_runtime_marker_at(path: &Path, message: &str) -> io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    std::fs::write(
        path,
        format!(
            "timestamp_unix={timestamp}\npid={}\nmessage={message}\n",
            std::process::id()
        ),
    )
}

fn write_runtime_marker(message: &str) {
    if let Ok(path) = tui_runtime_marker_path() {
        let _ = write_runtime_marker_at(&path, message);
    }
}

fn zorai_cli_binary() -> PathBuf {
    if let Ok(path) = std::env::var("ZORAI_CLI_PATH") {
        return PathBuf::from(path);
    }
    let binary_name = if cfg!(windows) { "zorai.exe" } else { "zorai" };
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            let sibling = parent.join(binary_name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(binary_name)
}

fn plugin_install_output_message(
    success: bool,
    source: &str,
    stdout: &[u8],
    stderr: &[u8],
) -> String {
    let output = if success { stdout } else { stderr };
    let text = String::from_utf8_lossy(output).trim().to_string();
    if text.is_empty() {
        if success {
            format!("Plugin installed from {source}")
        } else {
            format!("Plugin installation failed for {source}")
        }
    } else {
        text.lines().take(6).collect::<Vec<_>>().join("\n")
    }
}

fn install_plugin_source_from_tui(
    daemon_event_tx: mpsc::Sender<client::ClientEvent>,
    source: String,
) {
    thread::spawn(move || {
        let output = std::process::Command::new(zorai_cli_binary())
            .args(["plugin", "add", source.as_str()])
            .output();
        let event = match output {
            Ok(output) => client::ClientEvent::PluginAction {
                success: output.status.success(),
                message: plugin_install_output_message(
                    output.status.success(),
                    &source,
                    &output.stdout,
                    &output.stderr,
                ),
            },
            Err(err) => client::ClientEvent::PluginAction {
                success: false,
                message: format!("Plugin installation failed: {err}"),
            },
        };
        let _ = daemon_event_tx.send(event);
    });
}

fn best_effort_restore_stdio_terminal() {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    let _ = stdout.execute(DisableBracketedPaste);
    let _ = stdout.execute(DisableMouseCapture);
    let _ = stdout.execute(PopKeyboardEnhancementFlags);
    let _ = stdout.execute(LeaveAlternateScreen);
    // The Terminal-based restore path shows the cursor via `show_cursor`, but
    // this raw-stdout path (panic hook / signal handler) must do it explicitly,
    // or a panic mid-render leaves the user with an invisible cursor.
    let _ = stdout.execute(Show);
    let _ = stdout.flush();
}

/// The signals that should restore the terminal before the process dies.
/// Ctrl+C/Ctrl+\ don't reach us as signals while raw mode is on (ISIG is off —
/// they arrive as key bytes), but `kill`, a closed terminal (SIGHUP), and a
/// dying parent still terminate us; without this the user is left in the
/// alternate screen with raw mode on.
#[cfg(unix)]
const TERMINAL_RESTORE_SIGNALS: [libc::c_int; 4] =
    [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

#[cfg(unix)]
fn build_restore_signal_set() -> libc::sigset_t {
    use std::mem::MaybeUninit;
    unsafe {
        let mut set = MaybeUninit::<libc::sigset_t>::uninit();
        libc::sigemptyset(set.as_mut_ptr());
        let mut set = set.assume_init();
        for signal in TERMINAL_RESTORE_SIGNALS {
            libc::sigaddset(&mut set, signal);
        }
        set
    }
}

/// Block the restore signals process-wide and hand them to a dedicated thread
/// that waits with `sigwait`. Restoration then runs in normal thread context
/// (not an async-signal handler), so it can safely use crossterm. Must be called
/// before any other thread is spawned so the block is inherited.
#[cfg(unix)]
fn install_terminal_signal_restore() {
    let mut set = build_restore_signal_set();
    unsafe {
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }

    thread::spawn(move || {
        let mut signo: libc::c_int = 0;
        let rc = unsafe { libc::sigwait(&mut set, &mut signo) };
        best_effort_restore_stdio_terminal();
        write_runtime_marker(&format!("terminated by signal {signo}"));
        // 128 + signo is the conventional shell exit status for signal death.
        let code = if rc == 0 { 128 + signo } else { 1 };
        std::process::exit(code);
    });
}

#[cfg(not(unix))]
fn install_terminal_signal_restore() {}

fn install_terminal_panic_hook() {
    panic::set_hook(Box::new(|info| {
        best_effort_restore_stdio_terminal();
        let mut panic_message = format!("panic: {}", panic_payload_message(info.payload()));
        if let Some(location) = info.location() {
            panic_message.push_str(&format!(
                " @ {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            ));
        }
        write_runtime_marker(&panic_message);
        eprintln!(
            "zorai-tui panicked: {}",
            panic_payload_message(info.payload())
        );
        if let Some(location) = info.location() {
            eprintln!(
                "at {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        }
    }));
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    supports_keyboard_enhancement: bool,
) -> io::Result<()> {
    let mut first_error = None;
    let mut capture_error = |result: io::Result<()>| {
        if let Err(err) = result {
            if first_error.is_none() {
                first_error = Some(err);
            }
        }
    };

    capture_error(disable_raw_mode());
    capture_error(
        terminal
            .backend_mut()
            .execute(DisableBracketedPaste)
            .map(|_| ()),
    );
    capture_error(
        terminal
            .backend_mut()
            .execute(DisableMouseCapture)
            .map(|_| ()),
    );
    if supports_keyboard_enhancement {
        capture_error(
            terminal
                .backend_mut()
                .execute(PopKeyboardEnhancementFlags)
                .map(|_| ()),
        );
    }
    capture_error(
        terminal
            .backend_mut()
            .execute(LeaveAlternateScreen)
            .map(|_| ()),
    );
    capture_error(terminal.show_cursor());

    if let Some(err) = first_error {
        Err(err)
    } else {
        Ok(())
    }
}

fn forward_bridge_command_result(
    daemon_event_tx: &mpsc::Sender<client::ClientEvent>,
    action: &str,
    result: Result<()>,
) {
    if let Err(err) = result {
        emit_bridge_command_failure(daemon_event_tx, action, err);
    }
}

fn emit_bridge_command_failure(
    daemon_event_tx: &mpsc::Sender<client::ClientEvent>,
    action: &str,
    err: anyhow::Error,
) {
    tracing::error!(action, error = %err, "daemon bridge command failed");
    let _ = daemon_event_tx.send(client::ClientEvent::Error(format!(
        "daemon bridge command failed ({action}): {err}"
    )));
    let _ = daemon_event_tx.send(client::ClientEvent::Disconnected);
}

async fn dispatch_background_goal_hydration(
    client: &DaemonClient,
    daemon_event_tx: &mpsc::Sender<client::ClientEvent>,
    goal_run_id: String,
) {
    let detail_result = client.request_goal_run(goal_run_id.clone());
    let checkpoints_result = client.list_checkpoints(goal_run_id.clone());
    if detail_result.is_err() || checkpoints_result.is_err() {
        let err = detail_result
            .err()
            .or_else(|| checkpoints_result.err())
            .expect("one background hydration send must have failed");
        emit_bridge_command_failure(daemon_event_tx, "request background goal hydration", err);
        let _ =
            daemon_event_tx.send(client::ClientEvent::GoalHydrationScheduleFailed { goal_run_id });
    }
}

fn main() -> Result<()> {
    let log_writer = zorai_protocol::DailyLogWriter::new("zorai-tui.log")?;
    let log_path = log_writer.current_path()?;
    let (log_writer, _log_guard) = tracing_appender::non_blocking(log_writer);
    let tui_log = std::env::var("ZORAI_TUI_LOG").ok();
    let zorai_log = std::env::var("ZORAI_LOG").ok();
    tracing_subscriber::fmt()
        .with_env_filter(build_log_filter(tui_log.as_deref(), zorai_log.as_deref()))
        .with_writer(log_writer)
        .with_ansi(false)
        .init();
    tracing::info!(path = %log_path.display(), "tui log file initialized");
    write_runtime_marker("startup: logging initialized");
    install_terminal_panic_hook();
    install_terminal_signal_restore();

    enable_raw_mode()?;
    // Past this point raw mode is on, so any early failure must restore the
    // terminal — `restore_terminal` below only runs if we reach the run loop.
    let setup_result = (|| -> io::Result<(Terminal<CrosstermBackend<io::Stdout>>, bool)> {
        let mut stdout = io::stdout();
        stdout.execute(EnterAlternateScreen)?;
        stdout.execute(EnableMouseCapture)?;
        stdout.execute(EnableBracketedPaste)?;
        let supports_keyboard_enhancement = matches!(
            crossterm::terminal::supports_keyboard_enhancement(),
            Ok(true)
        );
        if supports_keyboard_enhancement {
            let _ = stdout.execute(PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
            ));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok((terminal, supports_keyboard_enhancement))
    })();
    let (mut terminal, supports_keyboard_enhancement) = match setup_result {
        Ok(value) => value,
        Err(error) => {
            best_effort_restore_stdio_terminal();
            return Err(error.into());
        }
    };
    if let Err(error) = terminal.clear() {
        let _ = restore_terminal(&mut terminal, supports_keyboard_enhancement);
        return Err(error.into());
    }
    write_runtime_marker("startup: terminal initialized");

    let app_result = panic::catch_unwind(AssertUnwindSafe(|| {
        let (daemon_event_tx, daemon_event_rx) = mpsc::channel();
        let (daemon_cmd_tx, daemon_cmd_rx) = tokio_mpsc::unbounded_channel();
        start_daemon_bridge(daemon_event_tx, daemon_cmd_rx);
        update::spawn_update_check(daemon_cmd_tx.clone());

        let mut model = TuiModel::new(daemon_event_rx, daemon_cmd_tx);
        model.load_saved_settings();
        let protocol = crate::terminal_graphics::configure_detected_protocol();
        let mut graphics_renderer =
            crate::terminal_graphics::TerminalGraphicsRenderer::new(protocol);

        let tick_rate = Duration::from_millis(crate::app::TUI_TICK_RATE_MS);
        run_loop(&mut terminal, &mut model, &mut graphics_renderer, tick_rate)
    }));

    let restore_result = restore_terminal(&mut terminal, supports_keyboard_enhancement);

    let app_result = match app_result {
        Ok(result) => result,
        Err(payload) => Err(anyhow::anyhow!(
            "zorai-tui panicked: {}",
            panic_payload_message(payload.as_ref())
        )),
    };

    if let Err(err) = restore_result {
        write_runtime_marker(&format!("exit: terminal restore failed: {err}"));
        tracing::error!(error = %err, "failed to restore terminal state");
        if app_result.is_ok() {
            return Err(err.into());
        }
    }

    match &app_result {
        Ok(_) => write_runtime_marker("exit: clean"),
        Err(err) => write_runtime_marker(&format!("exit: {err}")),
    }

    app_result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model: &mut TuiModel,
    graphics_renderer: &mut crate::terminal_graphics::TerminalGraphicsRenderer,
    tick_rate: Duration,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let mut next_tick = last_tick + loop_tick_rate(model, tick_rate);
    let mut terminal_error_streak = 0usize;
    let mut needs_draw = true;
    let mut last_drawn_at = Instant::now() - Duration::from_secs(1);
    let mut input_pending = false;

    loop {
        let now = Instant::now();
        if now >= next_tick {
            let elapsed_ticks =
                elapsed_model_ticks(now.saturating_duration_since(last_tick), tick_rate);
            if model.on_tick_elapsed(elapsed_ticks) {
                needs_draw = true;
            }
            last_tick = now;
            next_tick = now + loop_tick_rate(model, tick_rate);
        }

        let pump_outcome = model.pump_daemon_events_budgeted(MAX_DAEMON_EVENTS_PER_FRAME);
        let processed_daemon_events = pump_outcome.processed;
        if processed_daemon_events > 0 {
            needs_draw = true;
        }

        // Throttle redraws: when daemon events flood in (active goal run
        // streaming deltas / tool calls), we used to redraw on every
        // 32-event batch, which under heavy view-render cost backed up
        // the event drain by seconds. Cap render rate to ~30 fps unless
        // the user just pressed a key (input_pending always renders so
        // typing stays snappy). Tick-driven redraws still run at their
        // scheduled cadence.
        let since_last_draw = now.saturating_duration_since(last_drawn_at);
        let throttled = !input_pending
            && processed_daemon_events > 0
            && since_last_draw.as_millis() < MIN_REDRAW_INTERVAL_MS;
        if needs_draw && !throttled {
            let render_started = Instant::now();
            let pumped_for_log = processed_daemon_events;
            match terminal.draw(|frame| {
                model.render(frame);
            }) {
                Ok(_) => {
                    let elapsed_ms = render_started.elapsed().as_millis() as u64;
                    if elapsed_ms > 100 {
                        tracing::warn!(
                            elapsed_ms,
                            pumped = pumped_for_log,
                            input_pending,
                            "tui: slow render frame (>100ms)"
                        );
                    }
                    terminal_error_streak = 0;
                }
                Err(err) => {
                    if should_retry_terminal_error(terminal_error_streak) {
                        terminal_error_streak += 1;
                        tracing::warn!(
                            error = %err,
                            consecutive_errors = terminal_error_streak,
                            "transient terminal draw error"
                        );
                        thread::sleep(Duration::from_millis(TERMINAL_ERROR_RETRY_DELAY_MS));
                        continue;
                    }
                    return Err(err.into());
                }
            }

            if let Err(err) =
                graphics_renderer.render(terminal, model.terminal_image_overlay_spec())
            {
                tracing::warn!(error = %err, "terminal image overlay render failed");
            }
            needs_draw = false;
            last_drawn_at = now;
            input_pending = false;
        }

        let now = Instant::now();
        let preferred_tick_rate = loop_tick_rate(model, tick_rate);
        if next_tick.saturating_duration_since(now) > preferred_tick_rate {
            next_tick = now + preferred_tick_rate;
        }
        let until_tick = next_tick.saturating_duration_since(now);
        let until_thread_picker_refresh = model
            .pending_thread_picker_refresh_deadline()
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(until_tick);
        let wait_for = if processed_daemon_events == MAX_DAEMON_EVENTS_PER_FRAME
            || pump_outcome.stopped_for_render
        {
            Duration::ZERO
        } else {
            until_tick.min(until_thread_picker_refresh)
        };

        let polled = match event::poll(wait_for) {
            Ok(polled) => {
                terminal_error_streak = 0;
                polled
            }
            Err(err) => {
                if should_retry_terminal_error(terminal_error_streak) {
                    terminal_error_streak += 1;
                    tracing::warn!(
                        error = %err,
                        consecutive_errors = terminal_error_streak,
                        "transient terminal poll error"
                    );
                    thread::sleep(Duration::from_millis(TERMINAL_ERROR_RETRY_DELAY_MS));
                    continue;
                }
                return Err(err.into());
            }
        };

        if polled {
            let event = match event::read() {
                Ok(event) => {
                    terminal_error_streak = 0;
                    event
                }
                Err(err) => {
                    if should_retry_terminal_error(terminal_error_streak) {
                        terminal_error_streak += 1;
                        tracing::warn!(
                            error = %err,
                            consecutive_errors = terminal_error_streak,
                            "transient terminal read error"
                        );
                        thread::sleep(Duration::from_millis(TERMINAL_ERROR_RETRY_DELAY_MS));
                        continue;
                    }
                    return Err(err.into());
                }
            };

            match event {
                Event::Key(key)
                    if matches!(
                        key.kind,
                        crossterm::event::KeyEventKind::Press
                            | crossterm::event::KeyEventKind::Repeat
                    ) =>
                {
                    if model.handle_key(key.code, key.modifiers) {
                        return Ok(());
                    }
                    needs_draw = true;
                    input_pending = true;
                }
                Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Release => {
                    model.handle_key_release(key.code, key.modifiers);
                    needs_draw = true;
                    input_pending = true;
                }
                Event::Paste(text) => {
                    model.handle_paste(text);
                    needs_draw = true;
                    input_pending = true;
                }
                Event::Resize(w, h) => {
                    model.handle_resize(w, h);
                    needs_draw = true;
                    input_pending = true;
                }
                Event::Mouse(mouse) => {
                    model.handle_mouse(mouse);
                    needs_draw = true;
                    input_pending = true;
                }
                _ => {}
            }
        }

        // Terminal input wins at the debounce boundary. A source-switch event
        // processed above replaces the pending intent with a fresh deadline;
        // unrelated input does not starve an already-due refresh.
        if model.dispatch_due_thread_picker_refresh(Instant::now()) {
            needs_draw = true;
        }
    }
}

fn start_daemon_bridge(
    daemon_event_tx: mpsc::Sender<client::ClientEvent>,
    mut daemon_cmd_rx: tokio_mpsc::UnboundedReceiver<DaemonCommand>,
) {
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build();

        let Ok(runtime) = runtime else {
            let _ = daemon_event_tx.send(client::ClientEvent::Error(
                "failed to create tokio runtime".to_string(),
            ));
            return;
        };

        runtime.block_on(async move {
            let (client_event_tx, mut client_event_rx) = tokio_mpsc::channel(65_536);
            let client = DaemonClient::new(client_event_tx);
            let mut queued_goal_hydrations: VecDeque<String> = VecDeque::new();
            let mut queued_goal_hydration_ids: HashSet<String> = HashSet::new();

            if let Err(err) = client.connect().await {
                let _ = daemon_event_tx.send(client::ClientEvent::Error(format!(
                    "daemon connection init failed: {}",
                    err
                )));
            }

            loop {
                tokio::select! {
                    incoming = client_event_rx.recv() => {
                        match incoming {
                            Some(event) => {
                                let _ = daemon_event_tx.send(event);
                            }
                            None => break,
                        }
                    }
                    command = daemon_cmd_rx.recv() => {
                        let Some(command) = command else {
                            break;
                        };

                        match command {
                            DaemonCommand::Refresh => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "refresh",
                                    client.refresh(),
                                );
                            }
                            DaemonCommand::RefreshThreadsForAgent { agent_filter } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "refresh threads for agent",
                                    client.refresh_threads_for_agent(agent_filter),
                                );
                            }
                            DaemonCommand::GetConfig => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "get config",
                                    client.get_config(),
                                );
                            }
                            DaemonCommand::GetMlflowTracingStatus => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "get MLflow tracing status",
                                    client.get_mlflow_tracing_status(),
                                );
                            }
                            DaemonCommand::TestMlflowTracingConnection => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "test MLflow tracing connection",
                                    client.test_mlflow_tracing_connection(),
                                );
                            }
                            DaemonCommand::SendMlflowTracingTestTrace => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "send MLflow tracing test trace",
                                    client.send_mlflow_tracing_test_trace(),
                                );
                            }
                            DaemonCommand::RefreshServices => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "refresh services",
                                    client.refresh_services(),
                                );
                            }
                            DaemonCommand::ListTasks => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "list tasks",
                                    client.list_tasks(),
                                );
                            }
                            DaemonCommand::RequestThread {
                                thread_id,
                                message_limit,
                                message_offset,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request thread",
                                    client.request_thread(
                                        thread_id,
                                        message_limit,
                                        message_offset,
                                    ),
                                );
                            }
                            DaemonCommand::PinThreadMessageForCompaction {
                                thread_id,
                                message_id,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "pin thread message for compaction",
                                    client.pin_thread_message_for_compaction(
                                        thread_id,
                                        message_id,
                                    ),
                                );
                            }
                            DaemonCommand::UnpinThreadMessageForCompaction {
                                thread_id,
                                message_id,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "unpin thread message for compaction",
                                    client.unpin_thread_message_for_compaction(
                                        thread_id,
                                        message_id,
                                    ),
                                );
                            }
                            DaemonCommand::SubmitMessageFeedback {
                                thread_id,
                                message_id,
                                absolute_message_index,
                                reaction,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "submit message feedback",
                                    client.submit_message_feedback(
                                        thread_id,
                                        message_id,
                                        absolute_message_index,
                                        reaction,
                                    ),
                                );
                            }
                            DaemonCommand::RequestThreadTodos(thread_id) => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request thread todos",
                                    client.request_todos(thread_id),
                                );
                            }
                            DaemonCommand::RequestThreadWorkContext(thread_id) => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request thread work context",
                                    client.request_work_context(thread_id),
                                );
                            }
                            DaemonCommand::RequestGoalRunDetail(goal_run_id) => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request goal run detail",
                                    client.request_goal_run(goal_run_id),
                                );
                            }
                            DaemonCommand::RequestGoalRunDetailPage {
                                goal_run_id,
                                step_offset,
                                step_limit,
                                event_offset,
                                event_limit,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request goal run detail page",
                                    client.request_goal_run_page(
                                        goal_run_id,
                                        step_offset,
                                        step_limit,
                                        event_offset,
                                        event_limit,
                                    ),
                                );
                            }
                            DaemonCommand::RequestGoalRunCheckpoints(goal_run_id) => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request goal run checkpoints",
                                    client.list_checkpoints(goal_run_id),
                                );
                            }
                            DaemonCommand::ScheduleGoalHydrationRefresh(goal_run_id) => {
                                if queued_goal_hydration_ids.insert(goal_run_id.clone()) {
                                    queued_goal_hydrations.push_back(goal_run_id);
                                }
                            }
                            DaemonCommand::StartGoalRun {
                                goal,
                                thread_id,
                                session_id,
                                launch_assignments,
                            } => {
                                let _ = client.start_goal_run(
                                    goal,
                                    thread_id,
                                    session_id,
                                    launch_assignments,
                                );
                            }
                            DaemonCommand::ExplainAction {
                                action_id,
                                step_index,
                            } => {
                                let _ = client.explain_action(action_id, step_index);
                            }
                            DaemonCommand::StartDivergentSession {
                                problem_statement,
                                thread_id,
                                goal_run_id,
                            } => {
                                let _ = client.start_divergent_session(
                                    problem_statement,
                                    thread_id,
                                    goal_run_id,
                                );
                            }
                            DaemonCommand::GetDivergentSession { session_id } => {
                                let _ = client.get_divergent_session(session_id);
                            }
                            DaemonCommand::RequestGitDiff { repo_path, file_path } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request git diff",
                                    client.request_git_diff(repo_path, file_path),
                                );
                            }
                            DaemonCommand::RequestFilePreview { path, max_bytes } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request file preview",
                                    client.request_file_preview(path, max_bytes),
                                );
                            }
                            DaemonCommand::RequestAgentStatus => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request agent status",
                                    client.request_agent_status(),
                                );
                            }
                            DaemonCommand::CancelTask { task_id } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "cancel task",
                                    client.cancel_task(task_id),
                                );
                            }
                            DaemonCommand::RequestAgentStatistics { window } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request agent statistics",
                                    client.request_agent_statistics(window),
                                );
                            }
                            DaemonCommand::RequestPromptInspection { agent_id } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "request prompt inspection",
                                    client.request_prompt_inspection(agent_id),
                                );
                            }
                            DaemonCommand::SendMessage {
                                thread_id,
                                content,
                                content_blocks_json,
                                session_id,
                                target_agent_id,
                            } => {
                                let _ = client.send_message(
                                    thread_id,
                                    content,
                                    content_blocks_json,
                                    session_id,
                                    target_agent_id,
                                );
                            }
                            DaemonCommand::ForkThread {
                                thread_id,
                                thread_json,
                                messages_json,
                                refresh_message_limit,
                            } => {
                                let create_result = client.send(
                                    zorai_protocol::ClientMessage::CreateAgentThread {
                                        thread_json,
                                    },
                                );
                                if let Err(err) = create_result {
                                    let _ = daemon_event_tx.send(crate::client::ClientEvent::Error(
                                        format!("fork thread create failed: {err}"),
                                    ));
                                    continue;
                                }
                                for message_json in messages_json {
                                    if let Err(err) = client.send(
                                        zorai_protocol::ClientMessage::AddAgentMessage {
                                            message_json,
                                        },
                                    ) {
                                        let _ = daemon_event_tx.send(
                                            crate::client::ClientEvent::Error(format!(
                                                "fork thread message write failed: {err}"
                                            )),
                                        );
                                        break;
                                    }
                                }
                                let _ =
                                    client.request_thread(thread_id, Some(refresh_message_limit), Some(0));
                            }
                            DaemonCommand::ExportThread { thread_id, message_id } => {
                                if let Err(err) = client.send(
                                    zorai_protocol::ClientMessage::ExportAgentThread { thread_id, message_id },
                                ) {
                                    let _ = daemon_event_tx.send(crate::client::ClientEvent::Error(
                                        format!("export thread request failed: {err}"),
                                    ));
                                }
                            }
                            DaemonCommand::InternalDelegate {
                                thread_id,
                                target_agent_id,
                                content,
                                session_id,
                            } => {
                                let _ = client.send_internal_delegate(
                                    thread_id,
                                    target_agent_id,
                                    content,
                                    session_id,
                                );
                            }
                            DaemonCommand::ThreadParticipantCommand {
                                thread_id,
                                target_agent_id,
                                action,
                                instruction,
                                session_id,
                            } => {
                                let _ = client.send_thread_participant_command(
                                    thread_id,
                                    target_agent_id,
                                    action,
                                    instruction,
                                    session_id,
                                );
                            }
                            DaemonCommand::ThreadHandoff {
                                thread_id,
                                action,
                                target_agent_id,
                                reason,
                                summary,
                            } => {
                                forward_bridge_command_result(
                                    &daemon_event_tx,
                                    "thread handoff",
                                    client.send_thread_handoff(
                                        thread_id,
                                        action,
                                        target_agent_id,
                                        reason,
                                        summary,
                                    ),
                                );
                            }
                            DaemonCommand::SendParticipantSuggestion {
                                thread_id,
                                suggestion_id,
                            } => {
                                let _ = client.send_participant_suggestion(thread_id, suggestion_id);
                            }
                            DaemonCommand::DismissParticipantSuggestion {
                                thread_id,
                                suggestion_id,
                            } => {
                                let _ = client.dismiss_participant_suggestion(thread_id, suggestion_id);
                            }
                            DaemonCommand::StopStream { thread_id } => {
                                let _ = client.stop_stream(thread_id);
                            }
                            DaemonCommand::ForceCompact { thread_id } => {
                                let _ = client.force_compact(thread_id);
                            }
                            DaemonCommand::RetryStreamNow { thread_id } => {
                                let _ = client.retry_stream_now(thread_id);
                            }
                            DaemonCommand::DeleteMessages { thread_id, message_ids } => {
                                let _ = client.delete_messages(thread_id, message_ids);
                            }
                            DaemonCommand::DeleteThread { thread_id } => {
                                let _ = client.delete_thread(thread_id);
                            }
                            DaemonCommand::FetchModels {
                                provider_id,
                                base_url,
                                api_key,
                                output_modalities,
                            } => {
                                let _ = client.fetch_models(
                                    provider_id,
                                    base_url,
                                    api_key,
                                    output_modalities,
                                );
                            }
                            DaemonCommand::SetConfigItem { key_path, value_json } => {
                                let _ = client.set_config_item_json(key_path, value_json);
                            }
                            DaemonCommand::ExternalRuntimeMigrationStatus => {
                                let _ = client.external_runtime_migration_status();
                            }
                            DaemonCommand::ExternalRuntimeMigrationPreview {
                                runtime,
                                config_path,
                            } => {
                                let _ = client.external_runtime_migration_preview(runtime, config_path);
                            }
                            DaemonCommand::ExternalRuntimeMigrationApply {
                                runtime,
                                config_path,
                                conflict_policy,
                            } => {
                                let _ = client.external_runtime_migration_apply(
                                    runtime,
                                    config_path,
                                    conflict_policy,
                                );
                            }
                            DaemonCommand::ExternalRuntimeMigrationReport { runtime, limit } => {
                                let _ = client.external_runtime_migration_report(runtime, limit);
                            }
                            DaemonCommand::ExternalRuntimeMigrationShadowRun { runtime } => {
                                let _ = client.external_runtime_migration_shadow_run(runtime);
                            }
                            DaemonCommand::SetProviderModel { provider_id, model } => {
                                let _ = client.set_provider_model(provider_id, model);
                            }
                            DaemonCommand::SetTargetAgentProviderModel {
                                target_agent_id,
                                provider_id,
                                model,
                            } => {
                                let _ = client.set_target_agent_provider_model(
                                    target_agent_id,
                                    provider_id,
                                    model,
                                );
                            }
                            DaemonCommand::SetTargetAgentReasoningEffort {
                                target_agent_id,
                                reasoning_effort,
                            } => {
                                let _ = client.set_target_agent_reasoning_effort(
                                    target_agent_id,
                                    reasoning_effort,
                                );
                            }
                            DaemonCommand::SetTargetAgentContextWindow {
                                target_agent_id,
                                context_window_tokens,
                            } => {
                                let _ = client.set_target_agent_context_window(
                                    target_agent_id,
                                    context_window_tokens,
                                );
                            }
                            DaemonCommand::SetThreadExecutionProfile {
                                thread_id,
                                profile_json,
                            } => {
                                let _ =
                                    client.set_thread_execution_profile(thread_id, profile_json);
                            }
                            DaemonCommand::ControlGoalRun {
                                goal_run_id,
                                action,
                                step_index,
                                payload_json,
                            } => {
                                let _ = client.control_goal_run(
                                    goal_run_id,
                                    action,
                                    step_index,
                                    payload_json,
                                );
                            }
                            DaemonCommand::DeleteGoalRun { goal_run_id } => {
                                let _ = client.delete_goal_run(goal_run_id);
                            }
                            DaemonCommand::ListWorkspaceSettings => {
                                let _ = client.list_workspace_settings();
                            }
                            DaemonCommand::GetWorkspaceSettings { workspace_id } => {
                                let _ = client.get_workspace_settings(workspace_id);
                            }
                            DaemonCommand::SetWorkspaceOperator {
                                workspace_id,
                                operator,
                            } => {
                                let _ = client.set_workspace_operator(workspace_id, operator);
                            }
                            DaemonCommand::SetWorkspaceRepoMonitor {
                                workspace_id,
                                repo_monitor_enabled,
                                repo_monitor_include_dirs,
                                repo_monitor_exclude_dirs,
                            } => {
                                let _ = client.set_workspace_repo_monitor(
                                    workspace_id,
                                    repo_monitor_enabled,
                                    repo_monitor_include_dirs,
                                    repo_monitor_exclude_dirs,
                                );
                            }
                            DaemonCommand::CreateWorkspaceTask(request) => {
                                let _ = client.create_workspace_task(request);
                            }
                            DaemonCommand::ListWorkspaceTasks {
                                workspace_id,
                                include_deleted,
                            } => {
                                let _ = client.list_workspace_tasks(workspace_id, include_deleted);
                            }
                            DaemonCommand::ListWorkspaceNotices {
                                workspace_id,
                                task_id,
                            } => {
                                let _ = client.list_workspace_notices(workspace_id, task_id);
                            }
                            DaemonCommand::UpdateWorkspaceTask { task_id, update } => {
                                let _ = client.update_workspace_task(task_id, update);
                            }
                            DaemonCommand::RunWorkspaceTask(task_id) => {
                                let _ = client.run_workspace_task(task_id);
                            }
                            DaemonCommand::PauseWorkspaceTask(task_id) => {
                                let _ = client.pause_workspace_task(task_id);
                            }
                            DaemonCommand::StopWorkspaceTask(task_id) => {
                                let _ = client.stop_workspace_task(task_id);
                            }
                            DaemonCommand::MoveWorkspaceTask(request) => {
                                let _ = client.move_workspace_task(request);
                            }
                            DaemonCommand::SubmitWorkspaceReview(review) => {
                                let _ = client.submit_workspace_review(review);
                            }
                            DaemonCommand::DeleteWorkspaceTask(task_id) => {
                                let _ = client.delete_workspace_task(task_id);
                            }
                            DaemonCommand::ListTaskApprovalRules => {
                                let _ = client.list_task_approval_rules();
                            }
                            DaemonCommand::CreateTaskApprovalRule { approval_id } => {
                                let _ = client.create_task_approval_rule(approval_id);
                            }
                            DaemonCommand::RevokeTaskApprovalRule { rule_id } => {
                                let _ = client.revoke_task_approval_rule(rule_id);
                            }
                            DaemonCommand::ResolveTaskApproval { approval_id, decision } => {
                                let _ = client.resolve_task_approval(approval_id, decision);
                            }
                            DaemonCommand::SpawnSession { shell, cwd, cols, rows } => {
                                let _ = client.spawn_session(shell, cwd, cols, rows);
                            }
                            DaemonCommand::GetProviderAuthStates => {
                                let _ = client.get_provider_auth_states();
                            }
                            DaemonCommand::GetOpenAICodexAuthStatus => {
                                let _ = client.get_openai_codex_auth_status();
                            }
                            DaemonCommand::LoginOpenAICodex => {
                                let _ = client.login_openai_codex();
                            }
                            DaemonCommand::LogoutOpenAICodex => {
                                let _ = client.logout_openai_codex();
                            }
                            DaemonCommand::LoginProvider {
                                provider_id,
                                api_key,
                                base_url,
                            } => {
                                let _ = client.login_provider(provider_id, api_key, base_url);
                            }
                            DaemonCommand::LogoutProvider { provider_id } => {
                                let _ = client.logout_provider(provider_id);
                            }
                            DaemonCommand::ValidateProvider { provider_id, base_url, api_key, auth_source } => {
                                let _ = client.validate_provider(provider_id, base_url, api_key, auth_source);
                            }
                            DaemonCommand::SetSubAgent(sub_agent_json) => {
                                let _ = client.set_sub_agent(sub_agent_json);
                            }
                            DaemonCommand::RemoveSubAgent(sub_agent_id) => {
                                let _ = client.remove_sub_agent(sub_agent_id);
                            }
                            DaemonCommand::ListSubAgents => {
                                let _ = client.list_sub_agents();
                            }
                            DaemonCommand::GetConciergeConfig => {
                                let _ = client.get_concierge_config();
                            }
                            DaemonCommand::SetConciergeConfig(config_json) => {
                                let _ = client.set_concierge_config(config_json);
                            }
                            DaemonCommand::RequestConciergeWelcome => {
                                let _ = client.request_concierge_welcome();
                            }
                            DaemonCommand::RetryOperatorProfile => {
                                let _ = client.request_concierge_welcome();
                            }
                            DaemonCommand::StartOperatorProfileSession { kind } => {
                                let _ = client.start_operator_profile_session(kind);
                            }
                            DaemonCommand::NextOperatorProfileQuestion { session_id } => {
                                let _ = client.next_operator_profile_question(session_id);
                            }
                            DaemonCommand::SubmitOperatorProfileAnswer {
                                session_id,
                                question_id,
                                answer_json,
                            } => {
                                let _ = client.submit_operator_profile_answer(
                                    session_id,
                                    question_id,
                                    answer_json,
                                );
                            }
                            DaemonCommand::SkipOperatorProfileQuestion {
                                session_id,
                                question_id,
                                reason,
                            } => {
                                let _ = client.skip_operator_profile_question(
                                    session_id,
                                    question_id,
                                    reason,
                                );
                            }
                            DaemonCommand::DeferOperatorProfileQuestion {
                                session_id,
                                question_id,
                                defer_until_unix_ms,
                            } => {
                                let _ = client.defer_operator_profile_question(
                                    session_id,
                                    question_id,
                                    defer_until_unix_ms,
                                );
                            }
                            DaemonCommand::AnswerOperatorQuestion { question_id, answer } => {
                                let _ = client.answer_operator_question(question_id, answer);
                            }
                            DaemonCommand::GetOperatorProfileSummary => {
                                let _ = client.get_operator_profile_summary();
                            }
                            DaemonCommand::GetOperatorModel => {
                                let _ = client.get_operator_model();
                            }
                            DaemonCommand::ResetOperatorModel => {
                                let _ = client.reset_operator_model();
                            }
                            DaemonCommand::GetCollaborationSessions => {
                                let _ = client.get_collaboration_sessions();
                            }
                            DaemonCommand::VoteOnCollaborationDisagreement {
                                parent_task_id,
                                disagreement_id,
                                task_id,
                                position,
                                confidence,
                            } => {
                                let _ = client.vote_on_collaboration_disagreement(
                                    parent_task_id,
                                    disagreement_id,
                                    task_id,
                                    position,
                                    confidence,
                                );
                            }
                            DaemonCommand::GetGeneratedTools => {
                                let _ = client.get_generated_tools();
                            }
                            DaemonCommand::SpeechToText { args_json } => {
                                let _ = client.speech_to_text(args_json);
                            }
                            DaemonCommand::TextToSpeech { args_json } => {
                                let _ = client.text_to_speech(args_json);
                            }
                            DaemonCommand::GenerateImage { args_json } => {
                                let _ = client.generate_image(args_json);
                            }
                            DaemonCommand::SetOperatorProfileConsent {
                                consent_key,
                                granted,
                            } => {
                                let _ = client.set_operator_profile_consent(consent_key, granted);
                            }
                            DaemonCommand::DismissConciergeWelcome => {
                                let _ = client.dismiss_concierge_welcome();
                            }
                            DaemonCommand::RecordAttention {
                                surface,
                                thread_id,
                                goal_run_id,
                            } => {
                                let _ = client.record_attention(surface, thread_id, goal_run_id);
                            }
                            DaemonCommand::AuditDismiss { entry_id } => {
                                let _ = client.dismiss_audit_entry(entry_id);
                            }
                            DaemonCommand::PluginList => {
                                let _ = client.plugin_list();
                            }
                            DaemonCommand::PluginInstallSource(source) => {
                                install_plugin_source_from_tui(daemon_event_tx.clone(), source);
                            }
                            DaemonCommand::PluginGet(name) => {
                                let _ = client.plugin_get(name);
                            }
                            DaemonCommand::PluginEnable(name) => {
                                let _ = client.plugin_enable(name);
                            }
                            DaemonCommand::PluginDisable(name) => {
                                let _ = client.plugin_disable(name);
                            }
                            DaemonCommand::PluginGetSettings(name) => {
                                let _ = client.plugin_get_settings(name);
                            }
                            DaemonCommand::PluginUpdateSetting {
                                plugin_name,
                                key,
                                value,
                                is_secret,
                            } => {
                                let _ = client.plugin_update_setting(
                                    plugin_name,
                                    key,
                                    value,
                                    is_secret,
                                );
                            }
                            DaemonCommand::PluginTestConnection(name) => {
                                let _ = client.plugin_test_connection(name);
                            }
                            DaemonCommand::PluginListCommands => {
                                let _ = client.plugin_list_commands();
                            }
                            DaemonCommand::PluginOAuthStart(name) => {
                                let _ = client.plugin_oauth_start(name);
                            }
                            DaemonCommand::ListNotifications => {
                                let _ = client.list_notifications();
                            }
                            DaemonCommand::UpsertNotification(notification) => {
                                let _ = client.upsert_notification(notification);
                            }
                            DaemonCommand::MarkAllNotificationsRead => {
                                let _ = client.mark_all_notifications_read();
                            }
                            DaemonCommand::ArchiveReadNotifications => {
                                let _ = client.archive_read_notifications();
                            }
                            DaemonCommand::EnqueuePrompt {
                                thread_id,
                                prompt_id,
                                content,
                                content_blocks_json,
                            } => {
                                let _ = client.enqueue_prompt(
                                    thread_id,
                                    prompt_id,
                                    content,
                                    content_blocks_json,
                                );
                            }
                            DaemonCommand::ListPromptQueue { thread_id } => {
                                let _ = client.list_prompt_queue(thread_id);
                            }
                            DaemonCommand::UpdateQueuedPrompt {
                                thread_id,
                                prompt_id,
                                content,
                                content_blocks_json,
                            } => {
                                let _ = client.update_queued_prompt(
                                    thread_id,
                                    prompt_id,
                                    content,
                                    content_blocks_json,
                                );
                            }
                            DaemonCommand::CancelQueuedPrompt {
                                thread_id,
                                prompt_id,
                            } => {
                                let _ = client.cancel_queued_prompt(thread_id, prompt_id);
                            }
                            DaemonCommand::SendQueuedPromptNow {
                                thread_id,
                                prompt_id,
                            } => {
                                let _ = client.send_queued_prompt_now(thread_id, prompt_id);
                            }
                            DaemonCommand::WhatsAppLinkStart => {
                                let _ = client.whatsapp_link_start();
                            }
                            DaemonCommand::WhatsAppLinkStop => {
                                let _ = client.whatsapp_link_stop();
                            }
                            DaemonCommand::WhatsAppLinkStatus => {
                                let _ = client.whatsapp_link_status();
                            }
                            DaemonCommand::WhatsAppLinkSubscribe => {
                                let _ = client.whatsapp_link_subscribe();
                            }
                            DaemonCommand::WhatsAppLinkUnsubscribe => {
                                let _ = client.whatsapp_link_unsubscribe();
                            }
                            DaemonCommand::WhatsAppLinkReset => {
                                let _ = client.whatsapp_link_reset();
                            }
                            DaemonCommand::DatabaseSyncNow => {
                                let _ = client.database_sync_now();
                            }
                            DaemonCommand::DatabaseGetBackend => {
                                let _ = client.database_get_backend();
                            }
                        }
                    }
                }

                if daemon_cmd_rx.is_empty() {
                    if let Some(goal_run_id) = queued_goal_hydrations.pop_front() {
                        queued_goal_hydration_ids.remove(&goal_run_id);
                        dispatch_background_goal_hydration(
                            &client,
                            &daemon_event_tx,
                            goal_run_id,
                        )
                        .await;
                    }
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn fetch_models_command_does_not_use_local_http_bridge() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test http server");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let addr = listener.local_addr().expect("listener addr");
        let hit = Arc::new(AtomicBool::new(false));
        let hit_for_server = Arc::clone(&hit);

        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(700);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        hit_for_server.store(true, Ordering::SeqCst);
                        let mut buffer = [0_u8; 1024];
                        let _ = stream.read(&mut buffer);
                        let body = r#"{"data":[{"id":"sdk-model"}]}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });

        let (daemon_event_tx, _daemon_event_rx) = mpsc::channel();
        let (daemon_cmd_tx, daemon_cmd_rx) = tokio_mpsc::unbounded_channel();
        start_daemon_bridge(daemon_event_tx, daemon_cmd_rx);

        daemon_cmd_tx
            .send(DaemonCommand::FetchModels {
                provider_id: zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                output_modalities: None,
            })
            .expect("queue fetch models command");

        thread::sleep(Duration::from_millis(350));

        assert!(
            !hit.load(Ordering::SeqCst),
            "fetch models should be delegated to the daemon, not fetched directly over HTTP"
        );

        server.join().expect("join test http server");
    }

    #[test]
    fn tui_log_filter_defaults_to_error_only() {
        let filter = build_log_filter(None, None);

        assert_eq!(filter.to_string(), "error");
    }

    #[test]
    fn write_runtime_marker_overwrites_with_latest_message() {
        let tempdir = tempfile::tempdir().expect("temporary directory should be creatable");
        let marker_path = tempdir.path().join("zorai-tui.last-state");

        write_runtime_marker_at(&marker_path, "startup: terminal initialized")
            .expect("first marker write should succeed");
        write_runtime_marker_at(&marker_path, "exit: clean")
            .expect("second marker write should succeed");

        let contents =
            std::fs::read_to_string(&marker_path).expect("marker file should be readable");
        assert!(contents.contains("message=exit: clean"));
        assert!(!contents.contains("message=startup: terminal initialized"));
    }

    #[test]
    fn bridge_command_failure_surfaces_error_and_disconnect() {
        let (daemon_event_tx, daemon_event_rx) = mpsc::channel();
        let (client_event_tx, _client_event_rx) = tokio_mpsc::channel(8);
        let client = DaemonClient::new(client_event_tx);

        client.close_request_queue_for_test();

        forward_bridge_command_result(&daemon_event_tx, "refresh", client.refresh());

        match daemon_event_rx
            .recv()
            .expect("bridge failure should emit an error event")
        {
            client::ClientEvent::Error(message) => {
                assert!(message.contains("refresh"));
                assert!(message.contains("closed"));
            }
            other => panic!("expected error event, got {other:?}"),
        }

        match daemon_event_rx
            .recv()
            .expect("bridge failure should emit a disconnected event")
        {
            client::ClientEvent::Disconnected => {}
            other => panic!("expected disconnected event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn background_goal_hydration_send_failure_emits_clear_event() {
        let (daemon_event_tx, daemon_event_rx) = mpsc::channel();
        let (client_event_tx, _client_event_rx) = tokio_mpsc::channel(8);
        let client = DaemonClient::new(client_event_tx);
        client.close_request_queue_for_test();

        dispatch_background_goal_hydration(&client, &daemon_event_tx, "goal-1".to_string()).await;

        match daemon_event_rx
            .recv()
            .expect("background failure should emit an error event")
        {
            client::ClientEvent::Error(message) => {
                assert!(message.contains("background goal hydration"));
            }
            other => panic!("expected error event, got {other:?}"),
        }

        match daemon_event_rx
            .recv()
            .expect("background failure should emit a disconnect event")
        {
            client::ClientEvent::Disconnected => {}
            other => panic!("expected disconnected event, got {other:?}"),
        }

        match daemon_event_rx
            .recv()
            .expect("background failure should emit a cleanup event")
        {
            client::ClientEvent::GoalHydrationScheduleFailed { goal_run_id } => {
                assert_eq!(goal_run_id, "goal-1");
            }
            other => panic!("expected goal hydration failure event, got {other:?}"),
        }
    }

    #[test]
    fn panic_payload_message_handles_common_payload_types() {
        assert_eq!(panic_payload_message(&"plain panic"), "plain panic");

        let owned = String::from("owned panic");
        assert_eq!(panic_payload_message(&owned), "owned panic");
    }

    #[test]
    fn terminal_errors_retry_only_within_threshold() {
        assert!(should_retry_terminal_error(0));
        assert!(should_retry_terminal_error(
            MAX_TRANSIENT_TERMINAL_ERRORS - 1
        ));
        assert!(!should_retry_terminal_error(MAX_TRANSIENT_TERMINAL_ERRORS));
    }

    #[test]
    fn idle_loop_uses_coarser_tick_without_sliding_tick_units() {
        let base_tick = Duration::from_millis(crate::app::TUI_TICK_RATE_MS);

        assert_eq!(idle_loop_tick_rate(base_tick), base_tick * 5);
        assert_eq!(elapsed_model_ticks(base_tick, base_tick), 1);
        assert_eq!(elapsed_model_ticks(base_tick * 5, base_tick), 5);
    }
}
