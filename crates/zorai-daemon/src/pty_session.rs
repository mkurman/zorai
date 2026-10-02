use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;
use zorai_protocol::{DaemonMessage, ManagedCommandRequest, SessionId, SnapshotInfo};

use crate::history::{HistoryStore, ManagedHistoryRecord};
use crate::network;
use crate::osc::parse_osc_notifications;
use crate::sandbox;

mod helpers;
pub use helpers::sanitize_scrollback_for_replay;
use helpers::{configure_shell_command, default_shell, dispatch_managed_command, pty_reader_loop};

/// Rolling scrollback buffer capacity (bytes).
const SCROLLBACK_CAPACITY: usize = 1024 * 1024;

/// A single terminal session backed by a PTY.
pub struct PtySession {
    id: SessionId,
    master: Box<dyn MasterPty + Send>,
    master_write: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    child: Arc<std::sync::Mutex<Box<dyn Child + Send + Sync>>>,
    tx: broadcast::Sender<DaemonMessage>,
    cols: u16,
    rows: u16,
    shell: Option<String>,
    cwd: Option<String>,
    workspace_id: Option<String>,
    created_at: u64,
    last_activity_at_ms: Arc<AtomicU64>,
    scrollback: Arc<std::sync::Mutex<Vec<u8>>>,
    dead: Arc<AtomicBool>,
    managed_lane: Arc<std::sync::Mutex<ManagedLaneState>>,
    /// Last active (still-running) command detected via shell integration markers.
    active_command: Arc<std::sync::Mutex<Option<String>>>,
    /// CWD reported by the shell integration (works on all platforms).
    tracked_cwd: Arc<std::sync::Mutex<Option<String>>>,
}

struct ManagedQueuedCommand {
    execution_id: String,
    request: ManagedCommandRequest,
    snapshot: Option<SnapshotInfo>,
}

struct ActiveManagedCommand {
    execution_id: String,
    request: ManagedCommandRequest,
    snapshot: Option<SnapshotInfo>,
    enqueued_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManagedCommandLiveState {
    Queued,
    Running,
}

#[derive(Debug, Clone)]
pub(crate) struct ManagedCommandLiveStatus {
    pub state: ManagedCommandLiveState,
    pub position: usize,
    pub command: String,
    pub snapshot_path: Option<String>,
}

#[derive(Default)]
struct ManagedLaneState {
    active: Option<ActiveManagedCommand>,
    queue: VecDeque<ManagedQueuedCommand>,
}

enum CommandLifecycleMarker {
    Started(String),
    Finished(Option<i32>),
    Cwd(String),
}

impl PtySession {
    /// Spawn a new PTY with the given shell and dimensions.
    pub fn spawn(
        id: SessionId,
        shell: Option<String>,
        cwd: Option<String>,
        workspace_id: Option<String>,
        env: Option<Vec<(String, String)>>,
        cols: u16,
        rows: u16,
        history: HistoryStore,
        pty_channel_capacity: usize,
    ) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let configured_shell = shell;
        let shell_program = configured_shell.clone().unwrap_or_else(default_shell);
        tracing::info!(id = %id, shell = %shell_program, "starting PTY shell");
        let mut cmd = CommandBuilder::new(&shell_program);
        configure_shell_command(&shell_program, &mut cmd, cwd.as_deref())?;

        #[cfg(windows)]
        {
            let is_wsl = std::path::Path::new(&shell_program)
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_ascii_lowercase())
                .is_some_and(|name| name == "wsl" || name == "wsl.exe");
            if !is_wsl {
                if let Some(ref dir) = cwd {
                    cmd.cwd(dir);
                }
            }
        }
        #[cfg(not(windows))]
        if let Some(ref dir) = cwd {
            cmd.cwd(dir);
        }

        if let Some(vars) = &env {
            for (key, value) in vars {
                cmd.env(key, value);
            }
        }

        let child = Arc::new(std::sync::Mutex::new(pair.slave.spawn_command(cmd)?));
        drop(pair.slave);

        let master_read = pair.master.try_clone_reader()?;
        let master_write = Arc::new(std::sync::Mutex::new(pair.master.take_writer()?));

        let (tx, _) = broadcast::channel(pty_channel_capacity);
        let scrollback = Arc::new(std::sync::Mutex::new(Vec::with_capacity(
            SCROLLBACK_CAPACITY,
        )));
        let dead = Arc::new(AtomicBool::new(false));
        let managed_lane = Arc::new(std::sync::Mutex::new(ManagedLaneState::default()));
        let active_command: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        let tracked_cwd: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last_activity_at_ms = Arc::new(AtomicU64::new(unix_millis()));

        {
            let tx = tx.clone();
            let scrollback = scrollback.clone();
            let dead = dead.clone();
            let master_write = master_write.clone();
            let managed_lane = managed_lane.clone();
            let active_command = active_command.clone();
            let tracked_cwd = tracked_cwd.clone();
            let last_activity_at_ms = last_activity_at_ms.clone();
            let workspace_id = workspace_id.clone();
            let cwd = cwd.clone();
            let rt_handle = tokio::runtime::Handle::current();
            std::thread::Builder::new()
                .name(format!("pty-reader-{id}"))
                .spawn(move || {
                    pty_reader_loop(
                        id,
                        master_read,
                        master_write,
                        tx,
                        scrollback,
                        dead,
                        managed_lane,
                        active_command,
                        tracked_cwd,
                        last_activity_at_ms,
                        history,
                        workspace_id,
                        cwd,
                        rt_handle,
                    );
                })?;
        }

        Ok(Self {
            id,
            master: pair.master,
            master_write,
            child,
            tx,
            cols,
            rows,
            shell: configured_shell.or_else(|| Some(shell_program)),
            cwd,
            workspace_id,
            created_at,
            last_activity_at_ms,
            scrollback,
            dead,
            managed_lane,
            active_command,
            tracked_cwd,
        })
    }

    /// Write raw bytes into the PTY's stdin.
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.touch_activity();
        let mut writer = self.master_write.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn queue_managed_command(
        &mut self,
        execution_id: String,
        request: ManagedCommandRequest,
        snapshot: Option<SnapshotInfo>,
    ) -> Result<usize> {
        self.touch_activity();
        let mut lane = self.managed_lane.lock().unwrap();
        if lane.active.is_none() {
            dispatch_managed_command(&self.master_write, &request, self.cwd.as_deref())?;
            lane.active = Some(ActiveManagedCommand {
                execution_id,
                request,
                snapshot,
                enqueued_at: Instant::now(),
            });
            Ok(0)
        } else {
            lane.queue.push_back(ManagedQueuedCommand {
                execution_id,
                request,
                snapshot,
            });
            Ok(lane.queue.len())
        }
    }

    pub(crate) fn managed_command_status(
        &self,
        execution_id: &str,
    ) -> Option<ManagedCommandLiveStatus> {
        let lane = self.managed_lane.lock().unwrap();

        if let Some(active) = lane.active.as_ref() {
            if active.execution_id == execution_id {
                return Some(ManagedCommandLiveStatus {
                    state: ManagedCommandLiveState::Running,
                    position: 0,
                    command: active.request.command.clone(),
                    snapshot_path: active
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.path.clone()),
                });
            }
        }

        for (index, queued) in lane.queue.iter().enumerate() {
            if queued.execution_id == execution_id {
                return Some(ManagedCommandLiveStatus {
                    state: ManagedCommandLiveState::Queued,
                    position: index + 1,
                    command: queued.request.command.clone(),
                    snapshot_path: queued
                        .snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.path.clone()),
                });
            }
        }

        None
    }

    pub(crate) fn cancel_queued_managed_command(&self, execution_id: &str) -> bool {
        let mut lane = self.managed_lane.lock().unwrap();
        let before = lane.queue.len();
        lane.queue
            .retain(|queued| queued.execution_id != execution_id);
        lane.queue.len() != before
    }

    /// Resize the PTY.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        tracing::debug!(id = %self.id, cols, rows, "resize recorded");
        Ok(())
    }

    /// Kill the child process.
    pub fn kill(&mut self) -> Result<()> {
        self.child.lock().unwrap().kill()?;
        self.dead.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Subscribe to the output broadcast.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonMessage> {
        self.tx.subscribe()
    }

    /// Return a copy of the scrollback (optionally tail `max_lines` lines).
    pub fn scrollback(&self, max_lines: Option<usize>) -> Vec<u8> {
        let buf = self.scrollback.lock().unwrap();
        if let Some(line_limit) = max_lines {
            let mut count = 0;
            let mut start = buf.len();
            for (index, &byte) in buf.iter().enumerate().rev() {
                if byte == b'\n' {
                    count += 1;
                    if count >= line_limit {
                        start = index + 1;
                        break;
                    }
                }
            }
            buf[start..].to_vec()
        } else {
            buf.clone()
        }
    }

    /// Preload output bytes into session scrollback and current subscribers.
    pub fn preload_output(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        {
            let mut sb = self.scrollback.lock().unwrap();
            sb.extend_from_slice(data);
            if sb.len() > SCROLLBACK_CAPACITY {
                let excess = sb.len() - SCROLLBACK_CAPACITY;
                sb.drain(..excess);
            }
        }

        let _ = self.tx.send(DaemonMessage::Output {
            id: self.id,
            data: data.to_vec(),
        });
    }

    pub fn title(&self) -> Option<&str> {
        None
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Resolve the most current working directory for this PTY process.
    /// Priority: shell-integration tracked CWD → /proc/{pid}/cwd (Unix) → startup CWD.
    pub fn resolved_cwd(&self) -> Option<String> {
        if let Some(cwd) = self.tracked_cwd.lock().unwrap().clone() {
            return Some(cwd);
        }

        #[cfg(unix)]
        {
            if let Some(pid) = self.child.lock().unwrap().process_id() {
                let proc_cwd = format!("/proc/{pid}/cwd");
                if let Ok(path) = std::fs::read_link(proc_cwd) {
                    return Some(path.to_string_lossy().into_owned());
                }
            }
        }

        self.cwd.clone()
    }

    pub fn shell(&self) -> Option<&str> {
        self.shell.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn created_at(&self) -> u64 {
        self.created_at
    }

    pub fn last_activity_at_ms(&self) -> u64 {
        self.last_activity_at_ms.load(Ordering::Relaxed)
    }

    pub fn has_managed_work(&self) -> bool {
        let lane = self.managed_lane.lock().unwrap();
        lane.active.is_some() || !lane.queue.is_empty()
    }

    fn touch_activity(&self) {
        self.last_activity_at_ms
            .store(unix_millis(), Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn set_last_activity_at_ms_for_test(&self, ms: u64) {
        self.last_activity_at_ms.store(ms, Ordering::Relaxed);
    }

    pub fn is_dead(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    /// Return the last active (still-running) command detected via shell
    /// integration markers, or `None` if the shell has no integration or
    /// the last command has finished.
    pub fn active_command(&self) -> Option<String> {
        self.active_command.lock().unwrap().clone()
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
