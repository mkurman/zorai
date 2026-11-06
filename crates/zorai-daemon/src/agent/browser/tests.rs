use super::*;
use tempfile::tempdir;

async fn gui_thread_engine(thread_id: &str) -> (tempfile::TempDir, std::sync::Arc<AgentEngine>) {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    engine
        .set_thread_client_surface(thread_id, zorai_protocol::ClientSurface::Electron)
        .await;
    (root, engine)
}

async fn answer_next_request(
    engine: std::sync::Arc<AgentEngine>,
    mut events: tokio::sync::broadcast::Receiver<AgentEvent>,
    ok: bool,
    result: Value,
) -> (String, Value) {
    loop {
        if let AgentEvent::BrowserRequest {
            thread_id,
            request_id,
            op,
        } = events.recv().await.expect("event")
        {
            let payload = if ok {
                result.to_string()
            } else {
                serde_json::to_string(result.as_str().unwrap_or_default()).unwrap()
            };
            assert!(
                engine
                    .answer_browser_request(&request_id, ok, &payload)
                    .await
            );
            return (thread_id, op);
        }
    }
}

#[test]
fn threads_open_in_the_desktop_app_use_its_browser_and_everything_else_uses_chrome() {
    assert_eq!(
        browser_backend_for_surface(Some(zorai_protocol::ClientSurface::Electron)),
        BrowserBackend::Gui
    );
    assert_eq!(
        browser_backend_for_surface(Some(zorai_protocol::ClientSurface::Tui)),
        BrowserBackend::Cdp
    );
    assert_eq!(browser_backend_for_surface(None), BrowserBackend::Cdp);
}

#[tokio::test]
async fn navigate_on_a_gui_thread_is_carried_out_by_the_desktop_app_and_reports_the_page() {
    let (_root, engine) = gui_thread_engine("thread_gui").await;
    let answering = tokio::spawn(answer_next_request(
        engine.clone(),
        engine.event_tx.subscribe(),
        true,
        serde_json::json!({ "url": "http://127.0.0.1:5173/", "title": "Zorai dev" }),
    ));

    let output = execute_browser_tool(
        tool_names::BROWSER_NAVIGATE,
        &serde_json::json!({ "url": "http://127.0.0.1:5173/" }),
        &engine,
        "thread_gui",
    )
    .await
    .expect("navigate");

    let (thread_id, op) = answering.await.expect("gui");
    assert_eq!(thread_id, "thread_gui");
    assert_eq!(
        op,
        serde_json::json!({ "kind": "navigate", "url": "http://127.0.0.1:5173/" })
    );
    assert!(
        output.contains("Zorai dev — http://127.0.0.1:5173/"),
        "{output}"
    );
    assert!(
        engine.browser.gui_pending.lock().await.is_empty(),
        "answered requests must not linger"
    );
}

#[tokio::test]
async fn a_refusal_from_the_desktop_app_becomes_the_tool_error_the_agent_sees() {
    let (_root, engine) = gui_thread_engine("thread_gui").await;
    let answering = tokio::spawn(answer_next_request(
        engine.clone(),
        engine.event_tx.subscribe(),
        false,
        Value::String("The operator has taken control of the browser.".to_string()),
    ));

    let error = execute_browser_tool(
        tool_names::BROWSER_RELOAD,
        &serde_json::json!({}),
        &engine,
        "thread_gui",
    )
    .await
    .expect_err("refused");
    answering.await.expect("gui");

    assert_eq!(
        error.to_string(),
        "The operator has taken control of the browser."
    );
}

#[tokio::test]
async fn read_dom_lists_page_text_and_numbered_elements_from_the_page_script() {
    let (_root, engine) = gui_thread_engine("thread_gui").await;
    let snapshot = serde_json::json!({
        "url": "https://example.com/login",
        "title": "Sign in",
        "text": "Welcome back",
        "truncated": false,
        "elements": [
            { "ref": "1", "tag": "input", "type": "email", "name": "Email" },
            { "ref": "2", "tag": "a", "name": "Forgot password?", "href": "/reset" },
            { "ref": "3", "tag": "button", "name": "Sign in", "disabled": true }
        ]
    });
    let answering = tokio::spawn(answer_next_request(
        engine.clone(),
        engine.event_tx.subscribe(),
        true,
        serde_json::json!({ "value": snapshot.to_string() }),
    ));

    let output = execute_browser_tool(
        tool_names::BROWSER_READ_DOM,
        &serde_json::json!({}),
        &engine,
        "thread_gui",
    )
    .await
    .expect("read");
    let (_, op) = answering.await.expect("gui");

    assert_eq!(op.get("kind").and_then(Value::as_str), Some("evaluate"));
    assert!(
        output.starts_with("Sign in — https://example.com/login\n\nWelcome back"),
        "{output}"
    );
    assert!(output.contains("[1] email \"Email\""), "{output}");
    assert!(
        output.contains("[2] link \"Forgot password?\" -> /reset"),
        "{output}"
    );
    assert!(
        output.contains("[3] button \"Sign in\" (disabled)"),
        "{output}"
    );
}

#[tokio::test]
async fn a_click_on_a_ref_the_page_no_longer_has_tells_the_agent_to_refresh_refs() {
    let (_root, engine) = gui_thread_engine("thread_gui").await;
    let answering = tokio::spawn(answer_next_request(
        engine.clone(),
        engine.event_tx.subscribe(),
        true,
        serde_json::json!({ "value": r#"{"ok":false,"error":"No element matches {\"ref\":\"9\"}. Call browser_read_dom to refresh element refs."}"# }),
    ));

    let error = execute_browser_tool(
        tool_names::BROWSER_CLICK,
        &serde_json::json!({ "ref": 9 }),
        &engine,
        "thread_gui",
    )
    .await
    .expect_err("missing element");
    answering.await.expect("gui");

    assert!(error.to_string().contains("browser_read_dom"), "{error}");
}

#[test]
fn local_dev_servers_open_over_http_and_other_bare_hosts_over_https() {
    assert_eq!(
        browser_navigation_url("127.0.0.1:5173/"),
        "http://127.0.0.1:5173/"
    );
    assert_eq!(
        browser_navigation_url("localhost:3000"),
        "http://localhost:3000"
    );
    assert_eq!(
        browser_navigation_url("example.com/docs"),
        "https://example.com/docs"
    );
    assert_eq!(
        browser_navigation_url("http://127.0.0.1:5173/"),
        "http://127.0.0.1:5173/"
    );
    assert_eq!(browser_navigation_url("about:blank"), "about:blank");
}

#[test]
fn element_targets_prefer_ref_then_selector_then_text() {
    assert_eq!(
        browser_target(&serde_json::json!({ "ref": 4, "selector": "#a", "text": "Go" })),
        Some(serde_json::json!({ "ref": "4" }))
    );
    assert_eq!(
        browser_target(&serde_json::json!({ "selector": "#a", "text": "Go" })),
        Some(serde_json::json!({ "selector": "#a" }))
    );
    assert_eq!(
        browser_target(&serde_json::json!({ "text": "Go" })),
        Some(serde_json::json!({ "text": "Go" }))
    );
    assert_eq!(browser_target(&serde_json::json!({ "ref": "  " })), None);
}

#[test]
fn browser_tools_are_offered_only_when_web_browsing_is_enabled() {
    let root = tempdir().expect("tempdir");
    let mut config = AgentConfig::default();
    config.tools.web_browse = false;
    let names = |config: &AgentConfig| {
        crate::agent::tool_executor::get_available_tools(config, root.path(), false)
            .into_iter()
            .map(|tool| tool.function.name)
            .collect::<Vec<_>>()
    };
    assert!(names(&config)
        .iter()
        .all(|name| !tool_names::BROWSER_TOOLS.contains(&name.as_str())));

    config.tools.web_browse = true;
    let enabled = names(&config);
    for tool in tool_names::BROWSER_TOOLS {
        assert!(
            enabled.iter().any(|name| name == tool),
            "{tool} missing when web browsing is on"
        );
    }
}

#[tokio::test]
#[ignore = "launches a real headless Chrome"]
async fn chrome_over_cdp_reads_types_clicks_screenshots_and_manages_tabs() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config
        .extra
        .insert("browser_headless".to_string(), Value::Bool(true));
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let page = "data:text/html,<title>Form</title><input id=name placeholder=Name><button onclick=\"document.title='Hello '+document.getElementById('name').value\">Greet</button>";
    let run = |name: &'static str, args: Value| {
        let engine = engine.clone();
        async move { execute_browser_tool(name, &args, &engine, "thread_tui").await }
    };

    let opened = run(
        tool_names::BROWSER_NAVIGATE,
        serde_json::json!({ "url": page }),
    )
    .await
    .expect("navigate");
    assert!(opened.contains("Form"), "{opened}");
    let snapshot = run(tool_names::BROWSER_READ_DOM, serde_json::json!({}))
        .await
        .expect("read");
    assert!(snapshot.contains("\"Name\""), "{snapshot}");
    assert!(snapshot.contains("button \"Greet\""), "{snapshot}");
    run(
        tool_names::BROWSER_TYPE,
        serde_json::json!({ "text": "Name", "text_to_type": "Ada" }),
    )
    .await
    .expect("type");
    run(
        tool_names::BROWSER_CLICK,
        serde_json::json!({ "text": "Greet" }),
    )
    .await
    .expect("click");
    let title = run(
        tool_names::BROWSER_EVAL_JS,
        serde_json::json!({ "script": "document.title" }),
    )
    .await
    .expect("eval");
    assert_eq!(title, "Hello Ada");

    let shot = run(tool_names::BROWSER_TAKE_SCREENSHOT, serde_json::json!({}))
        .await
        .expect("screenshot");
    let path = shot
        .split(" to ")
        .nth(1)
        .and_then(|rest| rest.split(". ").next())
        .expect("path");
    assert!(
        std::fs::read(path).expect("png").starts_with(b"\x89PNG"),
        "{shot}"
    );

    let tabs = run(
        tool_names::BROWSER_TABS,
        serde_json::json!({ "action": "new", "url": "data:text/html,<title>Second</title>" }),
    )
    .await
    .expect("new tab");
    assert!(
        tabs.lines()
            .any(|line| line.starts_with('*') && line.contains("Second")),
        "{tabs}"
    );
    let first_tab = tabs
        .lines()
        .find(|line| line.contains("Hello Ada"))
        .and_then(|line| {
            line.split_whitespace()
                .nth(0)
                .filter(|id| *id != "*")
                .or_else(|| line.split_whitespace().nth(1))
        })
        .expect("first tab id")
        .to_string();
    let switched = run(
        tool_names::BROWSER_TABS,
        serde_json::json!({ "action": "switch", "tab_id": first_tab }),
    )
    .await
    .expect("switch");
    assert!(
        switched
            .lines()
            .any(|line| line.starts_with('*') && line.contains("Hello Ada")),
        "{switched}"
    );
}
