use super::*;
use serde::Serialize;
use serde_json::Value;
use zorai_protocol::tool_names;

mod cdp;
mod gui;
mod scripts;
#[cfg(test)]
#[path = "browser/tests.rs"]
mod tests;

const DEFAULT_READ_MAX_CHARS: usize = 6_000;
const MAX_READ_MAX_CHARS: usize = 40_000;
const EVAL_RESULT_MAX_CHARS: usize = 8_000;

#[derive(Default)]
pub(crate) struct BrowserRuntime {
    gui_pending:
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<std::result::Result<Value, String>>>>,
    cdp: Mutex<Option<cdp::CdpBrowser>>,
    cdp_active_tabs: Mutex<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BrowserOp {
    Navigate {
        url: String,
    },
    Evaluate {
        script: String,
    },
    Screenshot,
    History {
        direction: String,
    },
    Tabs {
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tab_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserBackend {
    Gui,
    Cdp,
}

pub(crate) fn browser_backend_for_surface(
    surface: Option<zorai_protocol::ClientSurface>,
) -> BrowserBackend {
    match surface {
        Some(zorai_protocol::ClientSurface::Electron) => BrowserBackend::Gui,
        _ => BrowserBackend::Cdp,
    }
}

async fn run_browser_op(
    agent: &AgentEngine,
    thread_id: &str,
    backend: BrowserBackend,
    op: &BrowserOp,
) -> Result<Value> {
    match backend {
        BrowserBackend::Gui => gui::gui_browser_request(agent, thread_id, op).await,
        BrowserBackend::Cdp => cdp::cdp_browser_request(agent, thread_id, op).await,
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn browser_navigation_url(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    let local = ["localhost", "127.0.0.1", "[::1]", "0.0.0.0"]
        .iter()
        .any(|host| {
            lower == *host
                || lower.starts_with(&format!("{host}:"))
                || lower.starts_with(&format!("{host}/"))
        });
    if local {
        format!("http://{url}")
    } else if url.contains("://") || lower.starts_with("about:") || lower.starts_with("data:") {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

pub(crate) fn browser_target(args: &Value) -> Option<Value> {
    if let Some(reference) = args.get("ref").filter(|value| !value.is_null()) {
        let reference = match reference {
            Value::Number(number) => number.to_string(),
            other => other.as_str().unwrap_or_default().trim().to_string(),
        };
        if !reference.is_empty() {
            return Some(serde_json::json!({ "ref": reference }));
        }
    }
    if let Some(selector) = arg_str(args, "selector") {
        return Some(serde_json::json!({ "selector": selector }));
    }
    arg_str(args, "text").map(|text| serde_json::json!({ "text": text }))
}

fn parse_script_json(value: &Value) -> Result<Value> {
    let raw = value.get("value").unwrap_or(value);
    let parsed = match raw {
        Value::String(text) => serde_json::from_str::<Value>(text)
            .map_err(|error| anyhow::anyhow!("page script returned invalid JSON: {error}"))?,
        other => other.clone(),
    };
    if parsed.get("ok").and_then(Value::as_bool) == Some(false) {
        let message = parsed
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("the page action failed");
        anyhow::bail!("{message}");
    }
    Ok(parsed)
}

fn page_line(info: &Value) -> String {
    let title = info
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("(untitled)");
    let url = info
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("about:blank");
    format!("{title} — {url}")
}

pub(crate) fn format_browser_elements(elements: &[Value]) -> String {
    elements
        .iter()
        .map(|element| {
            let reference = element.get("ref").and_then(Value::as_str).unwrap_or("?");
            let kind = element
                .get("role")
                .and_then(Value::as_str)
                .or_else(|| element.get("type").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    match element
                        .get("tag")
                        .and_then(Value::as_str)
                        .unwrap_or("element")
                    {
                        "a" => "link".to_string(),
                        other => other.to_string(),
                    }
                });
            let name = element
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut line = format!("[{reference}] {kind} \"{name}\"");
            if let Some(href) = element.get("href").and_then(Value::as_str) {
                line.push_str(&format!(" -> {href}"));
            }
            if element.get("disabled").and_then(Value::as_bool) == Some(true) {
                line.push_str(" (disabled)");
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_browser_snapshot(snapshot: &Value, include_text: bool) -> String {
    let mut output = page_line(snapshot);
    if include_text {
        let text = snapshot
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        output.push_str("\n\n");
        output.push_str(if text.is_empty() {
            "(no visible text)"
        } else {
            text
        });
        if snapshot.get("truncated").and_then(Value::as_bool) == Some(true) {
            output.push_str("\n\n[text truncated; raise max_chars or scroll to read more]");
        }
    }
    let elements = snapshot
        .get("elements")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    output.push_str("\n\n## Interactive elements (pass ref to browser_click / browser_type)\n");
    output.push_str(if elements.is_empty() {
        "(none visible)"
    } else {
        ""
    });
    output.push_str(&format_browser_elements(&elements));
    if elements.len() >= scripts::SNAPSHOT_ELEMENT_LIMIT {
        output.push_str(&format!(
            "\n[only the first {} elements are listed; scroll or pass selector to narrow]",
            scripts::SNAPSHOT_ELEMENT_LIMIT
        ));
    }
    output
}

fn format_tabs(result: &Value) -> String {
    let tabs = result
        .get("tabs")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if tabs.is_empty() {
        return "No open tabs.".to_string();
    }
    tabs.iter()
        .map(|tab| {
            format!(
                "{} {} — {}",
                if tab.get("active").and_then(Value::as_bool) == Some(true) {
                    "*"
                } else {
                    " "
                },
                tab.get("id").and_then(Value::as_str).unwrap_or("?"),
                page_line(tab),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn save_screenshot(agent: &AgentEngine, thread_id: &str, result: &Value) -> Result<String> {
    use base64::Engine as _;
    let data = result
        .get("png_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("the browser returned no screenshot"))?;
    let data = data
        .split_once("base64,")
        .map(|(_, payload)| payload)
        .unwrap_or(data);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .context("screenshot was not valid base64")?;
    let dir = zorai_protocol::thread_inventory_dir(agent.history.data_root(), thread_id);
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!("browser-screenshot-{}.png", now_millis()));
    tokio::fs::write(&path, &bytes).await?;
    Ok(path.display().to_string())
}

pub(crate) async fn execute_browser_tool(
    name: &str,
    args: &Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let backend = browser_backend_for_surface(agent.get_thread_client_surface(thread_id).await);
    let run = |op: BrowserOp| async move { run_browser_op(agent, thread_id, backend, &op).await };
    match name {
        tool_names::BROWSER_NAVIGATE => {
            let url = arg_str(args, "url").ok_or_else(|| anyhow::anyhow!("url is required"))?;
            let info = run(BrowserOp::Navigate {
                url: browser_navigation_url(url),
            })
            .await?;
            Ok(format!(
                "Opened {}. Call browser_read_dom to read the page.",
                page_line(&info)
            ))
        }
        tool_names::BROWSER_READ_DOM | tool_names::BROWSER_GET_ELEMENTS => {
            let include_text = name == tool_names::BROWSER_READ_DOM;
            let max_chars = args
                .get("max_chars")
                .and_then(Value::as_u64)
                .map(|value| (value as usize).clamp(200, MAX_READ_MAX_CHARS))
                .unwrap_or(DEFAULT_READ_MAX_CHARS);
            let script =
                scripts::snapshot_script(max_chars, arg_str(args, "selector"), include_text);
            let snapshot = parse_script_json(&run(BrowserOp::Evaluate { script }).await?)?;
            Ok(format_browser_snapshot(&snapshot, include_text))
        }
        tool_names::BROWSER_CLICK => {
            let target = browser_target(args).ok_or_else(|| {
                anyhow::anyhow!("pass ref, selector or text to choose what to click")
            })?;
            let clicked = parse_script_json(
                &run(BrowserOp::Evaluate {
                    script: scripts::click_script(&target),
                })
                .await?,
            )?;
            Ok(format!(
                "Clicked {} (now at {}).",
                clicked
                    .get("clicked")
                    .and_then(Value::as_str)
                    .unwrap_or("the element"),
                clicked
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("the same page"),
            ))
        }
        tool_names::BROWSER_TYPE => {
            let target = browser_target(args)
                .ok_or_else(|| anyhow::anyhow!("pass ref, selector or text to choose the field"))?;
            let text = args
                .get("text_to_type")
                .or_else(|| args.get("value"))
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("text_to_type is required"))?;
            let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(true);
            let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
            let typed = parse_script_json(
                &run(BrowserOp::Evaluate {
                    script: scripts::type_script(&target, text, clear, submit),
                })
                .await?,
            )?;
            Ok(format!(
                "Typed into {}{}.",
                typed
                    .get("typed_into")
                    .and_then(Value::as_str)
                    .unwrap_or("the field"),
                if submit { " and submitted" } else { "" },
            ))
        }
        tool_names::BROWSER_SCROLL => {
            let target = browser_target(args);
            let direction = arg_str(args, "direction").unwrap_or("down");
            let amount = args.get("amount").and_then(Value::as_i64);
            let scrolled = parse_script_json(
                &run(BrowserOp::Evaluate {
                    script: scripts::scroll_script(target.as_ref(), direction, amount),
                })
                .await?,
            )?;
            Ok(format!(
                "Scrolled to y={} of {} (viewport {}).",
                scrolled
                    .get("scroll_y")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                scrolled
                    .get("page_height")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                scrolled
                    .get("viewport_height")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            ))
        }
        tool_names::BROWSER_EVAL_JS => {
            let script =
                arg_str(args, "script").ok_or_else(|| anyhow::anyhow!("script is required"))?;
            let result = run(BrowserOp::Evaluate {
                script: script.to_string(),
            })
            .await?;
            let value = result.get("value").cloned().unwrap_or(Value::Null);
            let mut rendered = match value {
                Value::String(text) => text,
                other => serde_json::to_string_pretty(&other).unwrap_or_default(),
            };
            if rendered.chars().count() > EVAL_RESULT_MAX_CHARS {
                rendered = rendered
                    .chars()
                    .take(EVAL_RESULT_MAX_CHARS)
                    .collect::<String>()
                    + "\n[result truncated]";
            }
            Ok(rendered)
        }
        tool_names::BROWSER_TAKE_SCREENSHOT => {
            let shot = run(BrowserOp::Screenshot).await?;
            let path = save_screenshot(agent, thread_id, &shot).await?;
            Ok(format!("Saved a screenshot of the current tab to {path}. Pass this path to analyze_image to look at it."))
        }
        tool_names::BROWSER_BACK | tool_names::BROWSER_FORWARD | tool_names::BROWSER_RELOAD => {
            let direction = match name {
                tool_names::BROWSER_BACK => "back",
                tool_names::BROWSER_FORWARD => "forward",
                _ => "reload",
            };
            let info = run(BrowserOp::History {
                direction: direction.to_string(),
            })
            .await?;
            Ok(format!("Now at {}.", page_line(&info)))
        }
        tool_names::BROWSER_TABS => {
            let action = arg_str(args, "action").unwrap_or("list").to_string();
            let tabs = run(BrowserOp::Tabs {
                action,
                tab_id: arg_str(args, "tab_id").map(str::to_string),
                url: arg_str(args, "url").map(str::to_string),
            })
            .await?;
            Ok(format_tabs(&tabs))
        }
        other => anyhow::bail!("{other} is not a browser tool"),
    }
}

pub(crate) fn browser_tool_definitions() -> Vec<ToolDefinition> {
    use crate::agent::tool_executor::tool_def;
    let target_properties = serde_json::json!({
        "ref": { "type": "string", "description": "Element ref from the latest browser_read_dom / browser_get_elements output (preferred)" },
        "selector": { "type": "string", "description": "CSS selector, when no ref fits" },
        "text": { "type": "string", "description": "Visible text or label of the element, as a last resort" }
    });
    let with_target = |extra: Value| {
        let mut properties = target_properties.clone();
        if let (Some(base), Some(extra)) = (properties.as_object_mut(), extra.as_object()) {
            base.extend(extra.clone());
        }
        properties
    };
    vec![
        tool_def(
            tool_names::BROWSER_NAVIGATE,
            "Open a URL in the browser shared with the operator (the Zorai in-app browser in the desktop app, otherwise Chrome). Returns the page title; call browser_read_dom next to read it.",
            serde_json::json!({
                "type": "object",
                "properties": { "url": { "type": "string", "description": "URL to open; https:// is assumed when no scheme is given" } },
                "required": ["url"]
            }),
        ),
        tool_def(
            tool_names::BROWSER_READ_DOM,
            "Read the current page: title, URL, visible text and a numbered list of interactive elements. Use the element refs with browser_click and browser_type.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "max_chars": { "type": "integer", "description": "Max characters of page text (default 6000, max 40000)" },
                    "selector": { "type": "string", "description": "Only list interactive elements inside this CSS selector" }
                }
            }),
        ),
        tool_def(
            tool_names::BROWSER_GET_ELEMENTS,
            "List the page's visible interactive elements (links, buttons, fields) with refs, without the page text.",
            serde_json::json!({
                "type": "object",
                "properties": { "selector": { "type": "string", "description": "Only list elements inside this CSS selector" } }
            }),
        ),
        tool_def(
            tool_names::BROWSER_CLICK,
            "Click an element on the current page, chosen by ref (preferred), CSS selector or visible text.",
            serde_json::json!({ "type": "object", "properties": target_properties.clone() }),
        ),
        tool_def(
            tool_names::BROWSER_TYPE,
            "Type into a field on the current page, chosen by ref (preferred), CSS selector or label text. Replaces the field's value unless clear is false.",
            serde_json::json!({
                "type": "object",
                "properties": with_target(serde_json::json!({
                    "text_to_type": { "type": "string", "description": "Text to enter" },
                    "clear": { "type": "boolean", "description": "Replace the current value (default true)" },
                    "submit": { "type": "boolean", "description": "Press Enter / submit the form afterwards (default false)" }
                })),
                "required": ["text_to_type"]
            }),
        ),
        tool_def(
            tool_names::BROWSER_SCROLL,
            "Scroll the page up, down, to the top or bottom, or bring an element (by ref, selector or text) into view.",
            serde_json::json!({
                "type": "object",
                "properties": with_target(serde_json::json!({
                    "direction": { "type": "string", "enum": ["up", "down", "top", "bottom"], "description": "Scroll direction when no element is given (default down)" },
                    "amount": { "type": "integer", "description": "Pixels to scroll for up/down (default 80% of the viewport)" }
                }))
            }),
        ),
        tool_def(
            tool_names::BROWSER_EVAL_JS,
            "Run JavaScript in the current page and return the value of its last expression (promises are awaited). Use for reading data the snapshot does not show.",
            serde_json::json!({
                "type": "object",
                "properties": { "script": { "type": "string", "description": "JavaScript to run in the page" } },
                "required": ["script"]
            }),
        ),
        tool_def(
            tool_names::BROWSER_TAKE_SCREENSHOT,
            "Capture the visible part of the current tab as a PNG in the thread files and return its path; pass the path to analyze_image to see it.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool_def(
            tool_names::BROWSER_BACK,
            "Go back one page in the current tab.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool_def(
            tool_names::BROWSER_FORWARD,
            "Go forward one page in the current tab.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool_def(
            tool_names::BROWSER_RELOAD,
            "Reload the current tab.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ),
        tool_def(
            tool_names::BROWSER_TABS,
            "List, open, switch or close browser tabs. Other browser tools act on the active tab (marked * in the list).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["list", "new", "switch", "close"], "description": "Tab action (default list)" },
                    "tab_id": { "type": "string", "description": "Tab id from the list, for switch and close" },
                    "url": { "type": "string", "description": "URL to open in the new tab" }
                }
            }),
        ),
    ]
}
