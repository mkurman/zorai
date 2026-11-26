use super::*;
pub(crate) fn build_search_files_rg_args(request: &SearchFilesRequest) -> Vec<String> {
    let mut cmd_args = vec![
        "--line-number".to_string(),
        "--with-filename".to_string(),
        "--color=never".to_string(),
    ];

    if let Some(file_pattern) = &request.file_pattern {
        cmd_args.push(format!("--glob={file_pattern}"));
    }

    if !request.regex {
        cmd_args.push("--fixed-strings".to_string());
    }

    cmd_args.push("--".to_string());
    cmd_args.push(request.pattern.clone());
    cmd_args.push(request.path.clone());
    cmd_args
}

pub(crate) fn web_search_request(args: &serde_json::Value) -> Result<WebSearchRequest> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    if args
        .get("timeout_seconds")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(anyhow::anyhow!(
            "'timeout_seconds' must be a non-negative integer"
        ));
    }

    Ok(WebSearchRequest {
        query: query.to_string(),
        max_results: args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5),
        timeout_seconds: daemon_tool_timeout_seconds(tool_names::WEB_SEARCH, args),
    })
}

pub(crate) fn fetch_url_request(args: &serde_json::Value) -> Result<FetchUrlRequest> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'url' argument"))?
        .trim();

    if url.is_empty() {
        return Err(anyhow::anyhow!("'url' must not be empty"));
    }

    if args
        .get("timeout_seconds")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err(anyhow::anyhow!(
            "'timeout_seconds' must be a non-negative integer"
        ));
    }

    Ok(FetchUrlRequest {
        url: url.to_string(),
        max_length: args
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(10_000) as usize,
        timeout_seconds: daemon_tool_timeout_seconds(tool_names::FETCH_URL, args),
        profile_id: args
            .get("profile_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    })
}

pub(crate) async fn run_search_files_subprocess(
    request: SearchFilesRequest,
) -> Result<SearchFilesCommandOutput> {
    let mut command = tokio::process::Command::new(SEARCH_FILES_PROGRAM);
    configure_search_files_command(&mut command, &request);
    run_search_files_command_bounded(command, request.max_results).await
}

fn configure_search_files_command(
    command: &mut tokio::process::Command,
    request: &SearchFilesRequest,
) {
    if request.file_pattern.is_some() {
        if let Some((cwd, search_path)) = search_files_command_root_override(&request.path) {
            command.current_dir(cwd);
            let mut rooted_request = request.clone();
            rooted_request.path = search_path;
            command.args(build_search_files_rg_args(&rooted_request));
            return;
        }
    }

    command.args(build_search_files_rg_args(request));
}

fn search_files_command_root_override(path: &str) -> Option<(PathBuf, String)> {
    let path = PathBuf::from(path);
    let metadata = std::fs::metadata(&path).ok()?;

    if metadata.is_dir() {
        return Some((path, ".".to_string()));
    }

    let parent = path.parent()?.to_path_buf();
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    Some((parent, file_name))
}

fn search_files_success_exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
}

pub(crate) async fn run_search_files_command_bounded(
    mut command: tokio::process::Command,
    max_results: u64,
) -> Result<SearchFilesCommandOutput> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .context("failed to spawn search_files subprocess")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture search_files stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("failed to capture search_files stderr"))?;

    let stderr_task = tokio::spawn(async move {
        let mut stderr = stderr;
        read_search_files_stream_bounded(&mut stderr, SEARCH_FILES_MAX_STDERR_BYTES).await
    });

    let mut line_buffers = Vec::new();
    let max_results = max_results as usize;
    let mut truncated = false;
    let status = loop {
        if line_buffers.len() < max_results {
            match read_search_files_line(&mut stdout, SEARCH_FILES_MAX_LINE_BYTES).await {
                Ok(Some(line)) => {
                    line_buffers.push(line);
                    continue;
                }
                Ok(None) => break child.wait().await?,
                Err(error) => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let _ = stderr_task.await;
                    return Err(error);
                }
            }
        }

        let mut child_wait = Box::pin(child.wait());
        tokio::select! {
            next_line = read_search_files_line(&mut stdout, SEARCH_FILES_MAX_LINE_BYTES) => {
                match next_line {
                    Ok(Some(_)) => {
                        truncated = true;
                        drop(child_wait);
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        break search_files_success_exit_status();
                    }
                    Ok(None) => break child_wait.await?,
                    Err(error) => {
                        drop(child_wait);
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        let _ = stderr_task.await;
                        return Err(error);
                    }
                }
            }
            status = &mut child_wait => {
                let status = status?;
                match read_search_files_line(&mut stdout, SEARCH_FILES_MAX_LINE_BYTES).await {
                    Ok(Some(_)) => {
                        truncated = true;
                        break status;
                    }
                    Ok(None) => break status,
                    Err(error) => {
                        let _ = stderr_task.await;
                        return Err(error);
                    }
                }
            }
        }
    };

    if truncated {
        let stderr = stderr_task.await.map_err(|error| {
            anyhow::anyhow!("failed to join search_files stderr task: {error}")
        })??;
        let stderr_trimmed = String::from_utf8_lossy(&stderr).trim().to_string();
        if !stderr_trimmed.is_empty() {
            return Err(anyhow::anyhow!("search failed: {stderr_trimmed}"));
        }
        return Ok(SearchFilesCommandOutput {
            status: search_files_success_exit_status(),
            stdout: render_search_files_lines(&line_buffers),
            stderr,
            truncated: true,
        });
    }

    let stderr = stderr_task
        .await
        .map_err(|error| anyhow::anyhow!("failed to join search_files stderr task: {error}"))??;
    Ok(SearchFilesCommandOutput {
        status,
        stdout: render_search_files_lines(&line_buffers),
        stderr,
        truncated: false,
    })
}

async fn read_search_files_line<R>(reader: &mut R, max_line_bytes: usize) -> Result<Option<Vec<u8>>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        match reader.read(&mut byte).await? {
            0 if line.is_empty() => return Ok(None),
            0 => return Ok(Some(line)),
            _ => {
                if byte[0] == b'\n' {
                    return Ok(Some(line));
                }
                if line.len() >= max_line_bytes {
                    return Err(anyhow::anyhow!(
                        "search output line exceeded {} bytes",
                        max_line_bytes
                    ));
                }
                line.push(byte[0]);
            }
        }
    }
}

async fn read_search_files_stream_bounded<R>(reader: &mut R, max_bytes: usize) -> Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];

    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }

        let remaining = max_bytes.saturating_sub(buffer.len());
        let bytes_to_copy = remaining.min(read);
        if bytes_to_copy > 0 {
            buffer.extend_from_slice(&chunk[..bytes_to_copy]);
        }
    }

    Ok(buffer)
}

pub(crate) fn resolve_search_files_path(path: &str) -> Result<String> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return Ok(path.to_string_lossy().into_owned());
    }

    Ok(std::env::current_dir()
        .context("failed to resolve current directory for search_files path")?
        .join(path)
        .to_string_lossy()
        .into_owned())
}

fn render_search_files_lines(lines: &[Vec<u8>]) -> Vec<u8> {
    lines
        .iter()
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}

pub(crate) async fn run_search_files_command(
    mut command: tokio::process::Command,
) -> Result<std::process::Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = command
        .spawn()
        .context("failed to spawn search_files subprocess")?;
    child.wait_with_output().await.map_err(Into::into)
}

pub(crate) async fn execute_search_files_with_runner<F, Fut>(
    args: &serde_json::Value,
    runner: F,
) -> Result<String>
where
    F: FnOnce(SearchFilesRequest) -> Fut,
    Fut: Future<Output = Result<SearchFilesCommandOutput>>,
{
    let request = search_files_request(args)?;
    let timeout_seconds = request.timeout_seconds;
    let max_results = request.max_results;
    let fallback_request = request.clone();

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        runner(request),
    )
    .await
    .map_err(|_| anyhow::anyhow!("search timed out after {timeout_seconds} seconds"))?;

    let output = match output {
        Ok(output) => output,
        Err(error) if search_files_runner_error_is_missing_program(&error) => {
            return native_search_files(fallback_request).await;
        }
        Err(error) => return Err(error),
    };

    let truncated_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.truncated && !truncated_stderr.is_empty() {
        if search_files_stderr_looks_like_invalid_regex(&truncated_stderr) {
            return Err(anyhow::anyhow!("invalid regex: {truncated_stderr}"));
        }
        return Err(anyhow::anyhow!("search failed: {truncated_stderr}"));
    }

    match output.status.code() {
        Some(1) => return Ok("No matches found.".into()),
        Some(0) => {}
        Some(code) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if search_files_stderr_looks_like_invalid_regex(&stderr) {
                return Err(anyhow::anyhow!("invalid regex: {stderr}"));
            }
            if stderr.is_empty() {
                return Err(anyhow::anyhow!("search failed with rg exit code {code}"));
            }
            return Err(anyhow::anyhow!("search failed: {stderr}"));
        }
        None => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                return Err(anyhow::anyhow!("search failed: rg terminated by signal"));
            }
            if search_files_stderr_looks_like_invalid_regex(&stderr) {
                return Err(anyhow::anyhow!("invalid regex: {stderr}"));
            }
            return Err(anyhow::anyhow!("search failed: {stderr}"));
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().take(max_results as usize).collect();
    let total = stdout.lines().count();

    if lines.is_empty() {
        Ok("No matches found.".into())
    } else {
        let mut result = lines.join("\n");
        if output.truncated {
            result.push_str("\n\n... (more matches)");
        } else if total > lines.len() {
            result.push_str(&format!("\n\n... ({} more matches)", total - lines.len()));
        }
        Ok(result)
    }
}

async fn native_search_files(request: SearchFilesRequest) -> Result<String> {
    let max_results = request.max_results.max(1) as usize;
    let root = std::path::PathBuf::from(&request.path);
    let hits = tokio::task::spawn_blocking(move || {
        crate::code_intel::search::search_workspace(
            &root,
            &request.pattern,
            &crate::code_intel::search::CodeSearchOptions {
                case_sensitive: true,
                regex: request.regex,
                include: request.file_pattern.clone().into_iter().collect(),
                max_results: Some(max_results + 1),
                ..Default::default()
            },
        )
        .map(|hits| (root, hits))
    })
    .await
    .map_err(|error| anyhow::anyhow!("search failed: {error}"))?
    .map_err(|error| anyhow::anyhow!("invalid regex: {error:#}"))?;
    let (root, hits) = hits;
    if hits.is_empty() {
        return Ok("No matches found.".into());
    }
    let mut result = hits
        .iter()
        .take(max_results)
        .map(|hit| {
            format!(
                "{}:{}:{}",
                root.join(&hit.path).display(),
                hit.line,
                hit.preview
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if hits.len() > max_results {
        result.push_str("\n\n... (more matches)");
    }
    Ok(result)
}

fn search_files_runner_error_is_missing_program(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

fn search_files_stderr_looks_like_invalid_regex(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("regex parse error")
        || lower.contains("invalid regular expression")
        || lower.contains("error parsing regex")
}

pub(crate) async fn run_onecontext_search_subprocess(
    request: OnecontextSearchRequest,
) -> Result<std::process::Output> {
    let mut cmd = tokio::process::Command::new("aline");
    cmd.arg("search")
        .arg(&request.bounded_query)
        .arg("-t")
        .arg(&request.scope)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let child = cmd
        .spawn()
        .context("failed to spawn onecontext search subprocess")?;
    child.wait_with_output().await.map_err(Into::into)
}
