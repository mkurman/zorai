use super::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use zorai_protocol::tool_names;

const SESSION_GRANT: Duration = Duration::from_secs(10 * 60);
const IDLE_RELEASE: Duration = Duration::from_secs(2 * 60);
const USER_INPUT_MARGIN_MS: u64 = 400;
const TAKEOVER_POLL: Duration = Duration::from_millis(500);
const APPROVAL_PREFIX: &str = "computer-";
const OPERATOR_HOLD_NOTE: &str = "The operator took back control of the mouse and keyboard, so this action was not performed. Stop using computer tools now: do not retry or wait for approval. Briefly tell the operator where you stopped; you may continue only after they send a new message.";

pub(crate) fn computer_tool_definitions() -> Vec<ToolDefinition> {
    use crate::agent::tool_executor::tool_def;
    let point = |extra: Value| {
        let mut properties = serde_json::json!({
            "x": { "type": "integer", "description": "Horizontal pixel in the latest computer_screenshot" },
            "y": { "type": "integer", "description": "Vertical pixel in the latest computer_screenshot" }
        });
        if let (Some(map), Value::Object(more)) = (properties.as_object_mut(), extra) {
            map.extend(more);
        }
        properties
    };
    vec![
        tool_def(
            tool_names::COMPUTER_SCREENSHOT,
            "Capture the user's whole screen as a PNG in the thread files and return its path and size; pass the path to analyze_image to see it. Take one before clicking so coordinates match the screen.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool_def(
            tool_names::COMPUTER_CLICK,
            "Click at a point on the user's screen (coordinates from the latest computer_screenshot). The operator approves each action.",
            serde_json::json!({
                "type": "object",
                "properties": point(serde_json::json!({
                    "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "Mouse button (default left)" },
                    "clicks": { "type": "integer", "minimum": 1, "maximum": 3, "description": "1 = click, 2 = double-click, 3 = triple-click (default 1)" }
                })),
                "required": ["x", "y"]
            }),
        ),
        tool_def(
            tool_names::COMPUTER_MOVE,
            "Move the mouse pointer to a point on the user's screen without clicking, e.g. to reveal a hover menu.",
            serde_json::json!({ "type": "object", "properties": point(serde_json::json!({})), "required": ["x", "y"] }),
        ),
        tool_def(
            tool_names::COMPUTER_DRAG,
            "Press the left button at one point, drag to another and release.",
            serde_json::json!({
                "type": "object",
                "properties": point(serde_json::json!({
                    "to_x": { "type": "integer", "description": "Horizontal pixel to drag to" },
                    "to_y": { "type": "integer", "description": "Vertical pixel to drag to" }
                })),
                "required": ["x", "y", "to_x", "to_y"]
            }),
        ),
        tool_def(
            tool_names::COMPUTER_SCROLL,
            "Scroll at a point on the user's screen.",
            serde_json::json!({
                "type": "object",
                "properties": point(serde_json::json!({
                    "direction": { "type": "string", "enum": ["up", "down", "left", "right"], "description": "Scroll direction" },
                    "amount": { "type": "integer", "minimum": 1, "maximum": 30, "description": "Scroll steps (default 3)" }
                })),
                "required": ["x", "y", "direction"]
            }),
        ),
        tool_def(
            tool_names::COMPUTER_TYPE,
            "Type text into whatever has keyboard focus on the user's screen. Never type passwords or secrets.",
            serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string", "description": "Text to type" } },
                "required": ["text"]
            }),
        ),
        tool_def(
            tool_names::COMPUTER_KEY,
            "Press a key or shortcut on the user's screen, e.g. enter, escape, ctrl+s, ctrl+shift+t, alt+tab, super.",
            serde_json::json!({
                "type": "object",
                "properties": { "keys": { "type": "string", "description": "Key or combination joined with +" } },
                "required": ["keys"]
            }),
        ),
    ]
}

pub(crate) async fn execute_computer_tool(
    name: &str,
    args: &Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    tool_call_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    if name == tool_names::COMPUTER_SCREENSHOT {
        let shot = model_sized(capture_screen().await?)?;
        LAST_SCREEN_SIZE
            .lock()
            .expect("screen size lock")
            .replace((shot.width, shot.height));
        let dir = zorai_protocol::thread_inventory_dir(agent.history.data_root(), thread_id);
        tokio::fs::create_dir_all(&dir).await?;
        let path = dir.join(format!("screen-{}.png", now_millis()));
        tokio::fs::write(&path, &shot.png).await?;
        TOOL_IMAGES
            .lock()
            .expect("tool images lock")
            .insert(tool_call_id.to_string(), path.clone());
        return Ok((
            format!(
                "Saved a {}×{} screenshot of the screen ({}) to {}. The image is attached for models that can see images; otherwise pass this path to analyze_image.",
                shot.width,
                shot.height,
                shot.backend,
                path.display()
            ),
            None,
        ));
    }
    let action = ComputerAction::from_tool(name, args)?;
    if task_id.is_some() {
        anyhow::bail!("Mouse and keyboard control needs operator approval, which is only available in a normal thread, not inside a goal task.");
    }
    if operator_holds_control(thread_id) {
        return Ok((OPERATOR_HOLD_NOTE.to_string(), None));
    }
    let rationale = "The agent wants to control your mouse or keyboard. Approve once, approve for 10 minutes in this thread, or deny.".to_string();
    if session_granted(thread_id) {
        if user_took_over().await {
            hold_computer_control(thread_id);
            emit_control(
                agent,
                thread_id,
                false,
                Some("paused because you used the mouse or keyboard"),
            );
            return Ok((OPERATOR_HOLD_NOTE.to_string(), None));
        }
        let outcome = run_action(agent, thread_id, &action).await?;
        return Ok((outcome, None));
    }
    let approval_id = format!("{APPROVAL_PREFIX}{}", uuid::Uuid::new_v4());
    PENDING_ACTIONS
        .lock()
        .expect("pending actions lock")
        .insert(approval_id.clone(), action.clone());
    let pending = ToolPendingApproval {
        approval_id,
        execution_id: format!("computer-exec-{}", uuid::Uuid::new_v4()),
        command: action.describe(),
        rationale,
        risk_level: "high".to_string(),
        blast_radius: "your desktop: mouse and keyboard input".to_string(),
        reasons: vec!["computer use input action".to_string()],
        session_id: None,
    };
    Ok((
        format!("Waiting for the operator to approve: {}", action.describe()),
        Some(pending),
    ))
}

pub(crate) fn is_computer_approval(approval_id: &str) -> bool {
    approval_id.starts_with(APPROVAL_PREFIX)
}

pub(crate) async fn resolve_computer_approval(
    approval_id: &str,
    thread_id: &str,
    decision: zorai_protocol::ApprovalDecision,
    agent: &AgentEngine,
) -> String {
    let action = PENDING_ACTIONS
        .lock()
        .expect("pending actions lock")
        .remove(approval_id);
    let Some(action) = action else {
        return "That computer action is no longer pending, so nothing was done.".to_string();
    };
    match decision {
        zorai_protocol::ApprovalDecision::Deny => format!(
            "The operator denied this computer action, so it was not performed: {}\n\nDo not retry it or work around the denial. Ask the operator how to proceed.",
            action.describe()
        ),
        zorai_protocol::ApprovalDecision::ApproveSession | zorai_protocol::ApprovalDecision::ApproveOnce => {
            if matches!(decision, zorai_protocol::ApprovalDecision::ApproveSession) {
                SESSION_GRANTS
                    .lock()
                    .expect("session grants lock")
                    .insert(thread_id.to_string(), Instant::now() + SESSION_GRANT);
            }
            match run_action(agent, thread_id, &action).await {
                Ok(outcome) => format!("The operator approved this computer action. {outcome}\nTake a new computer_screenshot to see the result."),
                Err(error) => format!("The operator approved this computer action, but it failed: {} ({error:#})", action.describe()),
            }
        }
    }
}

pub(crate) fn revoke_computer_control() {
    SESSION_GRANTS.lock().expect("session grants lock").clear();
    PENDING_ACTIONS
        .lock()
        .expect("pending actions lock")
        .clear();
}

pub(crate) fn user_input_since_last_action(idle_ms: u64, since_last_action_ms: u64) -> bool {
    idle_ms + USER_INPUT_MARGIN_MS < since_last_action_ms
}

async fn user_took_over() -> bool {
    let Some(last) = *LAST_ACTION_AT.lock().expect("last action lock") else {
        return false;
    };
    match desktop_idle_ms().await {
        Some(idle_ms) => user_input_since_last_action(idle_ms, last.elapsed().as_millis() as u64),
        None => false,
    }
}

#[cfg(target_os = "linux")]
async fn desktop_idle_ms() -> Option<u64> {
    let connection = zbus::Connection::session().await.ok()?;
    let reply = connection
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .await
        .ok()?;
    reply.body().deserialize::<u64>().ok()
}

#[cfg(not(target_os = "linux"))]
async fn desktop_idle_ms() -> Option<u64> {
    None
}

fn emit_control(agent: &AgentEngine, thread_id: &str, active: bool, reason: Option<&str>) {
    let _ = agent.event_tx.send(AgentEvent::ComputerControl {
        thread_id: thread_id.to_string(),
        active,
        reason: reason.map(str::to_string),
        return_to_app: false,
    });
}

async fn run_action(
    agent: &AgentEngine,
    thread_id: &str,
    action: &ComputerAction,
) -> Result<String> {
    ACTION_IN_PROGRESS.store(true, Ordering::SeqCst);
    let outcome = perform_action(action, &agent.data_dir).await;
    *LAST_ACTION_AT.lock().expect("last action lock") = Some(Instant::now());
    ACTION_IN_PROGRESS.store(false, Ordering::SeqCst);
    *CONTROL_THREAD.lock().expect("control thread lock") = Some(thread_id.to_string());
    match &outcome {
        Ok(_) => {
            emit_control(agent, thread_id, true, None);
            watch_for_takeover(agent.event_tx.clone());
            let event_tx = agent.event_tx.clone();
            let thread = thread_id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(IDLE_RELEASE).await;
                let idle = LAST_ACTION_AT
                    .lock()
                    .expect("last action lock")
                    .is_some_and(|last| last.elapsed() >= IDLE_RELEASE);
                if idle {
                    release_desktop().await;
                    CONTROL_THREAD.lock().expect("control thread lock").take();
                    let _ = event_tx.send(AgentEvent::ComputerControl {
                        thread_id: thread,
                        active: false,
                        reason: Some("released after 2 minutes without actions".to_string()),
                        return_to_app: false,
                    });
                }
            });
        }
        Err(_) => emit_control(agent, thread_id, false, Some("the control session ended")),
    }
    outcome
}

fn hold_computer_control(thread_id: &str) {
    SESSION_GRANTS
        .lock()
        .expect("session grants lock")
        .remove(thread_id);
    OPERATOR_HOLDS
        .lock()
        .expect("operator holds lock")
        .insert(thread_id.to_string());
}

fn operator_holds_control(thread_id: &str) -> bool {
    OPERATOR_HOLDS
        .lock()
        .expect("operator holds lock")
        .contains(thread_id)
}

pub(crate) fn resume_computer_control(thread_id: &str) {
    OPERATOR_HOLDS
        .lock()
        .expect("operator holds lock")
        .remove(thread_id);
}

fn watch_for_takeover(event_tx: tokio::sync::broadcast::Sender<AgentEvent>) {
    if TAKEOVER_WATCH.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(TAKEOVER_POLL).await;
            let Some(thread_id) = CONTROL_THREAD.lock().expect("control thread lock").clone()
            else {
                break;
            };
            if LAST_ACTION_AT.lock().expect("last action lock").is_none() {
                break;
            }
            if ACTION_IN_PROGRESS.load(Ordering::SeqCst) || !user_took_over().await {
                continue;
            }
            if ACTION_IN_PROGRESS.load(Ordering::SeqCst) {
                continue;
            }
            hold_computer_control(&thread_id);
            CONTROL_THREAD.lock().expect("control thread lock").take();
            release_desktop().await;
            let _ = event_tx.send(AgentEvent::ComputerControl {
                thread_id,
                active: false,
                reason: Some("paused because you used the mouse or keyboard".to_string()),
                return_to_app: false,
            });
            break;
        }
        TAKEOVER_WATCH.store(false, Ordering::SeqCst);
    });
}

async fn release_desktop() {
    *LAST_ACTION_AT.lock().expect("last action lock") = None;
    #[cfg(target_os = "linux")]
    wayland_input::release().await;
}

pub(crate) async fn stop_computer_control(
    event_tx: &tokio::sync::broadcast::Sender<AgentEvent>,
    reason: &str,
) {
    revoke_computer_control();
    release_desktop().await;
    if let Some(thread_id) = CONTROL_THREAD.lock().expect("control thread lock").take() {
        OPERATOR_HOLDS
            .lock()
            .expect("operator holds lock")
            .insert(thread_id.clone());
        let _ = event_tx.send(AgentEvent::ComputerControl {
            thread_id,
            active: false,
            reason: Some(reason.to_string()),
            return_to_app: false,
        });
    }
}

pub(crate) async fn finish_computer_control_turn(
    event_tx: &tokio::sync::broadcast::Sender<AgentEvent>,
    thread_id: &str,
) {
    let controls_thread = CONTROL_THREAD
        .lock()
        .expect("control thread lock")
        .as_deref()
        == Some(thread_id);
    if !controls_thread {
        return;
    }
    release_desktop().await;
    CONTROL_THREAD.lock().expect("control thread lock").take();
    let _ = event_tx.send(AgentEvent::ComputerControl {
        thread_id: thread_id.to_string(),
        active: false,
        reason: None,
        return_to_app: true,
    });
}

static LAST_SCREEN_SIZE: LazyLock<std::sync::Mutex<Option<(u32, u32)>>> =
    LazyLock::new(Default::default);
static PENDING_ACTIONS: LazyLock<std::sync::Mutex<HashMap<String, ComputerAction>>> =
    LazyLock::new(Default::default);
static SESSION_GRANTS: LazyLock<std::sync::Mutex<HashMap<String, Instant>>> =
    LazyLock::new(Default::default);
static TOOL_IMAGES: LazyLock<std::sync::Mutex<HashMap<String, std::path::PathBuf>>> =
    LazyLock::new(Default::default);

pub(crate) fn take_tool_image_blocks(
    tool_call_id: &str,
    text: &str,
    provider_id: &str,
    model: &str,
) -> Vec<AgentContentBlock> {
    let path = TOOL_IMAGES
        .lock()
        .expect("tool images lock")
        .remove(tool_call_id);
    let Some(path) = path else {
        return Vec::new();
    };
    if !crate::agent::types::model_supports(
        provider_id,
        model,
        crate::agent::types::Modality::Image,
    ) {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    use base64::Engine as _;
    vec![
        AgentContentBlock::Text {
            text: text.to_string(),
        },
        AgentContentBlock::Image {
            url: None,
            data_url: Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )),
            mime_type: Some("image/png".to_string()),
            inventory_path: Some(path.display().to_string()),
        },
    ]
}

static LAST_ACTION_AT: LazyLock<std::sync::Mutex<Option<Instant>>> =
    LazyLock::new(Default::default);
static OPERATOR_HOLDS: LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);
static ACTION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static TAKEOVER_WATCH: AtomicBool = AtomicBool::new(false);
static CONTROL_THREAD: LazyLock<std::sync::Mutex<Option<String>>> = LazyLock::new(Default::default);

fn session_granted(thread_id: &str) -> bool {
    let mut grants = SESSION_GRANTS.lock().expect("session grants lock");
    grants.retain(|_, until| *until > Instant::now());
    grants.contains_key(thread_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    fn linux_code(self) -> i32 {
        match self {
            MouseButton::Left => 0x110,
            MouseButton::Right => 0x111,
            MouseButton::Middle => 0x112,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComputerAction {
    Click {
        x: i64,
        y: i64,
        button: MouseButton,
        clicks: u8,
    },
    Move {
        x: i64,
        y: i64,
    },
    Drag {
        x: i64,
        y: i64,
        to_x: i64,
        to_y: i64,
    },
    Scroll {
        x: i64,
        y: i64,
        direction: String,
        amount: i32,
    },
    Type {
        text: String,
    },
    Key {
        keys: String,
    },
}

impl ComputerAction {
    pub(crate) fn from_tool(name: &str, args: &Value) -> Result<Self> {
        let int = |key: &str| {
            args.get(key)
                .and_then(Value::as_i64)
                .ok_or_else(|| anyhow::anyhow!("missing integer argument '{key}'"))
        };
        let text = |key: &str| {
            args.get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("missing argument '{key}'"))
        };
        Ok(match name {
            tool_names::COMPUTER_CLICK => ComputerAction::Click {
                x: int("x")?,
                y: int("y")?,
                button: match args.get("button").and_then(Value::as_str).unwrap_or("left") {
                    "left" => MouseButton::Left,
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    other => anyhow::bail!("unknown mouse button '{other}'"),
                },
                clicks: args
                    .get("clicks")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .clamp(1, 3) as u8,
            },
            tool_names::COMPUTER_MOVE => ComputerAction::Move {
                x: int("x")?,
                y: int("y")?,
            },
            tool_names::COMPUTER_DRAG => ComputerAction::Drag {
                x: int("x")?,
                y: int("y")?,
                to_x: int("to_x")?,
                to_y: int("to_y")?,
            },
            tool_names::COMPUTER_SCROLL => {
                let direction = text("direction")?;
                if !["up", "down", "left", "right"].contains(&direction.as_str()) {
                    anyhow::bail!("unknown scroll direction '{direction}'");
                }
                ComputerAction::Scroll {
                    x: int("x")?,
                    y: int("y")?,
                    direction,
                    amount: args
                        .get("amount")
                        .and_then(Value::as_i64)
                        .unwrap_or(3)
                        .clamp(1, 30) as i32,
                }
            }
            tool_names::COMPUTER_TYPE => ComputerAction::Type {
                text: text("text")?,
            },
            tool_names::COMPUTER_KEY => {
                let keys = text("keys")?;
                parse_key_combo(&keys)?;
                ComputerAction::Key { keys }
            }
            other => anyhow::bail!("{other} is not a computer tool"),
        })
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            ComputerAction::Click {
                x,
                y,
                button,
                clicks,
            } => {
                let kind = match clicks {
                    2 => "double-click",
                    3 => "triple-click",
                    _ => "click",
                };
                let button = match button {
                    MouseButton::Left => "",
                    MouseButton::Right => "right ",
                    MouseButton::Middle => "middle ",
                };
                format!("{button}{kind} at ({x}, {y})")
            }
            ComputerAction::Move { x, y } => format!("move the pointer to ({x}, {y})"),
            ComputerAction::Drag { x, y, to_x, to_y } => {
                format!("drag from ({x}, {y}) to ({to_x}, {to_y})")
            }
            ComputerAction::Scroll {
                x,
                y,
                direction,
                amount,
            } => format!("scroll {direction} {amount} steps at ({x}, {y})"),
            ComputerAction::Type { text } => {
                format!("type \"{}\"", crate::agent::summarize_text(text, 120))
            }
            ComputerAction::Key { keys } => format!("press {keys}"),
        }
    }
}

pub(crate) fn char_keysym(ch: char) -> i32 {
    match ch {
        '\n' => 0xff0d,
        '\t' => 0xff09,
        c if (c as u32) >= 0x20 && (c as u32) <= 0x7e => c as i32,
        c if (c as u32) >= 0xa0 && (c as u32) <= 0xff => c as i32,
        c => 0x0100_0000 | c as i32,
    }
}

pub(crate) fn parse_key_combo(keys: &str) -> Result<Vec<i32>> {
    let parts = keys
        .split('+')
        .map(|part| part.trim().to_lowercase())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        anyhow::bail!("no key given");
    }
    parts
        .iter()
        .map(|part| {
            let keysym = match part.as_str() {
                "ctrl" | "control" => 0xffe3,
                "shift" => 0xffe1,
                "alt" | "option" => 0xffe9,
                "super" | "meta" | "win" | "cmd" | "command" => 0xffeb,
                "enter" | "return" => 0xff0d,
                "tab" => 0xff09,
                "esc" | "escape" => 0xff1b,
                "backspace" => 0xff08,
                "delete" | "del" => 0xffff,
                "space" => 0x20,
                "home" => 0xff50,
                "end" => 0xff57,
                "pageup" | "page_up" => 0xff55,
                "pagedown" | "page_down" => 0xff56,
                "left" => 0xff51,
                "up" => 0xff52,
                "right" => 0xff53,
                "down" => 0xff54,
                "insert" => 0xff63,
                function
                    if function.len() > 1
                        && function.starts_with('f')
                        && function[1..]
                            .parse::<i32>()
                            .is_ok_and(|n| (1..=24).contains(&n)) =>
                {
                    0xffbe + function[1..].parse::<i32>().unwrap() - 1
                }
                single if single.chars().count() == 1 => {
                    char_keysym(single.chars().next().unwrap())
                }
                other => anyhow::bail!("unknown key '{other}'"),
            };
            Ok(keysym)
        })
        .collect()
}

pub(crate) fn missing_input_devices(pointer: bool, keyboard: bool) -> Option<String> {
    let missing = match (pointer, keyboard) {
        (true, true) => return None,
        (false, false) => "mouse and keyboard control",
        (false, true) => "mouse control",
        (true, false) => "keyboard control",
    };
    Some(format!(
        "The desktop shared the screen but did not grant {missing}. The saved permission was cleared, so the next action asks again: in the system dialog, turn on \"Allow remote interaction\" before sharing."
    ))
}

pub(crate) fn scale_to_stream(
    x: i64,
    y: i64,
    screen: (u32, u32),
    stream: (i32, i32),
) -> Result<(f64, f64)> {
    if x < 0 || y < 0 || x >= screen.0 as i64 || y >= screen.1 as i64 {
        anyhow::bail!("({x}, {y}) is outside the {}×{} screen", screen.0, screen.1);
    }
    Ok((
        x as f64 * stream.0 as f64 / screen.0 as f64,
        y as f64 * stream.1 as f64 / screen.1 as f64,
    ))
}

#[cfg(target_os = "linux")]
async fn perform_action(action: &ComputerAction, data_dir: &std::path::Path) -> Result<String> {
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let display = std::env::var("DISPLAY").ok();
    match detect_display_session(wayland.as_deref(), display.as_deref()) {
        Some(DisplaySession::Wayland) => wayland_input::perform(action, data_dir).await,
        Some(DisplaySession::X11) => {
            anyhow::bail!("Mouse and keyboard control is not supported on X11 sessions yet.")
        }
        None => anyhow::bail!("No graphical session found, so there is nothing to control."),
    }
}

#[cfg(not(target_os = "linux"))]
async fn perform_action(_action: &ComputerAction, _data_dir: &std::path::Path) -> Result<String> {
    anyhow::bail!("Mouse and keyboard control is not available on this system yet.")
}

#[cfg(target_os = "linux")]
mod wayland_input {
    use super::*;
    use ashpd::desktop::remote_desktop::{Axis, DeviceType, KeyState, RemoteDesktop};
    use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
    use ashpd::desktop::{PersistMode, Session};

    struct Control {
        proxy: RemoteDesktop<'static>,
        session: Session<'static, RemoteDesktop<'static>>,
        stream_id: u32,
        stream_size: (i32, i32),
    }

    static CONTROL: LazyLock<tokio::sync::Mutex<Option<Control>>> = LazyLock::new(Default::default);
    const TOKEN_FILE: &str = "computer-use-restore-token";

    async fn start(data_dir: &std::path::Path) -> Result<Control> {
        let token_path = data_dir.join(TOKEN_FILE);
        let token = tokio::fs::read_to_string(&token_path)
            .await
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let proxy = RemoteDesktop::new()
            .await
            .context("the desktop remote-control portal is unavailable")?;
        let screencast = Screencast::new()
            .await
            .context("the desktop screen-cast portal is unavailable")?;
        let session = proxy.create_session().await?;
        proxy
            .select_devices(
                &session,
                DeviceType::Keyboard | DeviceType::Pointer,
                token.as_deref(),
                PersistMode::ExplicitlyRevoked,
            )
            .await?;
        screencast
            .select_sources(
                &session,
                CursorMode::Hidden,
                SourceType::Monitor.into(),
                false,
                None,
                PersistMode::DoNot,
            )
            .await?;
        let response = proxy
            .start(&session, None)
            .await?
            .response()
            .context("remote control was refused or cancelled in the system dialog")?;
        let devices = response.devices();
        tracing::info!(
            pointer = devices.contains(DeviceType::Pointer),
            keyboard = devices.contains(DeviceType::Keyboard),
            restored = token.is_some(),
            "computer use remote-control devices granted"
        );
        if let Some(problem) = missing_input_devices(
            devices.contains(DeviceType::Pointer),
            devices.contains(DeviceType::Keyboard),
        ) {
            let _ = tokio::fs::remove_file(&token_path).await;
            let _ = session.close().await;
            anyhow::bail!(problem);
        }
        if let Some(new_token) = response.restore_token() {
            let _ = tokio::fs::write(&token_path, new_token).await;
        }
        let stream = response
            .streams()
            .and_then(|streams| streams.first())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "the system did not share a screen, so pointer positions cannot be mapped"
                )
            })?;
        let stream_size = stream
            .size()
            .ok_or_else(|| anyhow::anyhow!("the shared screen has no size"))?;
        Ok(Control {
            stream_id: stream.pipe_wire_node_id(),
            stream_size,
            proxy,
            session,
        })
    }

    async fn pointer(control: &Control, x: i64, y: i64) -> Result<()> {
        let screen = LAST_SCREEN_SIZE
            .lock()
            .expect("screen size lock")
            .ok_or_else(|| {
                anyhow::anyhow!("take a computer_screenshot first so coordinates match the screen")
            })?;
        let (sx, sy) = scale_to_stream(x, y, screen, control.stream_size)?;
        control
            .proxy
            .notify_pointer_motion_absolute(&control.session, control.stream_id, sx, sy)
            .await?;
        Ok(())
    }

    async fn button(control: &Control, code: i32, state: KeyState) -> Result<()> {
        control
            .proxy
            .notify_pointer_button(&control.session, code, state)
            .await?;
        Ok(())
    }

    async fn keysym(control: &Control, keysym: i32, state: KeyState) -> Result<()> {
        control
            .proxy
            .notify_keyboard_keysym(&control.session, keysym, state)
            .await?;
        Ok(())
    }

    async fn run(control: &Control, action: &ComputerAction) -> Result<()> {
        match action {
            ComputerAction::Click {
                x,
                y,
                button: which,
                clicks,
            } => {
                pointer(control, *x, *y).await?;
                for _ in 0..*clicks {
                    button(control, which.linux_code(), KeyState::Pressed).await?;
                    button(control, which.linux_code(), KeyState::Released).await?;
                }
            }
            ComputerAction::Move { x, y } => pointer(control, *x, *y).await?,
            ComputerAction::Drag { x, y, to_x, to_y } => {
                pointer(control, *x, *y).await?;
                button(control, MouseButton::Left.linux_code(), KeyState::Pressed).await?;
                tokio::time::sleep(Duration::from_millis(60)).await;
                pointer(control, *to_x, *to_y).await?;
                tokio::time::sleep(Duration::from_millis(60)).await;
                button(control, MouseButton::Left.linux_code(), KeyState::Released).await?;
            }
            ComputerAction::Scroll {
                x,
                y,
                direction,
                amount,
            } => {
                pointer(control, *x, *y).await?;
                let (axis, steps) = match direction.as_str() {
                    "up" => (Axis::Vertical, -amount),
                    "down" => (Axis::Vertical, *amount),
                    "left" => (Axis::Horizontal, -amount),
                    _ => (Axis::Horizontal, *amount),
                };
                control
                    .proxy
                    .notify_pointer_axis_discrete(&control.session, axis, steps)
                    .await?;
            }
            ComputerAction::Type { text } => {
                for ch in text.chars() {
                    let sym = char_keysym(ch);
                    keysym(control, sym, KeyState::Pressed).await?;
                    keysym(control, sym, KeyState::Released).await?;
                }
            }
            ComputerAction::Key { keys } => {
                let syms = parse_key_combo(keys)?;
                for sym in &syms {
                    keysym(control, *sym, KeyState::Pressed).await?;
                }
                for sym in syms.iter().rev() {
                    keysym(control, *sym, KeyState::Released).await?;
                }
            }
        }
        Ok(())
    }

    pub(super) async fn perform(
        action: &ComputerAction,
        data_dir: &std::path::Path,
    ) -> Result<String> {
        let mut guard = CONTROL.lock().await;
        if guard.is_none() {
            match start(data_dir).await {
                Ok(control) => {
                    tracing::info!(
                        stream_id = control.stream_id,
                        stream_width = control.stream_size.0,
                        stream_height = control.stream_size.1,
                        "computer use remote-control session started"
                    );
                    *guard = Some(control);
                }
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "computer use remote-control session failed to start");
                    return Err(error);
                }
            }
        }
        let control = guard.as_ref().expect("control session");
        if let Err(error) = run(control, action).await {
            tracing::warn!(error = %format!("{error:#}"), action = %action.describe(), "computer use input action failed");
            *guard = None;
            return Err(error.context(
                "the remote-control session ended; the next action will ask for access again",
            ));
        }
        Ok(format!("Done: {}.", action.describe()))
    }

    pub(super) async fn release() {
        if let Some(control) = CONTROL.lock().await.take() {
            let _ = control.session.close().await;
        }
    }
}

pub(crate) struct ScreenCapture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub backend: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplaySession {
    Wayland,
    X11,
}

pub(crate) fn detect_display_session(
    wayland_display: Option<&str>,
    x_display: Option<&str>,
) -> Option<DisplaySession> {
    let present = |value: Option<&str>| value.is_some_and(|value| !value.trim().is_empty());
    if present(wayland_display) {
        Some(DisplaySession::Wayland)
    } else if present(x_display) {
        Some(DisplaySession::X11)
    } else {
        None
    }
}

pub(crate) fn bgrx_to_rgb(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let pixels = width as usize * height as usize;
    if data.len() < pixels * 4 {
        anyhow::bail!(
            "the X server returned {} bytes for a {width}×{height} screen; expected 32-bit pixels",
            data.len()
        );
    }
    let mut rgb = Vec::with_capacity(pixels * 3);
    for pixel in data[..pixels * 4].chunks_exact(4) {
        rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
    }
    Ok(rgb)
}

pub(crate) fn encode_rgb_png(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgb)?;
    writer.finish()?;
    Ok(out)
}

const MODEL_MAX_WIDTH: u32 = 1920;
const MODEL_MAX_HEIGHT: u32 = 1200;

pub(crate) fn model_image_size(width: u32, height: u32) -> (u32, u32) {
    let scale = (MODEL_MAX_WIDTH as f64 / width as f64)
        .min(MODEL_MAX_HEIGHT as f64 / height as f64)
        .min(1.0);
    (
        ((width as f64 * scale).round() as u32).max(1),
        ((height as f64 * scale).round() as u32).max(1),
    )
}

pub(crate) fn model_sized(shot: ScreenCapture) -> Result<ScreenCapture> {
    let (width, height) = model_image_size(shot.width, shot.height);
    if (width, height) == (shot.width, shot.height) {
        return Ok(shot);
    }
    let decoded = image::load_from_memory_with_format(&shot.png, image::ImageFormat::Png)
        .context("could not decode the screenshot for resizing")?
        .to_rgb8();
    let resized = image::imageops::resize(
        &decoded,
        width,
        height,
        image::imageops::FilterType::CatmullRom,
    );
    Ok(ScreenCapture {
        png: encode_rgb_png(resized.as_raw(), width, height)?,
        width,
        height,
        backend: shot.backend,
    })
}

pub(crate) fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info()?;
    let info = reader.info();
    Ok((info.width, info.height))
}

#[cfg(target_os = "linux")]
async fn capture_screen() -> Result<ScreenCapture> {
    let wayland = std::env::var("WAYLAND_DISPLAY").ok();
    let display = std::env::var("DISPLAY").ok();
    match detect_display_session(wayland.as_deref(), display.as_deref()) {
        Some(DisplaySession::Wayland) => capture_with_portal().await,
        Some(DisplaySession::X11) => tokio::task::spawn_blocking(capture_with_x11).await?,
        None => anyhow::bail!(
            "No graphical session found (neither WAYLAND_DISPLAY nor DISPLAY is set for the daemon), so there is no screen to capture."
        ),
    }
}

#[cfg(target_os = "linux")]
async fn capture_with_portal() -> Result<ScreenCapture> {
    let response = ashpd::desktop::screenshot::Screenshot::request()
        .interactive(false)
        .modal(false)
        .send()
        .await
        .context("the desktop screenshot portal is unavailable")?
        .response()
        .context("the screenshot request was refused or cancelled")?;
    let file = response
        .uri()
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("the screenshot portal returned a non-file location"))?;
    let png = tokio::fs::read(&file)
        .await
        .with_context(|| format!("could not read the portal screenshot at {}", file.display()))?;
    let _ = tokio::fs::remove_file(&file).await;
    let (width, height) = png_dimensions(&png)?;
    Ok(ScreenCapture {
        png,
        width,
        height,
        backend: "Wayland screenshot portal",
    })
}

#[cfg(target_os = "linux")]
fn capture_with_x11() -> Result<ScreenCapture> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
    let (conn, screen_num) = x11rb::connect(None).context("could not connect to the X server")?;
    let screen = &conn.setup().roots[screen_num];
    let (width, height) = (screen.width_in_pixels, screen.height_in_pixels);
    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, screen.root, 0, 0, width, height, !0)?
        .reply()
        .context("the X server refused to return the screen image")?;
    let rgb = bgrx_to_rgb(&reply.data, width.into(), height.into())?;
    Ok(ScreenCapture {
        png: encode_rgb_png(&rgb, width.into(), height.into())?,
        width: width.into(),
        height: height.into(),
        backend: "X11",
    })
}

#[cfg(not(target_os = "linux"))]
async fn capture_screen() -> Result<ScreenCapture> {
    anyhow::bail!("Screen capture for computer use is not available on this system yet.")
}

#[cfg(test)]
mod tests {
    use super::*;

    static CONTROL_STATE_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn after_the_operator_stops_control_the_agent_stays_off_the_desktop_until_they_write_again(
    ) {
        let _serial = CONTROL_STATE_TESTS.lock().await;
        let (event_tx, _events) = tokio::sync::broadcast::channel(8);
        SESSION_GRANTS
            .lock()
            .expect("session grants lock")
            .insert("held".to_string(), Instant::now() + SESSION_GRANT);
        *CONTROL_THREAD.lock().expect("control thread lock") = Some("held".to_string());

        stop_computer_control(&event_tx, "stopped from Zorai").await;
        assert!(operator_holds_control("held"));
        assert!(!session_granted("held"));
        assert!(!operator_holds_control("other"));

        resume_computer_control("held");
        assert!(!operator_holds_control("held"));
    }

    #[tokio::test]
    async fn finishing_the_controlling_turn_hands_the_desktop_back_and_returns_to_zorai() {
        let _serial = CONTROL_STATE_TESTS.lock().await;
        let (event_tx, mut events) = tokio::sync::broadcast::channel(8);
        *CONTROL_THREAD.lock().expect("control thread lock") = Some("controller".to_string());

        finish_computer_control_turn(&event_tx, "other-thread").await;
        assert!(events.try_recv().is_err());
        assert_eq!(
            CONTROL_THREAD
                .lock()
                .expect("control thread lock")
                .as_deref(),
            Some("controller")
        );

        finish_computer_control_turn(&event_tx, "controller").await;
        match events.try_recv().expect("control end event") {
            AgentEvent::ComputerControl {
                thread_id,
                active,
                return_to_app,
                ..
            } => {
                assert_eq!(thread_id, "controller");
                assert!(!active);
                assert!(return_to_app);
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(CONTROL_THREAD
            .lock()
            .expect("control thread lock")
            .is_none());
    }

    #[test]
    fn wayland_wins_over_xwayland_so_gnome_uses_the_portal_instead_of_a_black_x11_capture() {
        assert_eq!(
            detect_display_session(Some("wayland-0"), Some(":0")),
            Some(DisplaySession::Wayland)
        );
        assert_eq!(
            detect_display_session(None, Some(":0")),
            Some(DisplaySession::X11)
        );
        assert_eq!(detect_display_session(Some(" "), Some("")), None);
        assert_eq!(detect_display_session(None, None), None);
    }

    #[test]
    fn x11_pixels_are_converted_to_rgb_and_round_trip_through_png() {
        let bgrx = [10, 20, 30, 0, 40, 50, 60, 255];
        let rgb = bgrx_to_rgb(&bgrx, 2, 1).unwrap();
        assert_eq!(rgb, vec![30, 20, 10, 60, 50, 40]);
        let png = encode_rgb_png(&rgb, 2, 1).unwrap();
        assert_eq!(png_dimensions(&png).unwrap(), (2, 1));
    }

    #[test]
    fn short_x11_buffers_are_rejected_instead_of_producing_a_garbled_image() {
        assert!(bgrx_to_rgb(&[0; 7], 2, 1).is_err());
    }

    #[test]
    fn computer_tools_are_only_offered_when_computer_use_is_enabled() {
        let mut config = AgentConfig::default();
        assert!(!config.tools.computer_use);
        let root = tempfile::tempdir().unwrap();
        let names = |config: &AgentConfig| {
            crate::agent::tool_executor::get_available_tools(config, root.path(), false)
                .into_iter()
                .map(|tool| tool.function.name)
                .collect::<Vec<_>>()
        };
        assert!(!names(&config).contains(&tool_names::COMPUTER_SCREENSHOT.to_string()));
        config.tools.computer_use = true;
        assert!(names(&config).contains(&tool_names::COMPUTER_SCREENSHOT.to_string()));
    }

    #[test]
    fn key_combos_map_to_keysyms_in_press_order() {
        assert_eq!(
            parse_key_combo("ctrl+shift+t").unwrap(),
            vec![0xffe3, 0xffe1, 't' as i32]
        );
        assert_eq!(parse_key_combo("Enter").unwrap(), vec![0xff0d]);
        assert_eq!(parse_key_combo("alt+F4").unwrap(), vec![0xffe9, 0xffc1]);
        assert!(parse_key_combo("ctrl+bogus").is_err());
        assert!(parse_key_combo(" + ").is_err());
    }

    #[test]
    fn typed_characters_use_latin1_or_unicode_keysyms() {
        assert_eq!(char_keysym('a'), 'a' as i32);
        assert_eq!(char_keysym('é'), 0xe9);
        assert_eq!(char_keysym('ł'), 0x0100_0142);
        assert_eq!(char_keysym('\n'), 0xff0d);
    }

    #[test]
    fn screenshot_pixels_map_onto_the_logical_stream_so_hidpi_clicks_land_where_the_agent_looked() {
        assert_eq!(
            scale_to_stream(1920, 1080, (3840, 2160), (1920, 1080)).unwrap(),
            (960.0, 540.0)
        );
        assert!(scale_to_stream(4000, 10, (3840, 2160), (1920, 1080)).is_err());
        assert!(scale_to_stream(-1, 10, (3840, 2160), (1920, 1080)).is_err());
    }

    #[test]
    fn actions_are_parsed_and_described_for_the_approval_prompt() {
        let click = ComputerAction::from_tool(
            tool_names::COMPUTER_CLICK,
            &serde_json::json!({ "x": 10, "y": 20, "clicks": 2 }),
        )
        .unwrap();
        assert_eq!(click.describe(), "double-click at (10, 20)");
        let key = ComputerAction::from_tool(
            tool_names::COMPUTER_KEY,
            &serde_json::json!({ "keys": "ctrl+s" }),
        )
        .unwrap();
        assert_eq!(key.describe(), "press ctrl+s");
        assert!(ComputerAction::from_tool(
            tool_names::COMPUTER_KEY,
            &serde_json::json!({ "keys": "ctrl+nope" })
        )
        .is_err());
        assert!(ComputerAction::from_tool(
            tool_names::COMPUTER_CLICK,
            &serde_json::json!({ "x": 1 })
        )
        .is_err());
    }

    #[tokio::test]
    async fn input_actions_wait_for_approval_and_a_denial_never_touches_the_desktop() {
        let root = tempfile::tempdir().unwrap();
        let engine = AgentEngine::new_test(
            crate::session_manager::SessionManager::new_test(root.path()).await,
            AgentConfig::default(),
            root.path(),
        )
        .await;
        let (message, pending) = execute_computer_tool(
            tool_names::COMPUTER_CLICK,
            &serde_json::json!({ "x": 5, "y": 5 }),
            &engine,
            "thread_test",
            None,
            "call_1",
        )
        .await
        .unwrap();
        let pending = pending.expect("clicks must wait for the operator");
        assert!(message.contains("Waiting for the operator"));
        assert!(is_computer_approval(&pending.approval_id));
        let notice = resolve_computer_approval(
            &pending.approval_id,
            "thread_test",
            zorai_protocol::ApprovalDecision::Deny,
            &engine,
        )
        .await;
        assert!(notice.contains("denied"));
        let again = resolve_computer_approval(
            &pending.approval_id,
            "thread_test",
            zorai_protocol::ApprovalDecision::ApproveOnce,
            &engine,
        )
        .await;
        assert!(again.contains("no longer pending"));
    }

    #[tokio::test]
    async fn computer_input_is_refused_inside_goal_tasks_where_it_cannot_be_approved() {
        let root = tempfile::tempdir().unwrap();
        let engine = AgentEngine::new_test(
            crate::session_manager::SessionManager::new_test(root.path()).await,
            AgentConfig::default(),
            root.path(),
        )
        .await;
        let result = execute_computer_tool(
            tool_names::COMPUTER_TYPE,
            &serde_json::json!({ "text": "hi" }),
            &engine,
            "thread_test",
            Some("task_1"),
            "call_2",
        )
        .await;
        assert!(result.is_err());
    }

    #[test]
    fn input_newer_than_the_agents_last_action_means_the_user_took_over() {
        assert!(user_input_since_last_action(100, 5_000));
        assert!(!user_input_since_last_action(4_900, 5_000));
        assert!(!user_input_since_last_action(5_200, 5_000));
    }

    fn screenshot_turn() -> Vec<crate::agent::llm_client::ApiMessage> {
        use crate::agent::llm_client::{ApiContent, ApiMessage, ApiToolCall, ApiToolCallFunction};
        vec![
            ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("look at my screen".to_string()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: None,
            },
            ApiMessage {
                role: "assistant".to_string(),
                content: ApiContent::Text(String::new()),
                reasoning: None,
                tool_call_id: None,
                name: None,
                tool_calls: Some(vec![ApiToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ApiToolCallFunction {
                        name: "computer_screenshot".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
            },
            tool_image_message(),
        ]
    }

    fn tool_image_message() -> crate::agent::llm_client::ApiMessage {
        crate::agent::llm_client::ApiMessage {
            role: "tool".to_string(),
            content: crate::agent::llm_client::ApiContent::Blocks(vec![
                serde_json::json!({ "type": "input_text", "text": "Saved a screenshot" }),
                serde_json::json!({ "type": "input_image", "image_url": "data:image/png;base64,iVBORw0K" }),
            ]),
            reasoning: None,
            tool_call_id: Some("call_1".to_string()),
            name: Some("computer_screenshot".to_string()),
            tool_calls: None,
        }
    }

    #[test]
    fn vision_models_get_the_screenshot_itself_and_text_only_models_get_just_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.png");
        std::fs::write(&path, encode_rgb_png(&[1, 2, 3], 1, 1).unwrap()).unwrap();
        TOOL_IMAGES
            .lock()
            .unwrap()
            .insert("call_vision".to_string(), path.clone());
        let blocks = take_tool_image_blocks("call_vision", "Saved", "openai", "gpt-6.1-sol");
        assert!(
            matches!(&blocks[1], AgentContentBlock::Image { data_url: Some(url), .. } if url.starts_with("data:image/png;base64,"))
        );
        TOOL_IMAGES
            .lock()
            .unwrap()
            .insert("call_text".to_string(), path);
        assert!(
            take_tool_image_blocks("call_text", "Saved", "openai", "unknown-text-model").is_empty()
        );
        assert!(
            take_tool_image_blocks("call_missing", "Saved", "openai", "gpt-6.1-sol").is_empty()
        );
    }

    #[test]
    fn chat_completions_keeps_tool_text_and_moves_the_image_into_a_following_user_message() {
        let out = crate::agent::llm_client::build_chat_completion_messages_with_options(
            "sys",
            &screenshot_turn(),
            false,
            false,
            false,
            false,
        )
        .unwrap();
        let tool = out
            .iter()
            .find(|message| message["role"] == "tool")
            .unwrap();
        assert_eq!(tool["content"], "Saved a screenshot");
        let user = out.last().unwrap();
        assert_eq!(user["role"], "user");
        assert_eq!(user["content"][1]["type"], "image_url");
    }

    #[test]
    fn anthropic_tool_results_carry_the_image_in_anthropic_format() {
        let out = crate::agent::llm_client::build_anthropic_messages(&screenshot_turn());
        let content = &out.last().unwrap()["content"][0]["content"];
        assert!(content
            .as_array()
            .unwrap()
            .iter()
            .any(|block| block["type"] == "image"));
    }

    #[test]
    fn screenshots_shrink_to_what_vision_models_actually_use_without_distortion() {
        assert_eq!(model_image_size(3440, 1440), (1920, 804));
        assert_eq!(model_image_size(3840, 2160), (1920, 1080));
        assert_eq!(model_image_size(1600, 2560), (750, 1200));
        assert_eq!(model_image_size(1280, 720), (1280, 720));
    }

    #[test]
    fn downscaled_screenshots_are_valid_pngs_at_the_reported_size() {
        let rgb = vec![200u8; 3440 * 1440 * 3];
        let shot = ScreenCapture {
            png: encode_rgb_png(&rgb, 3440, 1440).unwrap(),
            width: 3440,
            height: 1440,
            backend: "test",
        };
        let small = model_sized(shot).unwrap();
        assert_eq!((small.width, small.height), (1920, 804));
        assert_eq!(png_dimensions(&small.png).unwrap(), (1920, 804));
    }

    #[test]
    fn a_screen_only_grant_is_rejected_with_a_fix_the_user_can_act_on() {
        assert_eq!(missing_input_devices(true, true), None);
        assert!(missing_input_devices(false, false)
            .unwrap()
            .contains("mouse and keyboard control"));
        assert!(missing_input_devices(false, true)
            .unwrap()
            .contains("Allow remote interaction"));
        assert!(missing_input_devices(true, false)
            .unwrap()
            .contains("keyboard control"));
    }
}
