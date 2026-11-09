use super::*;
pub(crate) fn emit_workflow_notice_for_tool(
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) {
    if thread_id.trim().is_empty() {
        return;
    }

    let (kind, message, details) = match tool_name {
        tool_names::UPDATE_TODO => {
            let count = args
                .get("todos")
                .or_else(|| args.get("items"))
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0);
            (
                "plan-mode",
                format!("Agent updated plan mode with {count} todo item(s)."),
                None,
            )
        }
        tool_names::UPDATE_BROWSER_PROFILE_HEALTH => {
            let profile_id = args
                .get("profile_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown-profile");
            let health_state = args
                .get("health_state")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("unknown");
            let failure_reason = args
                .get("last_auth_failure_reason")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty());

            let message = match health_state {
                "repair_needed" => Some(format!(
                    "Browser profile `{profile_id}` needs repair{}",
                    failure_reason
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                )),
                "repair_in_progress" => Some(format!(
                    "Browser profile `{profile_id}` repair is in progress{}",
                    failure_reason
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                )),
                "corrupted" => Some(format!(
                    "Browser profile `{profile_id}` is corrupted{}",
                    failure_reason
                        .map(|reason| format!(": {reason}"))
                        .unwrap_or_default()
                )),
                _ => None,
            };

            let Some(message) = message else {
                return;
            };

            (
                "browser-profile-repair",
                message,
                Some(
                    serde_json::json!({
                        "profile_id": profile_id,
                        "health_state": health_state,
                        "last_auth_failure_reason": failure_reason,
                    })
                    .to_string(),
                ),
            )
        }
        tool_names::UPDATE_MEMORY => (
            "memory-updated",
            "Agent updated persistent memory.".to_string(),
            Some(args.to_string()),
        ),
        tool_names::READ_MEMORY | tool_names::READ_USER | tool_names::READ_SOUL => (
            "memory-consulted",
            format!("Agent consulted persistent memory via {tool_name}."),
            Some(args.to_string()),
        ),
        tool_names::LIST_TOOLS | tool_names::TOOL_SEARCH => (
            "tool-catalog",
            format!("Agent inspected available tools via {tool_name}."),
            Some(args.to_string()),
        ),
        tool_names::DISCOVER_GUIDELINES
        | tool_names::LIST_GUIDELINES
        | tool_names::READ_GUIDELINE => (
            "guideline-consulted",
            format!("Agent consulted local guidelines via {tool_name}."),
            Some(args.to_string()),
        ),
        tool_names::DISCOVER_SKILLS | tool_names::LIST_SKILLS | tool_names::READ_SKILL => (
            "skill-consulted",
            format!("Agent consulted local skills via {tool_name}."),
            Some(args.to_string()),
        ),
        tool_names::RUN_WORKFLOW_PACK => (
            "workflow-pack-run",
            "Agent executed a canonical workflow pack.".to_string(),
            Some(args.to_string()),
        ),
        tool_names::ONECONTEXT_SEARCH
        | tool_names::SESSION_SEARCH
        | tool_names::AGENT_QUERY_MEMORY => (
            "history-consulted",
            format!("Agent consulted history via {tool_name}."),
            Some(args.to_string()),
        ),
        tool_names::SEMANTIC_QUERY => (
            "semantic-query",
            "Agent consulted local workspace semantics.".to_string(),
            Some(args.to_string()),
        ),
        _ => return,
    };

    let _ = event_tx.send(AgentEvent::WorkflowNotice {
        thread_id: thread_id.to_string(),
        kind: kind.to_string(),
        message,
        details,
    });
}

pub(crate) fn collect_skill_documents(
    dir: &std::path::Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_skill_documents(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let is_md = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if is_md {
            out.push(path);
        }
    }

    Ok(())
}

pub(crate) fn resolve_skill_path(
    skills_root: &std::path::Path,
    skill: &str,
    variant: Option<&SkillVariantRecord>,
) -> Result<std::path::PathBuf> {
    validate_read_path(skill)?;
    let root_canonical = std::fs::canonicalize(skills_root).unwrap_or(skills_root.to_path_buf());

    if let Some(variant) = variant {
        let (candidate, _) = crate::agent::skill_recommendation::resolve_skill_document_path(
            skills_root,
            &variant.relative_path,
        );
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let direct_candidate = std::path::Path::new(skill);
    if direct_candidate.components().count() > 1 || direct_candidate.is_absolute() {
        let candidate = if direct_candidate.is_absolute() {
            direct_candidate.to_path_buf()
        } else {
            skills_root.join(direct_candidate)
        };
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    files.retain(|path| !crate::agent::skill_library::is_skill_path_disabled(skills_root, path));
    let normalized = skill.to_lowercase();

    files.sort();
    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem == normalized || relative.to_lowercase() == normalized {
            return Ok(path.clone());
        }
    }

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.contains(&normalized) || relative.to_lowercase().contains(&normalized) {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "skill '{}' was not found under {}",
        skill,
        skills_root.display()
    )
}

pub(crate) async fn sync_skill_catalog(
    skills_root: &std::path::Path,
    history: &HistoryStore,
) -> Result<Vec<SkillVariantRecord>> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    files.retain(|path| !crate::agent::skill_library::is_skill_path_disabled(skills_root, path));
    let mut entries = Vec::new();
    for path in files {
        if let Ok(record) = history.register_skill_document(&path).await {
            entries.push(record);
        }
    }
    entries.sort_by(|left, right| {
        left.skill_name
            .cmp(&right.skill_name)
            .then_with(|| left.variant_name.cmp(&right.variant_name))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(entries)
}

pub(crate) async fn resolve_skill_context_tags(
    workspace_root: Option<&PathBuf>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
) -> Vec<String> {
    let root = if let Some(session_id) = session_id {
        let sessions = session_manager.list().await;
        sessions
            .iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.cwd.clone())
            .map(PathBuf::from)
    } else {
        None
    }
    .or_else(|| workspace_root.cloned())
    .or_else(|| std::env::current_dir().ok());

    root.filter(|path| path.is_dir())
        .map(|path| infer_workspace_context_tags(&path))
        .unwrap_or_default()
}

const FETCH_URL_PYTHON_SCRIPT: &str = r##"
import json, mimetypes, re, sys, urllib.error, urllib.request
from html.parser import HTMLParser


class MarkdownExtractor(HTMLParser):
    SKIP = {"script", "style", "noscript", "template", "head", "svg", "iframe"}
    HEADINGS = {"h1", "h2", "h3", "h4", "h5", "h6"}

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.out = []
        self.skip_depth = 0
        self.title = ""
        self.in_title = False
        self.in_pre = False
        self.href = None
        self.list_stack = []
        self.no_space = False

    def open_inline(self, marker):
        if self.out and not self.out[-1].endswith((" ", "\n", "(")):
            self.out.append(" ")
        self.out.append(marker)
        self.no_space = True

    def handle_starttag(self, tag, attrs):
        if tag == "title":
            self.in_title = True
            return
        if tag in self.SKIP:
            self.skip_depth += 1
            return
        if self.skip_depth:
            return
        a = dict(attrs)
        if tag in self.HEADINGS:
            self.out.append("\n\n" + "#" * int(tag[1]) + " ")
        elif tag == "p":
            self.out.append("\n\n")
        elif tag == "br":
            self.out.append("\n")
        elif tag in ("ul", "ol"):
            self.list_stack.append(tag)
            self.out.append("\n")
        elif tag == "li":
            marker = "1." if self.list_stack and self.list_stack[-1] == "ol" else "-"
            indent = "  " * max(len(self.list_stack) - 1, 0)
            self.out.append("\n" + indent + marker + " ")
        elif tag == "a":
            self.href = a.get("href")
        elif tag in ("strong", "b"):
            self.open_inline("**")
        elif tag in ("em", "i"):
            self.open_inline("*")
        elif tag == "code" and not self.in_pre:
            self.open_inline("`")
        elif tag == "pre":
            self.in_pre = True
            self.out.append("\n\n```\n")
        elif tag == "blockquote":
            self.out.append("\n\n> ")
        elif tag in ("td", "th"):
            self.out.append(" | ")
        elif tag == "img":
            alt = a.get("alt") or ""
            src = a.get("src") or ""
            if src:
                self.out.append("![" + alt + "](" + src + ")")
        elif tag in ("div", "section", "article", "header", "footer", "table", "tr"):
            self.out.append("\n")

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False
            return
        if tag in self.SKIP:
            self.skip_depth = max(self.skip_depth - 1, 0)
            return
        if self.skip_depth:
            return
        if tag in ("ul", "ol"):
            if self.list_stack:
                self.list_stack.pop()
            self.out.append("\n")
        elif tag == "a":
            if self.href:
                self.out.append(" (" + self.href + ")")
            self.href = None
        elif tag in ("strong", "b"):
            self.out.append("**")
        elif tag in ("em", "i"):
            self.out.append("*")
        elif tag == "code" and not self.in_pre:
            self.out.append("`")
        elif tag == "pre":
            self.in_pre = False
            self.out.append("\n```\n")
        elif tag in self.HEADINGS or tag in ("p", "blockquote"):
            self.out.append("\n")

    def handle_data(self, data):
        if self.in_title:
            self.title += data.strip()
            return
        if self.skip_depth:
            return
        if self.in_pre:
            self.out.append(data)
            return
        text = " ".join(data.split())
        if not text:
            return
        if self.no_space:
            self.no_space = False
        elif self.out and not self.out[-1].endswith((" ", "\n", "(", "#", ">")):
            self.out.append(" ")
        self.out.append(text)


def main():
    url, out_base, timeout_s, preview_cap = (
        sys.argv[1],
        sys.argv[2],
        float(sys.argv[3]),
        int(sys.argv[4]),
    )
    if not url.lower().startswith(("http://", "https://")):
        print(json.dumps({"error": "only http(s) URLs are supported"}))
        return
    request = urllib.request.Request(url, headers={"User-Agent": "zorai-agent/0.1"})
    try:
        response = urllib.request.urlopen(request, timeout=timeout_s)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        status = getattr(response, "status", None) or getattr(response, "code", 0)
        content_type = response.headers.get_content_type()
        charset = response.headers.get_content_charset() or "utf-8"
        data = response.read()

    title = ""
    converted = False
    if content_type in ("text/html", "application/xhtml+xml"):
        extractor = MarkdownExtractor()
        extractor.feed(data.decode(charset, errors="replace"))
        extractor.close()
        text = re.sub(r"\n{3,}", "\n\n", "".join(extractor.out)).strip() + "\n"
        title = extractor.title
        header = "<!-- source: " + url + " -->\n\n"
        if title:
            header += "# " + title + "\n\n"
        text = header + text
        path = out_base + ".md"
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        converted = True
        preview = text
    elif content_type.startswith("text/") or "json" in content_type or "xml" in content_type or "javascript" in content_type:
        text = data.decode(charset, errors="replace")
        ext = ".json" if "json" in content_type else (".xml" if "xml" in content_type else ".txt")
        path = out_base + ext
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        preview = text
    else:
        ext = mimetypes.guess_extension(content_type) or ".bin"
        path = out_base + ext
        with open(path, "wb") as handle:
            handle.write(data)
        preview = ""

    print(
        json.dumps(
            {
                "path": path,
                "status": status,
                "content_type": content_type,
                "bytes": len(data),
                "title": title,
                "converted": converted,
                "preview": preview[:preview_cap],
            }
        )
    )


main()
"##;

async fn resolve_fetch_python_binary() -> Option<&'static str> {
    for candidate in ["python3", "python"] {
        let available = tokio::process::Command::new(candidate)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success());
        if available {
            return Some(candidate);
        }
    }
    None
}

fn fetch_inventory_file_base(inventory_dir: &std::path::Path, url: &str) -> std::path::PathBuf {
    let stem: String = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut collapsed = String::with_capacity(stem.len());
    for c in stem.chars() {
        if c == '-' && collapsed.ends_with('-') {
            continue;
        }
        collapsed.push(c);
    }
    let trimmed: String = collapsed.trim_matches('-').chars().take(60).collect();
    let stem = if trimmed.is_empty() {
        "page".to_string()
    } else {
        trimmed
    };
    inventory_dir.join(format!("fetch-{stem}-{}", crate::agent::now_millis()))
}

fn fetch_preview_cap(args: &serde_json::Value) -> usize {
    args.get("max_length")
        .and_then(|value| value.as_u64())
        .map(|value| (value as usize).min(4_000))
        .unwrap_or(800)
}

#[derive(serde::Deserialize)]
struct PythonFetchOutcome {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    status: u32,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    converted: bool,
    #[serde(default)]
    preview: String,
}

fn format_saved_fetch_result(
    url: &str,
    path: &str,
    status: Option<u32>,
    content_type: &str,
    bytes: u64,
    title: &str,
    converted: bool,
    preview: &str,
) -> String {
    let mut lines = vec![format!("Fetched {url}"), format!("- saved_to: {path}")];
    if let Some(status) = status {
        lines.push(format!("- http_status: {status}"));
    }
    lines.push(format!(
        "- content_type: {content_type}{}",
        if converted {
            " (converted to markdown)"
        } else {
            ""
        }
    ));
    lines.push(format!("- bytes: {bytes}"));
    if !title.is_empty() {
        lines.push(format!("- title: {title}"));
    }
    let mut rendered = lines.join("\n");
    if !preview.trim().is_empty() {
        rendered.push_str(&format!("\n\nPreview:\n{}", preview.trim_end()));
    }
    rendered.push_str("\n\nUse read_file with offset/limit on saved_to for the full content.");
    rendered
}

async fn run_python_fetch(
    python_bin: &str,
    request: &FetchUrlRequest,
    out_base: &std::path::Path,
    preview_cap: usize,
) -> Result<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(request.timeout_seconds.saturating_add(5)),
        tokio::process::Command::new(python_bin)
            .arg("-c")
            .arg(FETCH_URL_PYTHON_SCRIPT)
            .arg(&request.url)
            .arg(out_base.as_os_str())
            .arg(request.timeout_seconds.to_string())
            .arg(preview_cap.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "fetch_url timed out after {} seconds",
            request.timeout_seconds
        )
    })??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("python fetch failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let payload = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .ok_or_else(|| anyhow::anyhow!("python fetch produced no result payload"))?;
    let outcome: PythonFetchOutcome = serde_json::from_str(payload.trim())?;
    if let Some(error) = outcome.error {
        anyhow::bail!("python fetch failed: {error}");
    }
    Ok(format_saved_fetch_result(
        &request.url,
        &outcome.path,
        Some(outcome.status),
        &outcome.content_type,
        outcome.bytes,
        &outcome.title,
        outcome.converted,
        &outcome.preview,
    ))
}

pub(crate) async fn execute_fetch_url(
    args: &serde_json::Value,
    agent: &AgentEngine,
    http_client: &reqwest::Client,
    browse_provider: &str,
    thread_id: &str,
) -> Result<String> {
    let python_bin = resolve_fetch_python_binary().await;
    execute_fetch_url_with_python(
        args,
        agent,
        http_client,
        browse_provider,
        thread_id,
        python_bin,
    )
    .await
}

pub(crate) async fn execute_fetch_url_with_python(
    args: &serde_json::Value,
    agent: &AgentEngine,
    http_client: &reqwest::Client,
    browse_provider: &str,
    thread_id: &str,
    python_bin: Option<&str>,
) -> Result<String> {
    let request = fetch_url_request(args)?;
    let preview_cap = fetch_preview_cap(args);
    let inventory_dir = zorai_protocol::thread_inventory_dir(agent.history.data_root(), thread_id);
    tokio::fs::create_dir_all(&inventory_dir)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "create thread inventory dir {}: {error}",
                inventory_dir.display()
            )
        })?;
    let out_base = fetch_inventory_file_base(&inventory_dir, &request.url);

    if request.profile_id.is_none() {
        if let Some(python_bin) = python_bin {
            match run_python_fetch(python_bin, &request, &out_base, preview_cap).await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    if is_fetch_url_timeout_error(&error) {
                        return Err(error);
                    }
                    tracing::warn!(
                        "python fetch_url failed, falling back to legacy fetch: {error}"
                    );
                }
            }
        }
    }

    let content = execute_fetch_url_legacy(agent, http_client, browse_provider, &request).await?;
    let path = out_base.with_extension("txt");
    tokio::fs::write(&path, &content)
        .await
        .map_err(|error| anyhow::anyhow!("write fetch result {}: {error}", path.display()))?;
    let preview: String = content.chars().take(preview_cap).collect();
    Ok(format_saved_fetch_result(
        &request.url,
        &path.display().to_string(),
        None,
        "text (legacy fetch, tags stripped)",
        content.len() as u64,
        "",
        false,
        &preview,
    ))
}

async fn execute_fetch_url_legacy(
    agent: &AgentEngine,
    http_client: &reqwest::Client,
    browse_provider: &str,
    request: &FetchUrlRequest,
) -> Result<String> {
    let mut request = request.clone();
    request.max_length = usize::MAX;
    let profile = match request.profile_id.as_deref() {
        Some(profile_id) => Some(resolve_fetch_browser_profile(agent, profile_id).await?),
        None => None,
    };
    let browser_preference = profile
        .as_ref()
        .and_then(|row| row.browser_kind.as_deref())
        .unwrap_or(browse_provider);
    let browser = if profile.is_some() {
        resolve_browser_for_profile(browser_preference)
    } else {
        resolve_browser(browser_preference)
    };
    let profile_dir = profile.as_ref().map(|row| row.profile_dir.clone());

    if let Some(profile) = profile.as_ref() {
        if browser.is_none() {
            anyhow::bail!(
                "browser profile '{}' requires a compatible headless browser, but none is available for '{}'",
                profile.profile_id,
                browser_preference,
            );
        }
    }

    let content =
        execute_fetch_url_request_with_runner(
            request,
            browser.is_some(),
            move |url, timeout_seconds| async move {
                let browser = browser
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("no headless browser available"))?;
                fetch_with_headless_browser(browser, &url, timeout_seconds, profile_dir.as_deref())
                    .await
            },
            |url, timeout_seconds| async move {
                fetch_raw_http(http_client, &url, timeout_seconds).await
            },
        )
        .await?;

    if let Some(profile) = profile.as_ref() {
        record_browser_profile_fetch_success(agent, profile).await?;
    }

    Ok(content)
}

pub(crate) async fn execute_fetch_url_with_runner<BrowserRunner, BrowserFut, HttpRunner, HttpFut>(
    args: &serde_json::Value,
    browser_available: bool,
    browser_runner: BrowserRunner,
    http_runner: HttpRunner,
) -> Result<String>
where
    BrowserRunner: FnOnce(String, u64) -> BrowserFut,
    BrowserFut: Future<Output = Result<String>>,
    HttpRunner: FnOnce(String, u64) -> HttpFut,
    HttpFut: Future<Output = Result<String>>,
{
    let request = fetch_url_request(args)?;
    execute_fetch_url_request_with_runner(request, browser_available, browser_runner, http_runner)
        .await
}

async fn execute_fetch_url_request_with_runner<BrowserRunner, BrowserFut, HttpRunner, HttpFut>(
    request: FetchUrlRequest,
    browser_available: bool,
    browser_runner: BrowserRunner,
    http_runner: HttpRunner,
) -> Result<String>
where
    BrowserRunner: FnOnce(String, u64) -> BrowserFut,
    BrowserFut: Future<Output = Result<String>>,
    HttpRunner: FnOnce(String, u64) -> HttpFut,
    HttpFut: Future<Output = Result<String>>,
{
    let timeout_seconds = request.timeout_seconds;
    let started = tokio::time::Instant::now();
    let max_length = request.max_length;
    let url = request.url;

    let remaining_budget = |started: tokio::time::Instant| -> Result<std::time::Duration> {
        std::time::Duration::from_secs(timeout_seconds)
            .checked_sub(started.elapsed())
            .ok_or_else(|| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))
    };

    let raw_html = if browser_available {
        match tokio::time::timeout(
            remaining_budget(started)?,
            browser_runner(url.clone(), timeout_seconds),
        )
        .await
        .map_err(|_| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))
        {
            Ok(Ok(html)) => html,
            Ok(Err(e)) => {
                if is_fetch_url_timeout_error(&e) {
                    return Err(anyhow::anyhow!(
                        "fetch_url timed out after {timeout_seconds} seconds"
                    ));
                }
                tracing::warn!("headless browser fetch failed, falling back to HTTP: {e}");
                tokio::time::timeout(
                    remaining_budget(started)?,
                    http_runner(url.clone(), timeout_seconds),
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds")
                })??
            }
            Err(err) => return Err(err),
        }
    } else {
        tokio::time::timeout(
            remaining_budget(started)?,
            http_runner(url.clone(), timeout_seconds),
        )
        .await
        .map_err(|_| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))??
    };

    let stripped = strip_html_tags(&raw_html);
    let truncated = if stripped.len() > max_length {
        format!(
            "{}...\n\n(truncated, {} chars total)",
            &stripped[..max_length],
            stripped.len()
        )
    } else {
        stripped
    };

    Ok(truncated)
}

fn is_fetch_url_timeout_error(error: &anyhow::Error) -> bool {
    error.to_string().to_ascii_lowercase().contains("timed out")
}

async fn fetch_raw_http(
    http_client: &reqwest::Client,
    url: &str,
    timeout_seconds: u64,
) -> Result<String> {
    let resp = http_client
        .get(url)
        .header("User-Agent", "zorai-agent/0.1")
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    Ok(format!("HTTP {status}\n\n{text}"))
}

/// Detected headless browser binary and its args for dump-dom mode.
pub(crate) struct HeadlessBrowser {
    pub(crate) kind: &'static str,
    pub(crate) bin: String,
    /// Extra args to produce DOM text on stdout for a given URL.
    pub(crate) args_prefix: Vec<String>,
    pub(crate) profile_dir_arg_prefix: Option<&'static str>,
}

/// Resolve which headless browser to use.
/// "auto" tries lightpanda → chrome → chromium → none.
fn resolve_browser(preference: &str) -> Option<HeadlessBrowser> {
    match preference {
        "none" | "off" | "" => None,
        "lightpanda" => detect_lightpanda(),
        "chrome" | "chromium" => detect_chrome(),
        "auto" | _ => detect_lightpanda().or_else(detect_chrome),
    }
}

fn resolve_browser_for_profile(preference: &str) -> Option<HeadlessBrowser> {
    match preference {
        "none" | "off" | "" => None,
        "lightpanda" => detect_lightpanda(),
        "chrome" | "chromium" => detect_chrome(),
        "auto" | _ => detect_chrome().or_else(detect_lightpanda),
    }
}

pub(crate) fn detect_lightpanda() -> Option<HeadlessBrowser> {
    which::which("lightpanda").ok().map(|path| HeadlessBrowser {
        kind: "lightpanda",
        bin: path.to_string_lossy().to_string(),
        args_prefix: vec![
            "fetch".to_string(),
            "--output".to_string(),
            "dom-text".to_string(),
        ],
        profile_dir_arg_prefix: None,
    })
}

pub(crate) fn detect_chrome() -> Option<HeadlessBrowser> {
    let candidates = [
        "google-chrome-stable",
        "google-chrome",
        "chromium-browser",
        "chromium",
    ];
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return Some(HeadlessBrowser {
                kind: "chrome",
                bin: path.to_string_lossy().to_string(),
                args_prefix: vec![
                    "--headless=new".to_string(),
                    "--no-sandbox".to_string(),
                    "--disable-gpu".to_string(),
                    "--dump-dom".to_string(),
                ],
                profile_dir_arg_prefix: Some("--user-data-dir="),
            });
        }
    }
    None
}

async fn fetch_with_headless_browser(
    browser: &HeadlessBrowser,
    url: &str,
    timeout_seconds: u64,
    profile_dir: Option<&str>,
) -> Result<String> {
    let args = build_headless_browser_args(browser, url, profile_dir)?;

    let child = tokio::process::Command::new(&browser.bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!("headless browser fetch timed out after {timeout_seconds} seconds")
    })??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "headless browser exited with {}: {}",
            output.status,
            &stderr[..stderr.len().min(200)]
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn build_headless_browser_args(
    browser: &HeadlessBrowser,
    url: &str,
    profile_dir: Option<&str>,
) -> Result<Vec<String>> {
    let mut args = browser.args_prefix.clone();

    if let Some(profile_dir) = profile_dir {
        let profile_dir = profile_dir.trim();
        if profile_dir.is_empty() {
            anyhow::bail!("browser profile directory must not be empty");
        }
        let profile_dir_arg_prefix = browser.profile_dir_arg_prefix.ok_or_else(|| {
            anyhow::anyhow!(
                "headless browser '{}' does not support browser profile directories",
                browser.kind,
            )
        })?;
        args.push(format!("{profile_dir_arg_prefix}{profile_dir}"));
    }

    args.push(url.to_string());
    Ok(args)
}

pub(crate) async fn resolve_fetch_browser_profile(
    agent: &AgentEngine,
    profile_id: &str,
) -> Result<crate::history::BrowserProfileRow> {
    agent
        .history
        .detect_and_classify_expired_profiles(crate::agent::now_millis())
        .await?;

    let profile = agent
        .history
        .get_browser_profile(profile_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("browser profile not found: {profile_id}"))?;

    match profile.health_state.as_str() {
        "expired" | "corrupted" | "repair_needed" | "repair_in_progress" | "retired" => {
            anyhow::bail!(
                "browser profile '{}' is not usable for fetch_url while in '{}' state",
                profile.profile_id,
                profile.health_state,
            );
        }
        _ => {}
    }

    Ok(profile)
}

pub(crate) async fn record_browser_profile_fetch_success(
    agent: &AgentEngine,
    profile: &crate::history::BrowserProfileRow,
) -> Result<()> {
    let health_state = crate::agent::types::BrowserProfileHealth::from_str(&profile.health_state)
        .ok_or_else(|| {
        anyhow::anyhow!(
            "invalid persisted browser profile health state: {}",
            profile.health_state
        )
    })?;
    let now = crate::agent::now_millis();
    let updated = crate::agent::types::BrowserProfile {
        profile_id: profile.profile_id.clone(),
        label: profile.label.clone(),
        profile_dir: profile.profile_dir.clone(),
        browser_kind: profile.browser_kind.clone(),
        workspace_id: profile.workspace_id.clone(),
        health_state,
        created_at: profile.created_at,
        updated_at: now,
        last_used_at: Some(now),
        last_auth_success_at: profile.last_auth_success_at,
        last_auth_failure_at: profile.last_auth_failure_at,
        last_auth_failure_reason: profile.last_auth_failure_reason.clone(),
    };
    agent.history.upsert_browser_profile(&updated).await
}
