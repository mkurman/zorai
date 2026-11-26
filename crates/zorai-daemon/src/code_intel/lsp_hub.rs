use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const IDLE_SHUTDOWN: Duration = Duration::from_secs(15 * 60);

const SERVERS: &[(&str, &[&str], &[&str], &str)] = &[
    ("rust-analyzer", &[], &["rs"], "rust"),
    ("typescript-language-server", &["--stdio"], &["ts", "tsx", "js", "jsx", "mjs", "cjs"], "typescript"),
    ("pyright-langserver", &["--stdio"], &["py"], "python"),
    ("gopls", &[], &["go"], "go"),
    ("clangd", &[], &["c", "h", "cc", "cpp", "hpp"], "cpp"),
];

pub(crate) fn server_for(path: &str) -> Option<(&'static str, &'static [&'static str], &'static str)> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    SERVERS
        .iter()
        .find(|(_, _, extensions, _)| extensions.contains(&extension.as_str()))
        .map(|(command, args, _, language)| (*command, *args, *language))
}

pub(crate) fn language_id(path: &str, language: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "tsx" => "typescriptreact",
        "jsx" => "javascriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "c" | "h" => "c",
        _ => match language {
            "rust" => "rust",
            "typescript" => "typescript",
            "python" => "python",
            "go" => "go",
            _ => "cpp",
        },
    }
}

pub(crate) fn encode_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn uri_to_relative(uri: &str, root: &Path) -> String {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let decoded = path.replace("%20", " ");
    Path::new(&decoded).strip_prefix(root).map(|relative| relative.to_string_lossy().replace('\\', "/")).unwrap_or(decoded)
}

struct LspServer {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Value>>>,
    diagnostics: Mutex<HashMap<String, (Instant, Vec<Value>)>>,
    versions: Mutex<HashMap<String, i64>>,
    last_used: Mutex<Instant>,
    next_id: AtomicI64,
    root: PathBuf,
    language: &'static str,
}

impl LspServer {
    async fn start(root: &Path, command: &str, args: &[&str], language: &'static str) -> Result<Arc<Self>> {
        let mut child = Command::new(command)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("{command} is not installed"))?;
        let stdin = child.stdin.take().context("language server stdin")?;
        let stdout = child.stdout.take().context("language server stdout")?;
        let server = Arc::new(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            versions: Mutex::new(HashMap::new()),
            last_used: Mutex::new(Instant::now()),
            next_id: AtomicI64::new(1),
            root: root.to_path_buf(),
            language,
        });
        let reader = Arc::downgrade(&server);
        tokio::spawn(async move {
            let mut stdout = BufReader::new(stdout);
            loop {
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if stdout.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                if stdout.read_exact(&mut body).await.is_err() {
                    return;
                }
                let Some(server) = reader.upgrade() else { return };
                if let Ok(message) = serde_json::from_slice::<Value>(&body) {
                    server.handle(message).await;
                }
            }
        });
        server
            .request("initialize", json!({
                "processId": std::process::id(),
                "rootUri": file_uri(root),
                "workspaceFolders": [{ "uri": file_uri(root), "name": root.file_name().and_then(|name| name.to_str()).unwrap_or("workspace") }],
                "capabilities": {
                    "textDocument": {
                        "publishDiagnostics": { "relatedInformation": false },
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "definition": { "linkSupport": true },
                        "synchronization": { "didSave": true }
                    },
                    "workspace": { "symbol": {}, "configuration": true, "workspaceFolders": true }
                }
            }))
            .await?;
        server.notify("initialized", json!({})).await?;
        Ok(server)
    }

    async fn handle(&self, message: Value) {
        if let (Some(id), None) = (message.get("id").and_then(Value::as_i64), message.get("method")) {
            if let Some(sender) = self.pending.lock().await.remove(&id) {
                let _ = sender.send(message);
            }
            return;
        }
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "textDocument/publishDiagnostics" {
            if let Some(uri) = message.pointer("/params/uri").and_then(Value::as_str) {
                let list = message.pointer("/params/diagnostics").and_then(Value::as_array).cloned().unwrap_or_default();
                self.diagnostics.lock().await.insert(uri.to_string(), (Instant::now(), list));
            }
            return;
        }
        if let Some(id) = message.get("id").cloned() {
            let result = if method == "workspace/configuration" {
                Value::Array(vec![Value::Null; message.pointer("/params/items").and_then(Value::as_array).map(Vec::len).unwrap_or(0)])
            } else {
                Value::Null
            };
            let _ = self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result })).await;
        }
    }

    async fn send(&self, message: &Value) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&encode_frame(message)).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params })).await
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        *self.last_used.lock().await = Instant::now();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await?;
        let response = tokio::time::timeout(REQUEST_TIMEOUT, receiver).await.context("language server timed out")??;
        if let Some(error) = response.get("error") {
            anyhow::bail!("language server error: {}", error.get("message").and_then(Value::as_str).unwrap_or("unknown"));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn sync_document(&self, relative: &str) -> Result<String> {
        let path = self.root.join(relative);
        let text = tokio::fs::read_to_string(&path).await.with_context(|| format!("cannot read {relative}"))?;
        let uri = file_uri(&path);
        let mut versions = self.versions.lock().await;
        match versions.get_mut(&uri) {
            Some(version) => {
                *version += 1;
                let version = *version;
                drop(versions);
                self.notify("textDocument/didChange", json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] })).await?;
            }
            None => {
                versions.insert(uri.clone(), 1);
                drop(versions);
                self.notify("textDocument/didOpen", json!({ "textDocument": { "uri": uri, "languageId": language_id(relative, self.language), "version": 1, "text": text } })).await?;
            }
        }
        Ok(uri)
    }
}

type Hub = Mutex<HashMap<(PathBuf, &'static str), Arc<LspServer>>>;

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| {
        tokio::spawn(async {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let mut servers = hub().lock().await;
                let mut idle = Vec::new();
                for (key, server) in servers.iter() {
                    if server.last_used.lock().await.elapsed() > IDLE_SHUTDOWN {
                        idle.push(key.clone());
                    }
                }
                for key in idle {
                    if let Some(server) = servers.remove(&key) {
                        let _ = server.child.lock().await.start_kill();
                    }
                }
            }
        });
        Mutex::new(HashMap::new())
    })
}

async fn server(root: &Path, relative: &str) -> Result<Arc<LspServer>> {
    let (command, args, language) = server_for(relative).with_context(|| format!("no language server is known for {relative}"))?;
    let key = (root.to_path_buf(), command);
    let mut servers = hub().lock().await;
    if let Some(existing) = servers.get(&key) {
        if existing.child.lock().await.try_wait().ok().flatten().is_none() {
            return Ok(Arc::clone(existing));
        }
    }
    let started = LspServer::start(root, command, args, language).await?;
    servers.insert(key, Arc::clone(&started));
    Ok(started)
}

fn position(line: u32, column: u32) -> Value {
    json!({ "line": line.saturating_sub(1), "character": column.saturating_sub(1) })
}

pub(crate) fn format_locations(result: &Value, root: &Path) -> Vec<String> {
    let items = match result {
        Value::Array(items) => items.clone(),
        Value::Null => Vec::new(),
        single => vec![single.clone()],
    };
    items
        .iter()
        .filter_map(|item| {
            let uri = item.get("targetUri").or_else(|| item.get("uri"))?.as_str()?;
            let range = item.get("targetSelectionRange").or_else(|| item.get("range"))?;
            let line = range.pointer("/start/line")?.as_u64()? + 1;
            let column = range.pointer("/start/character")?.as_u64()? + 1;
            Some(format!("{}:{line}:{column}", uri_to_relative(uri, root)))
        })
        .collect()
}

pub(crate) fn format_diagnostics(list: &[Value]) -> Vec<String> {
    list.iter()
        .map(|diagnostic| {
            let severity = match diagnostic.get("severity").and_then(Value::as_u64) {
                Some(1) => "error",
                Some(2) => "warning",
                Some(3) => "info",
                _ => "hint",
            };
            let line = diagnostic.pointer("/range/start/line").and_then(Value::as_u64).unwrap_or(0) + 1;
            let column = diagnostic.pointer("/range/start/character").and_then(Value::as_u64).unwrap_or(0) + 1;
            let message = diagnostic.get("message").and_then(Value::as_str).unwrap_or("").lines().next().unwrap_or("");
            format!("{line}:{column} {severity}: {message}")
        })
        .collect()
}

pub(crate) async fn query(root: &Path, action: &str, path: &str, line: u32, column: u32, query: &str, wait_seconds: u64) -> Result<String> {
    let server = server(root, path).await?;
    let uri = server.sync_document(path).await?;
    let document = json!({ "uri": uri });
    match action {
        "diagnostics" => {
            let synced_at = Instant::now();
            let deadline = synced_at + Duration::from_secs(wait_seconds.clamp(1, 120));
            loop {
                if let Some((at, list)) = server.diagnostics.lock().await.get(&uri) {
                    if *at >= synced_at || Instant::now() >= deadline {
                        let lines = format_diagnostics(list);
                        return Ok(if lines.is_empty() { format!("{path}: no diagnostics") } else { format!("{path}:\n{}", lines.join("\n")) });
                    }
                }
                if Instant::now() >= deadline {
                    return Ok(format!("{path}: the language server has not reported diagnostics yet (it may still be indexing); try again shortly"));
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
        "definition" | "type_definition" | "implementation" | "references" => {
            let method = match action {
                "definition" => "textDocument/definition",
                "type_definition" => "textDocument/typeDefinition",
                "implementation" => "textDocument/implementation",
                _ => "textDocument/references",
            };
            let mut params = json!({ "textDocument": document, "position": position(line, column) });
            if action == "references" {
                params["context"] = json!({ "includeDeclaration": false });
            }
            let locations = format_locations(&server.request(method, params).await?, &server.root);
            Ok(if locations.is_empty() { format!("No {action} found at {path}:{line}:{column}") } else { locations.into_iter().take(200).collect::<Vec<_>>().join("\n") })
        }
        "hover" => {
            let result = server.request("textDocument/hover", json!({ "textDocument": document, "position": position(line, column) })).await?;
            let contents = result.get("contents").cloned().unwrap_or(Value::Null);
            let text = match contents {
                Value::String(text) => text,
                Value::Object(object) => object.get("value").and_then(Value::as_str).unwrap_or("").to_string(),
                Value::Array(items) => items.iter().map(|item| item.get("value").and_then(Value::as_str).or(item.as_str()).unwrap_or("").to_string()).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            Ok(if text.trim().is_empty() { format!("No type information at {path}:{line}:{column}") } else { text })
        }
        "symbols" => {
            let result = server.request("workspace/symbol", json!({ "query": query })).await?;
            let symbols: Vec<String> = result
                .as_array()
                .map(|items| {
                    items.iter().take(100).filter_map(|item| {
                        let name = item.get("name")?.as_str()?;
                        let location = format_locations(item.get("location")?, &server.root).into_iter().next()?;
                        Some(format!("{name} — {location}"))
                    }).collect()
                })
                .unwrap_or_default();
            Ok(if symbols.is_empty() { format!("No symbols match '{query}'") } else { symbols.join("\n") })
        }
        other => anyhow::bail!("unknown lsp action '{other}'; use diagnostics, definition, type_definition, implementation, references, hover or symbols"),
    }
}

pub(crate) fn workspace_root_for(file: &Path) -> PathBuf {
    let directory = file.parent().unwrap_or(file);
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(directory)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        .unwrap_or_else(|| directory.to_path_buf())
}

pub(crate) async fn execute_tool(args: &Value) -> Result<String> {
    let action = args.get("action").and_then(Value::as_str).context("action is required")?;
    let raw_path = args.get("path").and_then(Value::as_str).context("path is required (the file to ask about)")?;
    let path = std::env::current_dir()?.join(raw_path);
    let path = std::fs::canonicalize(&path).unwrap_or(path);
    let root = args
        .get("root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root_for(&path));
    let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
    let number = |key: &str, fallback: u64| args.get(key).and_then(Value::as_u64).unwrap_or(fallback);
    query(
        &root,
        action,
        &relative,
        number("line", 1) as u32,
        number("column", 1) as u32,
        args.get("query").and_then(Value::as_str).unwrap_or(""),
        number("wait_seconds", 20),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_route_to_the_right_server_and_language_id() {
        assert_eq!(server_for("src/main.rs").map(|server| server.0), Some("rust-analyzer"));
        assert_eq!(server_for("web/App.tsx").map(|server| server.0), Some("typescript-language-server"));
        assert_eq!(language_id("web/App.tsx", "typescript"), "typescriptreact");
        assert!(server_for("README.md").is_none());
    }

    #[test]
    fn frames_carry_the_exact_byte_length_of_the_utf8_body() {
        let frame = String::from_utf8(encode_frame(&json!({ "text": "ź" }))).unwrap();
        assert!(frame.starts_with("Content-Length: 13\r\n\r\n"));
    }

    #[test]
    fn locations_and_diagnostics_read_like_compiler_output_relative_to_the_workspace() {
        let root = Path::new("/repo");
        let result = json!([{ "uri": "file:///repo/src/lib.rs", "range": { "start": { "line": 9, "character": 4 } } }, { "targetUri": "file:///repo/src/a.rs", "targetSelectionRange": { "start": { "line": 0, "character": 0 } } }]);
        assert_eq!(format_locations(&result, root), vec!["src/lib.rs:10:5", "src/a.rs:1:1"]);
        let diagnostics = vec![json!({ "severity": 1, "message": "mismatched types\nexpected i32", "range": { "start": { "line": 2, "character": 7 } } })];
        assert_eq!(format_diagnostics(&diagnostics), vec!["3:8 error: mismatched types"]);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rust_analyzer_reports_a_type_error_and_finds_a_definition_across_calls() {
        if std::process::Command::new("rust-analyzer").arg("--version").output().map(|output| !output.status.success()).unwrap_or(true) {
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"hub_probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn helper() -> i32 {\n    1\n}\n\npub fn broken() -> i32 {\n    let value: i32 = \"text\";\n    helper() + value\n}\n").unwrap();
        let diagnostics = query(root.path(), "diagnostics", "src/lib.rs", 1, 1, "", 90).await.unwrap();
        assert!(diagnostics.contains("6:") && diagnostics.contains("error"), "{diagnostics}");
        let definition = query(root.path(), "definition", "src/lib.rs", 7, 6, "", 5).await.unwrap();
        assert_eq!(definition, "src/lib.rs:1:8");
    }
}
