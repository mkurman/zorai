use super::*;
use crate::agent::{
    config::load_config_from_items,
    tool_executor::{execute_tool, get_available_tools},
    types::{AgentConfig, ToolCall, ToolFunction},
    AgentEngine,
};
use crate::session_manager::SessionManager;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    time::{timeout, Duration},
};

#[tokio::test]
async fn parallel_search_mcp_sends_project_user_agent_on_every_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut tool_calls = 0;
        let mut initializations = 0;
        while tool_calls < 4 {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut request_line = String::new();
            socket.read_line(&mut request_line).await.unwrap();
            let mut headers = Map::new();
            loop {
                let mut line = String::new();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                let (name, value) = line.split_once(':').unwrap();
                headers.insert(name.to_ascii_lowercase(), json!(value.trim()));
            }
            assert_eq!(headers["user-agent"], USER_AGENT);
            for header in [
                "authorization",
                "proxy-authorization",
                "x-api-key",
                "cookie",
            ] {
                assert!(!headers.contains_key(header), "unexpected {header}");
            }
            let length = headers
                .get("content-length")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            let request = if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            };
            let method = request["method"].as_str().unwrap_or_default();
            println!(
                "MCP wire {} {method} User-Agent: {USER_AGENT}",
                request_line.trim()
            );
            let mut response_headers = String::new();
            let mut body = String::new();
            let status = match method {
                "initialize" => {
                    initializations += 1;
                    response_headers = format!("Mcp-Session-Id: fixture-{initializations}\r\n");
                    body = json!({
                        "jsonrpc": "2.0", "id": request["id"],
                        "result": {
                            "protocolVersion": "2025-03-26",
                            "capabilities": {"tools": {}},
                            "serverInfo": {"name": "fixture", "version": "1"},
                        }
                    })
                    .to_string();
                    "200 OK"
                }
                "notifications/initialized" | "tools/call" => {
                    assert_eq!(
                        headers["mcp-session-id"],
                        format!("fixture-{initializations}")
                    );
                    assert_eq!(headers["mcp-protocol-version"], "2025-03-26");
                    if method == "notifications/initialized" {
                        "202 Accepted"
                    } else {
                        tool_calls += 1;
                        assert_eq!(request["params"]["name"], "web_search");
                        assert_eq!(
                            request["params"]["arguments"],
                            json!({"objective": "Rust docs", "search_queries": ["Rust docs"]})
                        );
                        if tool_calls == 1 {
                            socket.get_mut().write_all(
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            ).await.unwrap();
                            continue;
                        }
                        let results = json!({"results": [
                            {"url": "javascript:invalid", "title": "Ignore this"},
                            {
                                "url": "https://doc.rust-lang.org/book/?q=rust#introduction",
                                "title": "The Rust\nBook",
                                "excerpts": [format!("{}SHOULD BE TRUNCATED", "é".repeat(300))],
                            },
                            {"url": "https://example.org/extra"},
                        ]});
                        let result = match tool_calls {
                            2 => {
                                json!({"content": [{"type": "text", "text": results.to_string()}]})
                            }
                            3 => json!({"structuredContent": results, "content": []}),
                            _ => json!({"isError": true, "content": [
                                {"type": "text", "text": "Untrusted MCP error text"}
                            ]}),
                        };
                        let envelope =
                            json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
                        response_headers = "Content-Type: text/event-stream\r\n".to_string();
                        body = format!("event: message\r\ndata: {envelope}\r\n\r\n");
                        "200 OK"
                    }
                }
                _ if request_line.starts_with("GET /mcp ") => "405 Method Not Allowed",
                _ if request_line.starts_with("DELETE /mcp ") => "204 No Content",
                _ => panic!("unexpected request: {request_line}"),
            };
            if method == "initialize" {
                response_headers.push_str("Content-Type: application/json\r\n");
            }
            socket.get_mut().write_all(format!(
                "HTTP/1.1 {status}\r\n{response_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ).as_bytes()).await.unwrap();
        }
        assert_eq!(initializations, 4);
    });

    timeout(Duration::from_secs(15), async {
        for _ in 0..2 {
            let output = search_at(&endpoint, "Rust docs", 1).await.unwrap();
            assert!(output.contains("untrusted web content"));
            assert!(output.contains("The Rust Book"));
            assert!(output.contains("https://doc.rust-lang.org/book/?q=rust#introduction"));
            assert!(output.contains(&format!("{}...", "é".repeat(300))));
            assert!(!output.contains("SHOULD BE TRUNCATED"));
            assert!(!output.contains("javascript:"));
            assert!(!output.contains("https://example.org/extra"));
        }
        let error = search_at(&endpoint, "Rust docs", 1).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "Parallel Search MCP returned a search error"
        );
        server.await.unwrap();
    })
    .await
    .expect("MCP fixture did not finish");
}

#[tokio::test]
async fn parallel_search_drops_stalled_initialization_with_search() {
    for response in [
        &b""[..],
        &b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"[..],
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let (initialize_seen, seen) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut line = String::new();
            socket.read_line(&mut line).await.unwrap();
            assert!(line.starts_with("POST /mcp "));
            let mut length = 0;
            loop {
                line.clear();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&body).unwrap()["method"],
                "initialize"
            );
            socket.get_mut().write_all(response).await.unwrap();
            initialize_seen.send(()).unwrap();
            let mut extra_bytes = Vec::new();
            match socket.read_to_end(&mut extra_bytes).await {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionReset => {}
                Err(err) => panic!("could not observe the HTTP connection closing: {err}"),
            }
            assert!(extra_bytes.is_empty(), "unexpected extra HTTP data");
        });

        let mut search = Box::pin(search_at(&endpoint, "Rust docs", 1));
        timeout(Duration::from_secs(5), async {
            tokio::select! {
                result = &mut search => panic!("search finished before cancellation: {result:?}"),
                result = seen => result.unwrap(),
            }
        })
        .await
        .expect("fixture did not receive the initialize request");
        drop(search);
        timeout(Duration::from_secs(2), server)
            .await
            .expect("cancelled MCP initialization kept its HTTP connection open")
            .unwrap();
    }
}

#[tokio::test]
async fn parallel_search_setting_persists_without_enabling_web_search() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let config = engine
        .set_config_item_json("/search_provider", r#""parallel""#)
        .await
        .unwrap();
    assert_eq!(config.extra["search_provider"], "parallel");
    assert!(!get_available_tools(&config, root.path(), false)
        .iter()
        .any(|tool| tool.function.name == "web_search"));

    let persisted =
        load_config_from_items(engine.history.list_agent_config_items().await.unwrap()).unwrap();
    assert_eq!(persisted.extra["search_provider"], "parallel");
    assert!(!persisted.tools.web_search);

    let result = execute_tool(
        &ToolCall::with_default_weles_review(
            "parallel-disabled-search".to_string(),
            ToolFunction {
                name: "web_search".to_string(),
                arguments: json!({ "query": "Rust docs", "timeout_seconds": 1 }).to_string(),
            },
        ),
        &engine,
        "parallel-disabled-search",
        None,
        &manager,
        None,
        &engine.event_sender(),
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    assert!(result.is_error, "{}", result.content);
    assert_eq!(
        result.content,
        "Error: Parallel search requires Web Search to be enabled"
    );
}

#[tokio::test]
#[ignore = "calls the public anonymous Parallel Search MCP"]
async fn parallel_search_live_anonymous_via_daemon_tool() {
    let root = tempfile::tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let config = load_config_from_items(vec![
        ("/search_provider".to_string(), json!("parallel")),
        ("/tools/web_search".to_string(), json!(true)),
    ])
    .unwrap();
    assert!(get_available_tools(&config, root.path(), false)
        .iter()
        .any(|tool| tool.function.name == "web_search"));
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let result = execute_tool(
        &ToolCall::with_default_weles_review(
            "parallel-live-search".to_string(),
            ToolFunction {
                name: "web_search".to_string(),
                arguments: json!({
                    "query": "site:rust-lang.org Rust programming language official",
                    "max_results": 3,
                    "timeout_seconds": 30,
                })
                .to_string(),
            },
        ),
        &engine,
        "parallel-live-search",
        None,
        &manager,
        None,
        &engine.event_sender(),
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    assert!(!result.is_error, "{}", result.content);
    assert!(
        result.content.contains("https://rust-lang.org/"),
        "{}",
        result.content
    );
    assert!(result.content.contains("untrusted web content"));
    println!("{}", result.content);
}
