#![allow(dead_code)]

use super::*;

#[cfg(test)]
static TEST_SYNTHESIZE_TOOL_DELAY: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<usize, Duration>>,
> = std::sync::OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliWrapperSynthesisProposal {
    pub tool_name: String,
    pub target: String,
}

async fn cli_command(
    argv: &[String],
    os_sandbox_root: Option<&std::path::Path>,
    allow_network: bool,
) -> Result<tokio::process::Command> {
    let Some(root) = os_sandbox_root else {
        let mut command = tokio::process::Command::new(&argv[0]);
        command.args(&argv[1..]);
        return Ok(command);
    };
    tokio::fs::create_dir_all(root).await?;
    let wrapped = crate::sandbox::detect_sandbox().wrap(
        &crate::pty_session::shell_join(&argv[0], &argv[1..]),
        &root.to_string_lossy(),
        allow_network,
    );
    let mut command = tokio::process::Command::new(&wrapped.program);
    command.args(&wrapped.args);
    Ok(command)
}

pub(super) async fn synthesize_cli_tool(
    target: &str,
    requested_name: Option<&str>,
    os_sandbox_root: Option<&std::path::Path>,
) -> Result<GeneratedToolRecord> {
    let invocation = target
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if invocation.is_empty() {
        anyhow::bail!("empty CLI target");
    }
    validate_safe_cli_invocation(&invocation)?;
    let mut probe = invocation.clone();
    probe.push("--help".to_string());
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        cli_command(&probe, os_sandbox_root, false)
            .await?
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("CLI help timed out")??;
    let help = String::from_utf8_lossy(&output.stdout).to_string();
    let description = help
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or(target)
        .to_string();
    let parameters = parse_cli_help_parameters(&help);
    let name = requested_name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_tool_name(&invocation.join("_")));
    Ok(GeneratedToolRecord {
        id: name.clone(),
        name,
        description,
        kind: GeneratedToolKind::Cli,
        parameters,
        status: "new".to_string(),
        created_at: now_millis(),
        updated_at: now_millis(),
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: Some(GeneratedCliSpec {
            invocation,
            help_source: target.to_string(),
        }),
        openapi: None,
    })
}

pub(super) async fn synthesize_openapi_tool(
    spec_url: &str,
    requested_name: Option<&str>,
    args: &serde_json::Value,
    http_client: &reqwest::Client,
) -> Result<GeneratedToolRecord> {
    let spec = http_client
        .get(spec_url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let operation_id_hint = args.get("operation_id").and_then(|value| value.as_str());
    let (path, method, operation) = select_openapi_operation(&spec, operation_id_hint)?;
    let parameters = operation
        .get("parameters")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GeneratedToolParameter {
                        name: item.get("name")?.as_str()?.to_string(),
                        description: item
                            .get("description")
                            .and_then(|value| value.as_str())
                            .unwrap_or("openapi parameter")
                            .to_string(),
                        cli_flag: None,
                        required: item
                            .get("required")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false),
                        param_type: item
                            .get("schema")
                            .and_then(|value| value.get("type"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("string")
                            .to_string(),
                        location: item
                            .get("in")
                            .and_then(|value| value.as_str())
                            .unwrap_or("query")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let name = requested_name
        .map(ToOwned::to_owned)
        .or_else(|| {
            operation
                .get("operationId")
                .and_then(|value| value.as_str())
                .map(sanitize_tool_name)
        })
        .unwrap_or_else(|| sanitize_tool_name(&format!("{}_{}", method, path.replace('/', "_"))));
    let base_url = spec
        .get("servers")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("url"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .or_else(|| {
            reqwest::Url::parse(spec_url)
                .ok()
                .map(|url| format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default()))
        })
        .unwrap_or_else(|| spec_url.to_string());
    Ok(GeneratedToolRecord {
        id: name.clone(),
        name,
        description: operation
            .get("summary")
            .and_then(|value| value.as_str())
            .unwrap_or("generated openapi tool")
            .to_string(),
        kind: GeneratedToolKind::OpenApi,
        parameters,
        status: "new".to_string(),
        created_at: now_millis(),
        updated_at: now_millis(),
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: None,
        openapi: Some(GeneratedOpenApiSpec {
            spec_url: spec_url.to_string(),
            base_url,
            path,
            method,
            operation_id: operation
                .get("operationId")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
        }),
    })
}

pub(super) async fn run_cli_generated_tool(
    record: &GeneratedToolRecord,
    args: &serde_json::Value,
    sandbox: &ToolSynthesisSandboxConfig,
    os_sandbox_root: Option<&std::path::Path>,
) -> Result<String> {
    if !sandbox.allow_filesystem {
        anyhow::bail!(
            "generated CLI tools require filesystem access; enable tool_synthesis.sandbox.allow_filesystem first"
        );
    }
    let cli = record.cli.as_ref().context("missing CLI spec")?;
    validate_safe_cli_invocation(&cli.invocation)?;
    let mut argv = cli.invocation.clone();
    for parameter in &record.parameters {
        let Some(value) = args.get(&parameter.name) else {
            continue;
        };
        if let Some(flag) = parameter.cli_flag.as_deref() {
            if value.as_bool() == Some(false) {
                continue;
            }
            argv.push(flag.to_string());
            if parameter.param_type != "boolean" {
                argv.push(value_to_command_arg(value));
            }
        }
    }
    let mut command = cli_command(&argv, os_sandbox_root, sandbox.allow_network).await?;
    let output = tokio::time::timeout(
        Duration::from_secs(sandbox.max_execution_time_secs.max(1)),
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("generated tool execution timed out")??;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).to_string();
    }
    cap_output(text, sandbox.max_output_kb)
}

pub(super) async fn run_openapi_generated_tool(
    record: &GeneratedToolRecord,
    args: &serde_json::Value,
    sandbox: &ToolSynthesisSandboxConfig,
    http_client: &reqwest::Client,
) -> Result<String> {
    if !sandbox.allow_network {
        anyhow::bail!(
            "generated OpenAPI tools require network access; enable tool_synthesis.sandbox.allow_network first"
        );
    }
    let spec = record.openapi.as_ref().context("missing OpenAPI spec")?;
    if spec.method.to_uppercase() != "GET" {
        anyhow::bail!("only GET OpenAPI operations are supported for generated tools");
    }
    let mut url = reqwest::Url::parse(&format!(
        "{}{}",
        spec.base_url.trim_end_matches('/'),
        spec.path
    ))?;
    for parameter in &record.parameters {
        if parameter.location != "query" {
            continue;
        }
        if let Some(value) = args.get(&parameter.name) {
            url.query_pairs_mut()
                .append_pair(&parameter.name, &value_to_command_arg(value));
        }
    }
    let response = tokio::time::timeout(
        Duration::from_secs(sandbox.max_execution_time_secs.max(1)),
        http_client.get(url).send(),
    )
    .await
    .context("generated OpenAPI call timed out")??;
    let body = response.error_for_status()?.text().await?;
    cap_output(body, sandbox.max_output_kb)
}

pub(crate) fn parse_cli_help_parameters(help: &str) -> Vec<GeneratedToolParameter> {
    let regex = Regex::new(
        r"^\s*(?:-[A-Za-z],\s*)?(--[A-Za-z0-9][A-Za-z0-9-]*)(?:[ =]<?([A-Za-z0-9_-]+)>?)?\s{2,}(.*)$",
    )
    .expect("valid CLI help regex");
    help.lines()
        .filter_map(|line| {
            let captures = regex.captures(line)?;
            let flag = captures.get(1)?.as_str().to_string();
            let name = flag.trim_start_matches("--").replace('-', "_");
            let takes_value = captures.get(2).is_some();
            Some(GeneratedToolParameter {
                name,
                description: captures
                    .get(3)
                    .map(|value| value.as_str().trim().to_string())
                    .unwrap_or_else(|| "generated CLI parameter".to_string()),
                cli_flag: Some(flag),
                required: false,
                param_type: if takes_value { "string" } else { "boolean" }.to_string(),
                location: "argument".to_string(),
            })
        })
        .collect()
}

fn build_cli_wrapper_synthesis_proposal(
    invocation: Vec<String>,
) -> Option<CliWrapperSynthesisProposal> {
    validate_safe_cli_invocation(&invocation).ok()?;
    let base = invocation.first()?;
    which::which(base).ok()?;
    Some(CliWrapperSynthesisProposal {
        tool_name: sanitize_tool_name(&invocation.join("_")),
        target: invocation.join(" "),
    })
}

fn infer_cli_wrapper_invocation(tool_name: &str) -> Option<Vec<String>> {
    let normalized = tool_name.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 64
        || !normalized
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        return None;
    }

    let tokens = normalized
        .split('_')
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() || tokens.len() > 2 {
        return None;
    }

    let invocation = tokens
        .iter()
        .map(|token| (*token).to_string())
        .collect::<Vec<_>>();
    validate_safe_cli_invocation(&invocation).ok()?;
    Some(invocation)
}

pub(crate) fn detect_cli_wrapper_synthesis_proposal(
    tool_name: &str,
) -> Option<CliWrapperSynthesisProposal> {
    let invocation = infer_cli_wrapper_invocation(tool_name)?;
    build_cli_wrapper_synthesis_proposal(invocation)
}

pub(crate) fn detect_cli_wrapper_synthesis_proposal_from_command(
    command: &str,
) -> Option<CliWrapperSynthesisProposal> {
    let trimmed = command.trim();
    if trimmed.is_empty()
        || trimmed.len() > 160
        || trimmed.contains(['|', '&', ';', '>', '<', '$', '`', '"', '\'', '(', ')', '\n'])
    {
        return None;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    let base = tokens[0];
    if base.starts_with('-') {
        return None;
    }
    let subcommand = tokens[1];
    if subcommand.starts_with('-')
        || !subcommand
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }

    let mut awaiting_flag_value = false;
    for token in tokens.iter().skip(2) {
        if token == &"--" {
            return None;
        }
        if token.starts_with('-') {
            awaiting_flag_value = true;
            continue;
        }
        if awaiting_flag_value {
            awaiting_flag_value = false;
            continue;
        }
        return None;
    }

    build_cli_wrapper_synthesis_proposal(vec![base.to_string(), subcommand.to_string()])
}

fn select_openapi_operation<'a>(
    spec: &'a serde_json::Value,
    operation_id_hint: Option<&str>,
) -> Result<(String, String, &'a serde_json::Value)> {
    let paths = spec
        .get("paths")
        .and_then(|value| value.as_object())
        .context("OpenAPI spec has no paths object")?;
    for (path, item) in paths {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        for (method, operation) in item_obj {
            if method.to_ascii_uppercase() != "GET" {
                continue;
            }
            if let Some(operation_id_hint) = operation_id_hint {
                if operation
                    .get("operationId")
                    .and_then(|value| value.as_str())
                    != Some(operation_id_hint)
                {
                    continue;
                }
            }
            return Ok((path.to_string(), method.to_ascii_uppercase(), operation));
        }
    }
    anyhow::bail!("no matching GET OpenAPI operation found")
}

pub(super) fn generated_tools_dir(agent_data_dir: &Path) -> PathBuf {
    agent_data_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("generated-tools")
}

fn sanitize_tool_name(raw: &str) -> String {
    let mut value = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while value.contains("__") {
        value = value.replace("__", "_");
    }
    value.trim_matches('_').to_string()
}

fn validate_safe_cli_invocation(invocation: &[String]) -> Result<()> {
    const SAFE_READONLY: &[&str] = &[
        "git", "kubectl", "docker", "cargo", "npm", "pnpm", "rg", "find", "ls", "cat",
    ];
    const DENY_TOKENS: &[&str] = &[
        "apply", "push", "commit", "delete", "rm", "write", "set", "scale", "patch", "exec", "run",
        "install", "publish",
    ];
    let base = invocation.first().map(String::as_str).unwrap_or_default();
    if !SAFE_READONLY.contains(&base) {
        anyhow::bail!(
            "CLI synthesis only allows conservative read-mostly commands; `{base}` is not allowed"
        );
    }
    if invocation
        .iter()
        .skip(1)
        .any(|token| DENY_TOKENS.contains(&token.as_str()))
    {
        anyhow::bail!(
            "CLI synthesis rejected because the invocation includes a mutating or risky token"
        );
    }
    Ok(())
}

fn value_to_command_arg(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn cap_output(mut text: String, max_output_kb: usize) -> Result<String> {
    let max_bytes = max_output_kb.max(1) * 1024;
    if text.len() > max_bytes {
        text.truncate(max_bytes);
        text.push_str("\n[output truncated]");
    }
    Ok(text)
}

pub(super) fn default_parameter_type() -> String {
    "string".to_string()
}

pub(super) fn default_parameter_location() -> String {
    "argument".to_string()
}

pub(super) fn run_generated_tool_json_wrappers() {}

impl AgentEngine {
    #[cfg(test)]
    pub async fn set_test_synthesize_tool_delay(&self, delay: Option<Duration>) {
        let gate = TEST_SYNTHESIZE_TOOL_DELAY
            .get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let key = self as *const AgentEngine as usize;
        let mut delays = gate.lock().await;
        if let Some(delay) = delay {
            delays.insert(key, delay);
        } else {
            delays.remove(&key);
        }
    }

    #[cfg(test)]
    pub async fn take_test_synthesize_tool_delay(&self) -> Option<Duration> {
        let gate = TEST_SYNTHESIZE_TOOL_DELAY
            .get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()));
        let key = self as *const AgentEngine as usize;
        gate.lock().await.remove(&key)
    }

    pub async fn list_generated_tools_json(&self) -> Result<String> {
        list_generated_tools(&self.data_dir)
    }

    pub async fn synthesize_tool_json(&self, request_json: &str) -> Result<String> {
        let args = serde_json::from_str::<serde_json::Value>(request_json)
            .context("invalid generated-tool synthesis request JSON")?;
        synthesize_tool(&args, self, &self.data_dir, &self.http_client, None).await
    }

    pub async fn run_generated_tool_json(
        &self,
        tool_name: &str,
        args_json: &str,
        session_key: Option<&str>,
    ) -> Result<String> {
        let args = serde_json::from_str::<serde_json::Value>(args_json)
            .context("invalid generated-tool arguments JSON")?;
        execute_generated_tool(
            tool_name,
            &args,
            self,
            &self.data_dir,
            &self.http_client,
            session_key,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))
    }

    pub async fn promote_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        promote_generated_tool(&self.data_dir, tool_name)
    }

    pub async fn activate_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        activate_generated_tool(&self.data_dir, tool_name)
    }

    pub async fn retire_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        retire_generated_tool(&self.data_dir, tool_name)
    }

    pub async fn restore_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        restore_generated_tool(&self.data_dir, tool_name)
    }
}
