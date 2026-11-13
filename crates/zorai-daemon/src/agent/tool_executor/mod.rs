#![allow(dead_code)]

//! Tool execution for the agent engine.
//!
//! Maps tool calls to daemon infrastructure. Tools that require frontend
//! state (workspace/pane/browser) are not available in daemon mode — only
//! tools that can execute headlessly are included here.

mod deferred_tools;
#[path = "catalog/part_a.rs"]
mod part_a;
#[path = "catalog/part_b.rs"]
mod part_b;
#[path = "catalog/part_c.rs"]
mod part_c;
#[path = "catalog/part_d.rs"]
mod part_d;
mod prelude;
mod result_metadata;
mod search_runtime;
mod thread_tools;

fn tool_required_fields(parameters: &serde_json::Value) -> Vec<String> {
    parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn tool_parameter_names(parameters: &serde_json::Value) -> Vec<String> {
    parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn summarize_tool_definition(tool: ToolDefinition) -> zorai_protocol::ToolDescriptorPublic {
    zorai_protocol::ToolDescriptorPublic {
        name: tool.function.name,
        description: tool.function.description,
        required: tool_required_fields(&tool.function.parameters),
        parameters: tool.function.parameters.to_string(),
    }
}

fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn score_tool_definition(
    tool: &ToolDefinition,
    query_lower: &str,
    tokens: &[String],
) -> Option<zorai_protocol::ToolSearchMatchPublic> {
    let name = tool.function.name.to_ascii_lowercase();
    let description = tool.function.description.to_ascii_lowercase();
    let parameter_names = tool_parameter_names(&tool.function.parameters)
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let mut score = 0u32;
    let mut matched_fields = std::collections::BTreeSet::new();

    if name == query_lower {
        score += 100;
        matched_fields.insert("name_exact".to_string());
    } else if name.contains(query_lower) {
        score += 60;
        matched_fields.insert("name".to_string());
    }
    if description.contains(query_lower) {
        score += 25;
        matched_fields.insert("description".to_string());
    }
    if parameter_names
        .iter()
        .any(|value| value.contains(query_lower))
    {
        score += 15;
        matched_fields.insert("parameters".to_string());
    }

    for token in tokens {
        if token == query_lower {
            continue;
        }
        if name.contains(token) {
            score += 12;
            matched_fields.insert("name".to_string());
        }
        if description.contains(token) {
            score += 4;
            matched_fields.insert("description".to_string());
        }
        if parameter_names.iter().any(|value| value.contains(token)) {
            score += 3;
            matched_fields.insert("parameters".to_string());
        }
    }

    if score == 0 {
        return None;
    }

    Some(zorai_protocol::ToolSearchMatchPublic {
        name: tool.function.name.clone(),
        description: tool.function.description.clone(),
        required: tool_required_fields(&tool.function.parameters),
        parameters: tool.function.parameters.to_string(),
        score,
        matched_fields: matched_fields.into_iter().collect(),
    })
}

pub fn reorder_tools_by_heuristics(
    tools: &mut [ToolDefinition],
    heuristic_store: &super::learning::heuristics::HeuristicStore,
    task_type: &str,
    preferred_tool_fallbacks: &[String],
    prioritize_clarification: bool,
) {
    let scores: std::collections::HashMap<String, f64> = if task_type.is_empty() {
        std::collections::HashMap::new()
    } else {
        heuristic_store
            .tool_heuristics
            .iter()
            .filter(|h| h.task_type == task_type && h.usage_count >= 5)
            .map(|h| (h.tool_name.clone(), h.effectiveness_score))
            .collect()
    };

    let preferred_ranks: std::collections::HashMap<String, usize> =
        preferred_tool_fallbacks.iter().enumerate().fold(
            std::collections::HashMap::new(),
            |mut ranks, (idx, tool_name)| {
                ranks.entry(tool_name.to_ascii_lowercase()).or_insert(idx);
                ranks
            },
        );

    if scores.is_empty() && preferred_ranks.is_empty() && !prioritize_clarification {
        return;
    }

    tools.sort_by(|a, b| {
        if prioritize_clarification {
            match (
                a.function
                    .name
                    .eq_ignore_ascii_case(tool_names::ASK_QUESTIONS),
                b.function
                    .name
                    .eq_ignore_ascii_case(tool_names::ASK_QUESTIONS),
            ) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
        }

        let score_cmp = match (
            scores.get(&a.function.name).copied(),
            scores.get(&b.function.name).copied(),
        ) {
            (Some(score_a), Some(score_b)) => score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        if score_cmp != std::cmp::Ordering::Equal {
            return score_cmp;
        }

        match (
            preferred_ranks
                .get(&a.function.name.to_ascii_lowercase())
                .copied(),
            preferred_ranks
                .get(&b.function.name.to_ascii_lowercase())
                .copied(),
        ) {
            (Some(rank_a), Some(rank_b)) => rank_a.cmp(&rank_b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

pub fn get_available_tools(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    add_available_tools_part_a(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_b(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_c(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_d(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_workspace_task_tools(&mut tools);
    tools
}

pub(crate) fn filter_tools_for_client_surface(
    tools: &mut Vec<ToolDefinition>,
    client_surface: Option<zorai_protocol::ClientSurface>,
) {
    if matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui)) {
        tools.retain(|tool| {
            !matches!(
                tool.function.name.as_str(),
                tool_names::LIST_SESSIONS
                    | tool_names::LIST_TERMINALS
                    | tool_names::READ_ACTIVE_TERMINAL_CONTENT
                    | tool_names::RUN_TERMINAL_COMMAND
                    | tool_names::EXECUTE_MANAGED_COMMAND
                    | tool_names::ALLOCATE_TERMINAL
                    | tool_names::TYPE_IN_TERMINAL
            )
        });
    }
}

pub(crate) fn list_available_tools_public(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
    client_surface: Option<zorai_protocol::ClientSurface>,
    limit: usize,
    offset: usize,
) -> zorai_protocol::ToolListResultPublic {
    let mut tools = get_available_tools(config, agent_data_dir, has_workspace_topology);
    filter_tools_for_client_surface(&mut tools, client_surface);
    let total = tools.len();
    let limit = limit.clamp(1, 200);
    let items = tools
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(summarize_tool_definition)
        .collect();

    zorai_protocol::ToolListResultPublic {
        total,
        limit,
        offset,
        items,
    }
}

pub(crate) fn search_available_tools_public(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
    client_surface: Option<zorai_protocol::ClientSurface>,
    query: &str,
    limit: usize,
    offset: usize,
) -> zorai_protocol::ToolSearchResultPublic {
    let normalized_query = query.trim().to_ascii_lowercase();
    let tokens = query_tokens(&normalized_query);
    let mut tools = get_available_tools(config, agent_data_dir, has_workspace_topology);
    filter_tools_for_client_surface(&mut tools, client_surface);
    let mut matches = tools
        .into_iter()
        .filter_map(|tool| score_tool_definition(&tool, &normalized_query, &tokens))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
    let total = matches.len();
    let limit = limit.clamp(1, 200);
    let items = matches.into_iter().skip(offset).take(limit).collect();

    zorai_protocol::ToolSearchResultPublic {
        query: query.to_string(),
        total,
        limit,
        offset,
        items,
    }
}

mod execute_tool_impl;
mod memory_flush;
mod memory_tools;

mod file_tools;
mod gateway_workspace;
mod managed_commands;
mod managed_helpers;
mod media_tools;
mod operation_status;
mod parent_child;
mod parse_args;
pub(crate) use parent_child::task_is_awaiting_parent;
mod python_execute;
mod setup_web;
mod skills_and_search;
mod subagent_budget;
mod subagents;
mod system_history;
mod tasks;
mod terminal_alloc;
mod terminal_input;
mod terminal_runtime;
mod workflow_fetch;
mod workspace_task_tools;

pub(crate) async fn execute_media_tool_for_ipc(
    tool_name: &str,
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    if !args.is_object() {
        anyhow::bail!("media tool arguments must be a JSON object");
    }

    match tool_name {
        tool_names::SPEECH_TO_TEXT => execute_speech_to_text(args, agent, &agent.http_client).await,
        tool_names::TEXT_TO_SPEECH => execute_text_to_speech(args, agent, &agent.http_client).await,
        tool_names::ANALYZE_IMAGE => execute_analyze_image(args, agent, &agent.http_client).await,
        _ => anyhow::bail!("unsupported media tool for IPC: {tool_name}"),
    }
}

pub(crate) use deferred_tools::*;
pub use execute_tool_impl::*;
pub(crate) use file_tools::*;
pub(crate) use gateway_workspace::*;
pub(crate) use managed_commands::*;
pub(crate) use managed_helpers::*;
pub(crate) use media_tools::*;
pub use memory_flush::*;
pub(crate) use memory_tools::*;
pub(crate) use operation_status::*;
pub(crate) use parent_child::*;
pub(crate) use parse_args::*;
pub(crate) use part_a::*;
pub(crate) use part_b::*;
pub(crate) use part_c::*;
pub(crate) use part_d::*;
pub(crate) use prelude::*;
pub(crate) use python_execute::*;
pub(crate) use result_metadata::*;
pub(crate) use search_runtime::*;
pub(crate) use setup_web::*;
pub(crate) use skills_and_search::*;
pub(crate) use subagent_budget::*;
pub(crate) use subagents::*;
pub(crate) use system_history::*;
pub(crate) use tasks::*;
pub(crate) use terminal_alloc::*;
pub(crate) use terminal_input::*;
pub(crate) use terminal_runtime::*;
pub(crate) use thread_tools::*;
pub(crate) use workflow_fetch::*;
pub(crate) use workspace_task_tools::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod schema_token_budget {
    /// Every tool schema rides in every LLM request. This ceiling exists so a
    /// new or expanded tool cannot silently inflate the per-turn fixed cost;
    /// raise it only as a deliberate decision. Run with `--nocapture` to see
    /// the per-tool size breakdown when it trips.
    const MAX_TOTAL_SCHEMA_CHARS: usize = 95_000;
    const MAX_SINGLE_SCHEMA_CHARS: usize = 2_600;

    #[test]
    fn tool_schemas_stay_within_token_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let config = crate::agent::AgentConfig::default();
        let tools = super::get_available_tools(&config, tmp.path(), true);
        let mut sizes: Vec<(usize, String)> = tools
            .iter()
            .map(|t| {
                (
                    serde_json::to_string(t).unwrap().chars().count(),
                    t.function.name.clone(),
                )
            })
            .collect();
        sizes.sort_by(|a, b| b.0.cmp(&a.0));
        let total: usize = sizes.iter().map(|(s, _)| s).sum();
        println!(
            "TOTAL {} tools, {} chars (~{} tokens)",
            sizes.len(),
            total,
            total / 4
        );
        for (size, name) in &sizes {
            println!("{:>7} chars (~{:>5} tok)  {}", size, size / 4, name);
        }
        assert!(
            total <= MAX_TOTAL_SCHEMA_CHARS,
            "tool schemas total {total} chars (~{} tokens), over the {MAX_TOTAL_SCHEMA_CHARS}-char budget; trim descriptions or gate tools",
            total / 4,
        );
        if let Some((size, name)) = sizes.first() {
            assert!(
                *size <= MAX_SINGLE_SCHEMA_CHARS,
                "tool `{name}` serializes to {size} chars (~{} tokens); single-tool ceiling is {MAX_SINGLE_SCHEMA_CHARS}",
                size / 4,
            );
        }
    }
}
