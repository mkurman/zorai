//! Runtime generated-tool registry with conservative guardrails.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::*;

#[path = "tool_synthesis_runtime.rs"]
mod runtime;

#[cfg(test)]
pub(crate) use runtime::parse_cli_help_parameters;
use runtime::{
    default_parameter_location, default_parameter_type, generated_tools_dir,
    run_cli_generated_tool, run_openapi_generated_tool, synthesize_cli_tool,
    synthesize_openapi_tool, CliWrapperSynthesisProposal,
};
pub(crate) use runtime::{
    detect_cli_wrapper_synthesis_proposal, detect_cli_wrapper_synthesis_proposal_from_command,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GeneratedToolKind {
    Cli,
    OpenApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedToolParameter {
    name: String,
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli_flag: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default = "default_parameter_type")]
    param_type: String,
    #[serde(default = "default_parameter_location")]
    location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedCliSpec {
    invocation: Vec<String>,
    help_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedOpenApiSpec {
    spec_url: String,
    base_url: String,
    path: String,
    method: String,
    operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GeneratedToolRecord {
    id: String,
    name: String,
    description: String,
    kind: GeneratedToolKind,
    parameters: Vec<GeneratedToolParameter>,
    status: String,
    created_at: u64,
    updated_at: u64,
    last_used_at: Option<u64>,
    calls_total: u32,
    calls_success: u32,
    calls_failure: u32,
    calls_timeout: u32,
    sessions_used: u32,
    last_session_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    promoted_skill_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli: Option<GeneratedCliSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openapi: Option<GeneratedOpenApiSpec>,
}

impl GeneratedToolRecord {
    fn effectiveness(&self) -> f64 {
        if self.calls_total == 0 {
            return 0.5;
        }
        let success = self.calls_success as f64 / self.calls_total as f64;
        let recency = if self.last_used_at.is_some() {
            1.0
        } else {
            0.5
        };
        success * 0.8 + recency * 0.2
    }

    fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "active" | "promoted")
    }
}

pub(crate) fn generated_tool_definitions(
    config: &AgentConfig,
    agent_data_dir: &Path,
) -> Vec<ToolDefinition> {
    if !config.tool_synthesis.enabled {
        return Vec::new();
    }
    load_generated_tools(agent_data_dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| tool.is_active())
        .map(|tool| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for parameter in &tool.parameters {
                properties.insert(
                    parameter.name.clone(),
                    serde_json::json!({
                        "type": parameter.param_type,
                        "description": parameter.description,
                    }),
                );
                if parameter.required {
                    required.push(parameter.name.clone());
                }
            }
            ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: tool.id.clone(),
                    description: format!("Generated tool: {}", tool.description),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }),
                },
            }
        })
        .collect()
}

pub(crate) async fn synthesize_tool(
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &Path,
    http_client: &reqwest::Client,
    thread_id: Option<&str>,
) -> Result<String> {
    let config = agent.config.read().await.clone();
    if !config.tool_synthesis.enabled {
        anyhow::bail!("tool synthesis is disabled in agent config");
    }

    let kind = args
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("cli");
    let target = args
        .get("target")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'target' argument"))?;
    let requested_name = args.get("name").and_then(|value| value.as_str());
    let activate = args
        .get("activate")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let record = match kind {
        "cli" => {
            let os_sandbox_root =
                crate::agent::tool_executor::operator_sandbox_enabled(agent, thread_id)
                    .await
                    .then(|| agent_data_dir.join("generated-tool-sandbox"));
            synthesize_cli_tool(target, requested_name, os_sandbox_root.as_deref()).await?
        }
        "openapi" => {
            let operation_id = args.get("operation_id").and_then(|value| value.as_str());
            if let Some(existing) =
                find_equivalent_generated_openapi_tool(agent_data_dir, target, operation_id)?
            {
                if let Some(existing_id) = existing.get("id").and_then(|value| value.as_str()) {
                    if let Some(record) = load_generated_tool(agent_data_dir, existing_id)? {
                        return Ok(serde_json::to_string_pretty(&record)?);
                    }
                }
            }
            synthesize_openapi_tool(target, requested_name, args, http_client).await?
        }
        other => anyhow::bail!("unsupported synthesis kind: {other}"),
    };
    let mut record = record;
    record.status = if activate && !config.tool_synthesis.require_activation {
        "active".to_string()
    } else {
        "new".to_string()
    };
    ensure_generated_tool_capacity(
        agent_data_dir,
        config.tool_synthesis.max_generated_tools,
        &record.id,
    )?;
    save_generated_tool(agent_data_dir, &record)?;
    prune_generated_tools(agent_data_dir, config.tool_synthesis.max_generated_tools)?;

    Ok(serde_json::to_string_pretty(&record)?)
}

pub(crate) fn list_generated_tools(agent_data_dir: &Path) -> Result<String> {
    Ok(serde_json::to_string_pretty(&load_generated_tools(
        agent_data_dir,
    )?)?)
}

#[cfg(test)]
pub(crate) fn has_equivalent_generated_cli_tool(
    agent_data_dir: &Path,
    proposal: &CliWrapperSynthesisProposal,
) -> Result<bool> {
    Ok(find_equivalent_generated_cli_tool(agent_data_dir, proposal)?.is_some())
}

pub(crate) fn find_equivalent_generated_cli_tool(
    agent_data_dir: &Path,
    proposal: &CliWrapperSynthesisProposal,
) -> Result<Option<serde_json::Value>> {
    let normalized_target = proposal
        .target
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for tool in load_generated_tools(agent_data_dir)? {
        if tool.status == "archived" {
            continue;
        }
        let Some(cli) = tool.cli.as_ref() else {
            continue;
        };
        let invocation = cli.invocation.join(" ");
        let normalized_invocation = invocation.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized_help_source = cli
            .help_source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if normalized_invocation == normalized_target
            || normalized_help_source == normalized_target
            || tool.id == proposal.tool_name
        {
            return Ok(Some(serde_json::json!({
                "id": tool.id,
                "name": tool.name,
                "status": tool.status,
                "target": normalized_target,
            })));
        }
    }
    Ok(None)
}

pub(crate) fn find_equivalent_generated_openapi_tool(
    agent_data_dir: &Path,
    spec_url: &str,
    operation_id: Option<&str>,
) -> Result<Option<serde_json::Value>> {
    let normalized_spec_url = spec_url.trim();
    for tool in load_generated_tools(agent_data_dir)? {
        if tool.status == "archived" {
            continue;
        }
        let Some(openapi) = tool.openapi.as_ref() else {
            continue;
        };
        if openapi.spec_url.trim() != normalized_spec_url {
            continue;
        }
        if let Some(operation_id) = operation_id {
            if openapi.operation_id.as_deref() != Some(operation_id) {
                continue;
            }
        }
        return Ok(Some(serde_json::json!({
            "id": tool.id,
            "name": tool.name,
            "status": tool.status,
            "spec_url": normalized_spec_url,
            "operation_id": openapi.operation_id,
        })));
    }
    Ok(None)
}

pub(crate) async fn execute_generated_tool(
    tool_name: &str,
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &Path,
    http_client: &reqwest::Client,
    session_key: Option<&str>,
) -> Result<Option<String>> {
    let mut record = match load_generated_tool(agent_data_dir, tool_name)? {
        Some(record) => record,
        None => return Ok(None),
    };
    if !record.is_active() {
        anyhow::bail!("generated tool `{tool_name}` is not active");
    }

    let sandbox = agent.config.read().await.tool_synthesis.sandbox.clone();
    let os_sandbox_root = crate::agent::tool_executor::operator_sandbox_enabled(agent, session_key)
        .await
        .then(|| agent_data_dir.join("generated-tool-sandbox"));
    let output: Result<String> = match record.kind {
        GeneratedToolKind::Cli => {
            run_cli_generated_tool(&record, args, &sandbox, os_sandbox_root.as_deref()).await
        }
        GeneratedToolKind::OpenApi => {
            run_openapi_generated_tool(&record, args, &sandbox, http_client).await
        }
    };

    let now = now_millis();
    record.calls_total = record.calls_total.saturating_add(1);
    record.updated_at = now;
    record.last_used_at = Some(now);
    if let Some(session_key) = session_key {
        if record.last_session_key.as_deref() != Some(session_key) {
            record.sessions_used = record.sessions_used.saturating_add(1);
            record.last_session_key = Some(session_key.to_string());
        }
    }
    match &output {
        Ok(_) => record.calls_success = record.calls_success.saturating_add(1),
        Err(error) if error.to_string().contains("timed out") => {
            record.calls_timeout = record.calls_timeout.saturating_add(1);
        }
        Err(_) => record.calls_failure = record.calls_failure.saturating_add(1),
    }
    if record.effectiveness()
        > agent
            .config
            .read()
            .await
            .tool_synthesis
            .auto_promote_threshold
        && record.calls_total >= 3
        && record.status == "active"
    {
        record.status = "promotable".to_string();
    }
    save_generated_tool(agent_data_dir, &record)?;
    output.map(Some)
}

pub(crate) fn promote_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    if record.status == "new" {
        anyhow::bail!(
            "generated tool `{tool_name}` is not ready for promotion; activate and review it first"
        );
    }
    let skill_dir = super::skills_dir(agent_data_dir).join("generated");
    std::fs::create_dir_all(&skill_dir)?;
    let path = skill_dir.join(format!("use-{}.md", record.id));
    let parameter_lines = if record.parameters.is_empty() {
        "- none".to_string()
    } else {
        record
            .parameters
            .iter()
            .map(|parameter| format!("- `{}`: {}", parameter.name, parameter.description))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let implementation = match (&record.cli, &record.openapi) {
        (Some(cli), None) => cli.invocation.join(" "),
        (None, Some(api)) => format!("{} {}", api.method, api.path),
        _ => record.name.clone(),
    };
    let content = format!(
        "# Skill: Use {}\n\n## When to Use\n- Use this generated tool when the task needs `{}`.\n\n## How\n```text\n{}\n```\n\n## Parameters\n{}\n\n## Notes\n- Generated from {}\n- Effectiveness: {:.0}%\n- Used in {} sessions\n",
        record.name,
        record.description,
        implementation,
        parameter_lines,
        record
            .cli
            .as_ref()
            .map(|cli| cli.help_source.as_str())
            .or_else(|| record.openapi.as_ref().map(|api| api.spec_url.as_str()))
            .unwrap_or("generated runtime metadata"),
        record.effectiveness() * 100.0,
        record.sessions_used,
    );
    std::fs::write(&path, content)?;
    record.status = "promoted".to_string();
    record.promoted_skill_path = Some(path.display().to_string());
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

pub(crate) fn activate_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    record.status = "active".to_string();
    record.updated_at = now_millis();
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

pub(crate) fn retire_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    if let Some(path) = record.promoted_skill_path.as_deref() {
        let skill_path = Path::new(path);
        if skill_path.exists() {
            std::fs::remove_file(skill_path).with_context(|| {
                format!("failed to remove promoted skill artifact for generated tool `{tool_name}`")
            })?;
        }
    }
    record.status = "archived".to_string();
    record.updated_at = now_millis();
    record.promoted_skill_path = None;
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

pub(crate) fn restore_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    if record.status != "archived" {
        anyhow::bail!(
            "only archived generated tools can be restored; `{tool_name}` is currently `{}`",
            record.status
        );
    }
    record.status = "active".to_string();
    record.updated_at = now_millis();
    record.promoted_skill_path = None;
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

fn load_generated_tools(agent_data_dir: &Path) -> Result<Vec<GeneratedToolRecord>> {
    let root = generated_tools_dir(agent_data_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut tools = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path().join("tool.json");
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(path)?;
        if let Ok(record) = serde_json::from_str::<GeneratedToolRecord>(&raw) {
            tools.push(record);
        }
    }
    tools.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(tools)
}

fn load_generated_tool(
    agent_data_dir: &Path,
    tool_name: &str,
) -> Result<Option<GeneratedToolRecord>> {
    for tool in load_generated_tools(agent_data_dir)? {
        if tool.id == tool_name {
            return Ok(Some(tool));
        }
    }
    Ok(None)
}

fn save_generated_tool(agent_data_dir: &Path, record: &GeneratedToolRecord) -> Result<()> {
    let root = generated_tools_dir(agent_data_dir).join(&record.id);
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("tool.json"),
        serde_json::to_string_pretty(record)?,
    )?;
    Ok(())
}

fn ensure_generated_tool_capacity(
    agent_data_dir: &Path,
    max_tools: usize,
    incoming_id: &str,
) -> Result<()> {
    let tools = load_generated_tools(agent_data_dir)?;
    if tools.iter().any(|record| record.id == incoming_id) || tools.len() < max_tools {
        return Ok(());
    }
    let prunable = tools
        .iter()
        .filter(|record| generated_tool_is_prunable(record))
        .count();
    if prunable == 0 {
        anyhow::bail!(
            "generated tool registry is full at {max_tools} tools and only active/promoted tools remain; remove or demote one before synthesizing another"
        );
    }
    Ok(())
}

fn generated_tool_is_prunable(record: &GeneratedToolRecord) -> bool {
    !matches!(record.status.as_str(), "active" | "promoted")
}

fn prune_generated_tools(agent_data_dir: &Path, max_tools: usize) -> Result<()> {
    let tools = load_generated_tools(agent_data_dir)?;
    if tools.len() <= max_tools {
        return Ok(());
    }
    let excess = tools.len() - max_tools;
    let mut removed = 0usize;
    for record in tools.iter().rev() {
        if removed >= excess {
            break;
        }
        if !generated_tool_is_prunable(record) {
            continue;
        }
        let dir = generated_tools_dir(agent_data_dir).join(&record.id);
        std::fs::remove_dir_all(&dir).with_context(|| {
            format!(
                "failed to prune generated tool `{}` from {}",
                record.id,
                dir.display()
            )
        })?;
        removed += 1;
    }
    if tools.len().saturating_sub(removed) > max_tools {
        anyhow::bail!(
            "generated tool registry still exceeds the configured limit because active/promoted tools are protected from pruning"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/tool_synthesis.rs"]
mod tests;
