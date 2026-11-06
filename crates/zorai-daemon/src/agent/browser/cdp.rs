use super::*;
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio_tungstenite::tungstenite::Message;

const CDP_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const CDP_LAUNCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const PAGE_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

type PendingCalls = Arc<
    std::sync::Mutex<
        HashMap<u64, tokio::sync::oneshot::Sender<std::result::Result<Value, String>>>,
    >,
>;

pub(crate) struct CdpBrowser {
    outgoing: tokio::sync::mpsc::UnboundedSender<Message>,
    pending: PendingCalls,
    next_id: AtomicU64,
    closed: Arc<AtomicBool>,
    sessions: HashMap<String, String>,
    _child: Option<tokio::process::Child>,
}

impl CdpBrowser {
    async fn connect(ws_url: &str, child: Option<tokio::process::Child>) -> Result<Self> {
        let (stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .with_context(|| format!("failed to connect to Chrome DevTools at {ws_url}"))?;
        let (mut sink, mut source) = stream.split();
        let (outgoing, mut outgoing_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
        let pending: PendingCalls = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));

        tokio::spawn(async move {
            while let Some(message) = outgoing_rx.recv().await {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = pending.clone();
        let reader_closed = closed.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let Ok(payload) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                let Some(id) = payload.get("id").and_then(Value::as_u64) else {
                    continue;
                };
                let Some(reply) = reader_pending
                    .lock()
                    .ok()
                    .and_then(|mut calls| calls.remove(&id))
                else {
                    continue;
                };
                let outcome = match payload.get("error") {
                    Some(error) => Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Chrome DevTools call failed")
                        .to_string()),
                    None => Ok(payload.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(outcome);
            }
            reader_closed.store(true, Ordering::SeqCst);
            if let Ok(mut calls) = reader_pending.lock() {
                calls.clear();
            }
        });

        Ok(Self {
            outgoing,
            pending,
            next_id: AtomicU64::new(1),
            closed,
            sessions: HashMap::new(),
            _child: child,
        })
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    async fn call(&self, method: &str, params: Value, session_id: Option<&str>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut request = serde_json::json!({ "id": id, "method": method, "params": params });
        if let Some(session_id) = session_id {
            request["sessionId"] = Value::String(session_id.to_string());
        }
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Chrome DevTools state is poisoned"))?
            .insert(id, reply_tx);
        self.outgoing
            .send(Message::Text(request.to_string().into()))
            .map_err(|_| anyhow::anyhow!("Chrome DevTools connection is closed"))?;
        match tokio::time::timeout(CDP_CALL_TIMEOUT, reply_rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(message))) => Err(anyhow::anyhow!("{method} failed: {message}")),
            Ok(Err(_)) => Err(anyhow::anyhow!("Chrome closed while running {method}")),
            Err(_) => {
                if let Ok(mut calls) = self.pending.lock() {
                    calls.remove(&id);
                }
                Err(anyhow::anyhow!(
                    "{method} timed out after {}s",
                    CDP_CALL_TIMEOUT.as_secs()
                ))
            }
        }
    }

    async fn session_for(&mut self, target_id: &str) -> Result<String> {
        if let Some(session_id) = self.sessions.get(target_id) {
            return Ok(session_id.clone());
        }
        let attached = self
            .call(
                "Target.attachToTarget",
                serde_json::json!({ "targetId": target_id, "flatten": true }),
                None,
            )
            .await?;
        let session_id = attached
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("Chrome did not return a session for tab {target_id}"))?
            .to_string();
        self.sessions
            .insert(target_id.to_string(), session_id.clone());
        Ok(session_id)
    }

    async fn page_targets(&self) -> Result<Vec<Value>> {
        let targets = self
            .call("Target.getTargets", serde_json::json!({}), None)
            .await?;
        Ok(targets
            .get("targetInfos")
            .and_then(Value::as_array)
            .map(|infos| {
                infos
                    .iter()
                    .filter(|info| info.get("type").and_then(Value::as_str) == Some("page"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn create_tab(&self, url: &str) -> Result<String> {
        let created = self
            .call(
                "Target.createTarget",
                serde_json::json!({ "url": url }),
                None,
            )
            .await?;
        created
            .get("targetId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("Chrome did not return the new tab id"))
    }

    async fn evaluate(&mut self, target_id: &str, expression: &str) -> Result<Value> {
        let session_id = self.session_for(target_id).await?;
        let evaluated = self
            .call(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                    "userGesture": true,
                }),
                Some(&session_id),
            )
            .await?;
        if let Some(details) = evaluated.get("exceptionDetails") {
            let message = details
                .pointer("/exception/description")
                .or_else(|| details.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("script threw an exception");
            anyhow::bail!("{message}");
        }
        Ok(evaluated
            .pointer("/result/value")
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn wait_until_ready(&mut self, target_id: &str) -> Result<()> {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let deadline = tokio::time::Instant::now() + PAGE_READY_TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            if let Ok(state) = self
                .evaluate(target_id, &scripts::ready_state_script())
                .await
            {
                if state.as_str() == Some("complete") {
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        Ok(())
    }

    async fn page_info(&mut self, target_id: &str) -> Result<Value> {
        let info = self
            .evaluate(target_id, &scripts::page_info_script())
            .await?;
        Ok(info
            .as_str()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(info))
    }
}

fn headless_by_default() -> bool {
    cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

async fn attach_ws_url(endpoint: &str, http_client: &reqwest::Client) -> Result<String> {
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_string());
    }
    let version_url = format!("{}/json/version", endpoint.trim_end_matches('/'));
    let version: Value = http_client
        .get(&version_url)
        .send()
        .await
        .with_context(|| format!("could not reach Chrome DevTools at {version_url}; start Chrome with --remote-debugging-port"))?
        .json()
        .await?;
    version
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{version_url} did not report a webSocketDebuggerUrl"))
}

async fn launch_chrome(
    agent: &AgentEngine,
    headless: bool,
) -> Result<(String, tokio::process::Child)> {
    let chrome = crate::agent::tool_executor::detect_chrome().ok_or_else(|| {
        anyhow::anyhow!("no Chrome or Chromium found on PATH; install one or set browser_cdp_url to an existing DevTools endpoint")
    })?;
    let profile_dir = agent.data_dir.join("browser").join("chrome-cdp-profile");
    tokio::fs::create_dir_all(&profile_dir).await?;
    let port_file = profile_dir.join("DevToolsActivePort");
    let _ = tokio::fs::remove_file(&port_file).await;
    let mut command = tokio::process::Command::new(&chrome.bin);
    command
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if headless {
        command.arg("--headless=new");
    }
    command.arg("about:blank");
    let child = command
        .spawn()
        .with_context(|| format!("failed to start {}", chrome.bin))?;
    let deadline = tokio::time::Instant::now() + CDP_LAUNCH_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if let Ok(contents) = tokio::fs::read_to_string(&port_file).await {
            let mut lines = contents.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                return Ok((
                    format!("ws://127.0.0.1:{}{}", port.trim(), path.trim()),
                    child,
                ));
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    anyhow::bail!(
        "Chrome did not open its DevTools port within {}s",
        CDP_LAUNCH_TIMEOUT.as_secs()
    )
}

async fn ensure_browser<'a>(
    agent: &AgentEngine,
    slot: &'a mut Option<CdpBrowser>,
) -> Result<&'a mut CdpBrowser> {
    if slot.as_ref().is_some_and(CdpBrowser::is_closed) {
        *slot = None;
        agent.browser.cdp_active_tabs.lock().await.clear();
    }
    if slot.is_none() {
        let (endpoint, headless) = {
            let config = agent.config.read().await;
            let endpoint = config
                .extra
                .get("browser_cdp_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let headless = config
                .extra
                .get("browser_headless")
                .and_then(Value::as_bool)
                .unwrap_or_else(headless_by_default);
            (endpoint, headless)
        };
        let browser = match endpoint {
            Some(endpoint) => {
                let ws_url = attach_ws_url(&endpoint, &agent.http_client).await?;
                CdpBrowser::connect(&ws_url, None).await?
            }
            None => {
                let (ws_url, child) = launch_chrome(agent, headless).await?;
                CdpBrowser::connect(&ws_url, Some(child)).await?
            }
        };
        *slot = Some(browser);
    }
    Ok(slot.as_mut().expect("browser connected above"))
}

async fn active_tab(agent: &AgentEngine, browser: &CdpBrowser, thread_id: &str) -> Result<String> {
    let pages = browser.page_targets().await?;
    let mut active_tabs = agent.browser.cdp_active_tabs.lock().await;
    if let Some(target_id) = active_tabs.get(thread_id) {
        if pages
            .iter()
            .any(|page| page.get("targetId").and_then(Value::as_str) == Some(target_id.as_str()))
        {
            return Ok(target_id.clone());
        }
    }
    let target_id = browser.create_tab("about:blank").await?;
    active_tabs.insert(thread_id.to_string(), target_id.clone());
    Ok(target_id)
}

async fn tab_list(agent: &AgentEngine, browser: &CdpBrowser, thread_id: &str) -> Result<Value> {
    let pages = browser.page_targets().await?;
    let active = agent
        .browser
        .cdp_active_tabs
        .lock()
        .await
        .get(thread_id)
        .cloned();
    let tabs = pages
        .iter()
        .map(|page| {
            let id = page
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            serde_json::json!({
                "id": id,
                "url": page.get("url").cloned().unwrap_or(Value::Null),
                "title": page.get("title").cloned().unwrap_or(Value::Null),
                "active": active.as_deref() == Some(id),
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({ "tabs": tabs }))
}

pub(super) async fn cdp_browser_request(
    agent: &AgentEngine,
    thread_id: &str,
    op: &BrowserOp,
) -> Result<Value> {
    let mut slot = agent.browser.cdp.lock().await;
    let browser = ensure_browser(agent, &mut slot).await?;
    match op {
        BrowserOp::Tabs {
            action,
            tab_id,
            url,
        } => {
            match action.as_str() {
                "list" => {}
                "new" => {
                    let target_id = browser
                        .create_tab(url.as_deref().unwrap_or("about:blank"))
                        .await?;
                    agent
                        .browser
                        .cdp_active_tabs
                        .lock()
                        .await
                        .insert(thread_id.to_string(), target_id.clone());
                    if url.is_some() {
                        browser.wait_until_ready(&target_id).await?;
                    }
                }
                "switch" | "close" => {
                    let tab_id = tab_id
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("tab_id is required to {action} a tab"))?;
                    if action == "switch" {
                        browser
                            .call(
                                "Target.activateTarget",
                                serde_json::json!({ "targetId": tab_id }),
                                None,
                            )
                            .await?;
                        agent
                            .browser
                            .cdp_active_tabs
                            .lock()
                            .await
                            .insert(thread_id.to_string(), tab_id.to_string());
                    } else {
                        browser
                            .call(
                                "Target.closeTarget",
                                serde_json::json!({ "targetId": tab_id }),
                                None,
                            )
                            .await?;
                        browser.sessions.remove(tab_id);
                        let mut active_tabs = agent.browser.cdp_active_tabs.lock().await;
                        if active_tabs.get(thread_id).map(String::as_str) == Some(tab_id) {
                            active_tabs.remove(thread_id);
                        }
                    }
                }
                other => {
                    anyhow::bail!("unknown tab action {other}; use list, new, switch or close")
                }
            }
            tab_list(agent, browser, thread_id).await
        }
        BrowserOp::Navigate { url } => {
            let target_id = active_tab(agent, browser, thread_id).await?;
            let session_id = browser.session_for(&target_id).await?;
            let navigated = browser
                .call(
                    "Page.navigate",
                    serde_json::json!({ "url": url }),
                    Some(&session_id),
                )
                .await?;
            if let Some(error) = navigated.get("errorText").and_then(Value::as_str) {
                anyhow::bail!("navigation to {url} failed: {error}");
            }
            browser.wait_until_ready(&target_id).await?;
            browser.page_info(&target_id).await
        }
        BrowserOp::History { direction } => {
            let target_id = active_tab(agent, browser, thread_id).await?;
            match direction.as_str() {
                "back" => browser
                    .evaluate(&target_id, "history.back()")
                    .await
                    .map(|_| ())?,
                "forward" => browser
                    .evaluate(&target_id, "history.forward()")
                    .await
                    .map(|_| ())?,
                "reload" => {
                    let session_id = browser.session_for(&target_id).await?;
                    browser
                        .call("Page.reload", serde_json::json!({}), Some(&session_id))
                        .await?;
                }
                other => anyhow::bail!("unknown history direction {other}"),
            }
            browser.wait_until_ready(&target_id).await?;
            browser.page_info(&target_id).await
        }
        BrowserOp::Evaluate { script } => {
            let target_id = active_tab(agent, browser, thread_id).await?;
            let value = browser.evaluate(&target_id, script).await?;
            Ok(serde_json::json!({ "value": value }))
        }
        BrowserOp::Screenshot => {
            let target_id = active_tab(agent, browser, thread_id).await?;
            let session_id = browser.session_for(&target_id).await?;
            let shot = browser
                .call(
                    "Page.captureScreenshot",
                    serde_json::json!({ "format": "png" }),
                    Some(&session_id),
                )
                .await?;
            let data = shot
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("Chrome returned no screenshot data"))?;
            Ok(serde_json::json!({ "png_base64": data }))
        }
    }
}
