use super::types::ThreadWorkspaceContext;
use zorai_protocol::tool_names as tools;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerMode {
    Ask,
    Plan,
    Agent,
}

const READ_ONLY_TOOLS: &[&str] = &[
    tools::READ_FILE,
    tools::LIST_FILES,
    tools::LIST_DIRECTORY,
    tools::SEARCH_FILES,
    tools::SEARCH_CODEBASE,
    tools::LSP_QUERY,
    tools::FIND_SYMBOL,
    tools::SYMBOL_SEARCH,
    tools::SEMANTIC_QUERY,
    tools::GET_GIT_STATUS,
    tools::GET_GIT_LINE_STATUSES,
    tools::GET_TERMINAL_CONTENT,
    tools::READ_ACTIVE_TERMINAL_CONTENT,
    tools::LIST_TERMINALS,
    tools::WEB_SEARCH,
    tools::WEB_READ,
    tools::FETCH_URL,
    tools::SEARCH_QUERY,
    tools::GET_TODOS,
    tools::LIST_TODOS,
    tools::UPDATE_TODO,
    tools::GET_CURRENT_DATETIME,
    tools::GET_SYSTEM_INFO,
    tools::READ_SKILL,
    tools::LIST_SKILLS,
    tools::DISCOVER_SKILLS,
    tools::READ_GUIDELINE,
    tools::LIST_GUIDELINES,
    tools::DISCOVER_GUIDELINES,
    tools::READ_MEMORY,
    tools::SEARCH_MEMORY,
    tools::SEARCH_HISTORY,
    tools::SESSION_SEARCH,
    tools::ONECONTEXT_SEARCH,
    tools::TOOL_SEARCH,
    tools::LOAD_TOOLS,
    tools::LIST_TOOLS,
    tools::ASK_QUESTIONS,
    tools::GET_THREAD,
    tools::READ_OFFLOADED_PAYLOAD,
    tools::ANALYZE_IMAGE,
    tools::IMAGE_QUERY,
    tools::READ_SOUL,
    tools::READ_USER,
    tools::SEARCH_SOUL,
    tools::SEARCH_USER,
    tools::AGENT_QUERY_MEMORY,
];

pub(crate) fn composer_mode(context: Option<&ThreadWorkspaceContext>) -> ComposerMode {
    match context
        .and_then(|context| context.mode.as_deref())
        .map(|mode| mode.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("ask") => ComposerMode::Ask,
        Some("plan") => ComposerMode::Plan,
        _ => ComposerMode::Agent,
    }
}

pub(crate) fn tool_allowed_in_mode(mode: ComposerMode, tool_name: &str) -> bool {
    mode == ComposerMode::Agent || READ_ONLY_TOOLS.contains(&tool_name)
}

pub(crate) fn blocked_tool_message(mode: ComposerMode, tool_name: &str) -> Option<String> {
    if tool_allowed_in_mode(mode, tool_name) {
        return None;
    }
    let mode_name = if mode == ComposerMode::Ask {
        "Ask"
    } else {
        "Plan"
    };
    Some(format!(
        "`{tool_name}` is not available: this thread is in {mode_name} mode, which is read-only. \
Do not retry it. Answer from what you can read, and tell the operator to switch the thread to Agent mode if changes are needed."
    ))
}

pub(crate) fn mode_prompt_block(mode: ComposerMode) -> Option<&'static str> {
    match mode {
        ComposerMode::Ask => Some(
            "## Mode: Ask\nThis thread is in Ask mode. You can read, search, and explain, but you cannot edit files, run commands, or delegate work: those tools are disabled and refused if called. Answer the question directly from what you can read.",
        ),
        ComposerMode::Plan => Some(
            "## Mode: Plan\nThis thread is in Plan mode. Explore read-only (editing, commands, and delegation are disabled and refused), then reply with a concrete implementation plan in Markdown: the goal, the files to change and why, numbered steps, risks, and how to verify. Do not make the changes. End by telling the operator to switch to Agent mode and ask you to execute the plan.",
        ),
        ComposerMode::Agent => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(mode: Option<&str>) -> ThreadWorkspaceContext {
        ThreadWorkspaceContext {
            root: "/repo".to_string(),
            mode: mode.map(String::from),
            ..ThreadWorkspaceContext::default()
        }
    }

    #[test]
    fn ask_and_plan_refuse_every_tool_that_can_change_the_machine() {
        for mode in [ComposerMode::Ask, ComposerMode::Plan] {
            for tool in [
                tools::WRITE_FILE,
                tools::EDIT_FILE,
                tools::APPLY_PATCH,
                tools::DELETE_FILE,
                tools::BASH_COMMAND,
                tools::RUN_TERMINAL_COMMAND,
                tools::PYTHON_EXECUTE,
                tools::SPAWN_SUBAGENT,
                tools::TYPE_IN_TERMINAL,
            ] {
                assert!(
                    blocked_tool_message(mode, tool).is_some(),
                    "{tool} must be refused"
                );
            }
            assert!(tool_allowed_in_mode(mode, tools::READ_FILE));
            assert!(tool_allowed_in_mode(mode, tools::SEARCH_FILES));
        }
    }

    #[test]
    fn agent_mode_and_unknown_or_missing_modes_keep_full_access() {
        assert_eq!(composer_mode(None), ComposerMode::Agent);
        assert_eq!(composer_mode(Some(&context(None))), ComposerMode::Agent);
        assert_eq!(
            composer_mode(Some(&context(Some("bogus")))),
            ComposerMode::Agent
        );
        assert_eq!(
            composer_mode(Some(&context(Some("ASK")))),
            ComposerMode::Ask
        );
        assert!(blocked_tool_message(ComposerMode::Agent, tools::WRITE_FILE).is_none());
        assert!(mode_prompt_block(ComposerMode::Agent).is_none());
    }
}
