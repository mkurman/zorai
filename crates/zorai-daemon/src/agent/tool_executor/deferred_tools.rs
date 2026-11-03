//! Deferred tool loading policy.
//!
//! The full tool catalog is large; sending every schema on every turn costs
//! thousands of fixed tokens. With deferral enabled, the niche long-tail tools
//! are withheld from the request `tools` array until the agent explicitly
//! activates them with `load_tools` (after discovering them via `tool_search`,
//! which always searches the entire catalog). Common file/shell/search/memory/
//! agent tools and the discovery meta-tools are always present.

use zorai_protocol::tool_names as tn;

use super::ToolDefinition;

pub(crate) const META_TOOL_NAMES: &[&str] = &[tn::TOOL_SEARCH, tn::LIST_TOOLS, tn::LOAD_TOOLS];

const DEFERRABLE_GROUPS: &[&[&str]] = &[
    tn::ROUTINE_TOOLS,
    tn::TRIGGER_TOOLS,
    tn::DEBATE_TOOLS,
    tn::COLLABORATION_TOOLS,
    tn::MODEL_TOOLS,
    tn::AUDIO_TOOLS,
    tn::IMAGE_TOOLS,
    tn::VIDEO_TOOLS,
];

const DEFERRABLE_EXTRA: &[&str] = &[
    // Emergent-protocol management (LIST_THREADS / GET_THREAD stay available).
    tn::LOOKUP_EMERGENT_PROTOCOL,
    tn::LIST_EMERGENT_PROTOCOL_PROPOSALS,
    tn::RESPOND_EMERGENT_PROTOCOL_PROPOSAL,
    tn::RELOAD_EMERGENT_PROTOCOL_REGISTRY,
    tn::DECODE_EMERGENT_PROTOCOL,
    tn::GET_EMERGENT_PROTOCOL_USAGE_LOG,
    // Generated-tool synthesis and skill variants (skill discovery stays).
    tn::SYNTHESIZE_TOOL,
    tn::ACTIVATE_GENERATED_TOOL,
    tn::PROMOTE_GENERATED_TOOL,
    tn::RESTORE_GENERATED_TOOL,
    tn::LIST_GENERATED_TOOLS,
    tn::RUN_GENERATED_TOOL,
    tn::LIST_SKILL_VARIANTS,
    tn::INSPECT_SKILL_VARIANT,
    // Compliance / provenance / audit / ops.
    tn::GENERATE_SOC2_ARTIFACT,
    tn::GET_CAUSAL_TRACE_REPORT,
    tn::GET_COUNTERFACTUAL_REPORT,
    tn::GET_MEMORY_PROVENANCE_REPORT,
    tn::GET_PROVENANCE_REPORT,
    tn::QUERY_AUDITS,
    tn::VERIFY_INTEGRITY,
    tn::SCRUB_SENSITIVE,
    tn::LIST_SNAPSHOTS,
    tn::RESTORE_SNAPSHOT,
    tn::SHOW_DREAMS,
    tn::SHOW_HARNESS_STATE,
    tn::IMPORT_EXTERNAL_RUNTIME,
    tn::SHOW_IMPORT_REPORT,
    tn::PREVIEW_SHADOW_RUN,
    tn::FETCH_GATEWAY_HISTORY,
    tn::DEPLOY,
    tn::INSTALL_PACKAGE,
    tn::WRITE_CONFIG,
    tn::GET_COST_SUMMARY,
    // Browser profile management (browser navigation stays available).
    tn::LIST_BROWSER_PROFILES,
    tn::CREATE_BROWSER_PROFILE,
    tn::UPDATE_BROWSER_PROFILE_HEALTH,
    // Workspace layout / surfaces / snippets / workspace lifecycle.
    // Workspace *task* tools are intentionally NOT deferred.
    tn::CREATE_WORKSPACE,
    tn::SET_ACTIVE_WORKSPACE,
    tn::LIST_WORKSPACES,
    tn::CREATE_SURFACE,
    tn::SET_ACTIVE_SURFACE,
    tn::SPLIT_PANE,
    tn::RENAME_PANE,
    tn::SET_LAYOUT_PRESET,
    tn::EQUALIZE_LAYOUT,
    tn::LIST_SNIPPETS,
    tn::CREATE_SNIPPET,
    tn::RUN_SNIPPET,
    tn::LIST_SESSIONS,
    // External messaging channels.
    tn::SEND_SLACK_MESSAGE,
    tn::SEND_DISCORD_MESSAGE,
    tn::SEND_TELEGRAM_MESSAGE,
    tn::SEND_WHATSAPP_MESSAGE,
    tn::WHATSAPP_LINK_START,
    tn::WHATSAPP_LINK_STATUS,
    tn::WHATSAPP_LINK_STOP,
    tn::WHATSAPP_LINK_RESET,
    // Misc niche.
    tn::RUN_WORKFLOW_PACK,
    tn::PLUGIN_API_CALL,
];

/// Whether `name` is part of the deferrable long-tail (withheld until loaded).
pub(crate) fn is_deferrable_tool(name: &str) -> bool {
    if META_TOOL_NAMES.contains(&name) {
        return false;
    }
    DEFERRABLE_GROUPS.iter().any(|group| group.contains(&name)) || DEFERRABLE_EXTRA.contains(&name)
}

/// Remove deferrable tools from `tools` and return them as a pool the runner can
/// splice back when the agent calls `load_tools`. Tools kept are the always-on
/// set; the returned pool is the withheld long-tail.
pub(crate) fn partition_deferred_tools(tools: &mut Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    let mut kept = Vec::with_capacity(tools.len());
    let mut pool = Vec::new();
    for tool in tools.drain(..) {
        if is_deferrable_tool(&tool.function.name) {
            pool.push(tool);
        } else {
            kept.push(tool);
        }
    }
    *tools = kept;
    pool
}
