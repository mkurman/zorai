use super::*;

const MEMORY_PROMPT_SECTIONS: &[&str] = &[
    "Persistent Memory",
    "Operator Profile",
    "Shared User Profile Policy",
    "Structured Memory Summary",
    "Soul Summary",
    "Memory Summary",
    "User Summary",
    "Continuity Summary",
    "Active Constraints",
    "Working Continuity",
    "Ruled-out approaches",
    "Learned Patterns",
    "Semantic Memory Palace",
    "Cross-Session Memory",
    "OneContext Recall",
];

const SKILL_PROMPT_SECTIONS: &[&str] = &[
    "Local Guidelines",
    "Local Skills",
    "Plugin Skills",
    "Preloaded Skills",
];

const SESSION_STATE_PROMPT_SECTIONS: &[&str] = &[
    "Runtime Identity",
    "Time Context",
    "Thread Artifact Directories",
    "Thread Workspace Context",
    "Temporal Foresight",
    "Emergent Protocol Decode",
    "Deferred Tools",
    "Internal DM Constraints",
];

#[derive(Clone, Copy)]
enum PromptSection {
    System,
    Memory,
    Skills,
    SessionState,
}

fn prompt_section_for_heading(heading: &str) -> PromptSection {
    let matches = |names: &[&str]| names.iter().any(|name| heading.starts_with(name));
    if matches(MEMORY_PROMPT_SECTIONS) {
        PromptSection::Memory
    } else if matches(SKILL_PROMPT_SECTIONS) {
        PromptSection::Skills
    } else if matches(SESSION_STATE_PROMPT_SECTIONS) {
        PromptSection::SessionState
    } else {
        PromptSection::System
    }
}

fn chars_to_tokens(chars: usize) -> u64 {
    chars.div_ceil(APPROX_CHARS_PER_TOKEN) as u64
}

pub(crate) fn context_breakdown(
    system_prompt: &str,
    tools: &[ToolDefinition],
    deferred_tools_count: usize,
    messages: &[AgentMessage],
    context_window_tokens: usize,
    compaction_threshold_tokens: usize,
) -> ContextBreakdown {
    let mut section_chars = [0usize; 4];
    let mut section = PromptSection::System;
    for line in system_prompt.split('\n') {
        if let Some(heading) = line.strip_prefix("## ") {
            section = prompt_section_for_heading(heading.trim());
        }
        section_chars[section as usize] += line.chars().count() + 1;
    }

    let tools_chars = tools
        .iter()
        .map(|tool| {
            serde_json::to_string(tool)
                .map(|json| json.chars().count())
                .unwrap_or(0)
        })
        .sum::<usize>();

    let (_, active_messages) = active_compaction_window(messages);
    let (compaction_summary_tokens, messages_tokens) =
        active_messages
            .iter()
            .fold((0u64, 0u64), |(summary, rest), message| {
                let tokens = estimate_single_message_tokens(message) as u64;
                if message_is_compaction_summary(message) {
                    (summary + tokens, rest)
                } else {
                    (summary, rest + tokens)
                }
            });

    ContextBreakdown {
        context_window_tokens: context_window_tokens as u64,
        compaction_threshold_tokens: compaction_threshold_tokens as u64,
        messages_tokens,
        compaction_summary_tokens,
        system_prompt_tokens: chars_to_tokens(section_chars[PromptSection::System as usize]),
        memory_tokens: chars_to_tokens(section_chars[PromptSection::Memory as usize]),
        skills_tokens: chars_to_tokens(section_chars[PromptSection::Skills as usize]),
        session_state_tokens: chars_to_tokens(section_chars[PromptSection::SessionState as usize]),
        tools_tokens: chars_to_tokens(tools_chars),
        tools_count: tools.len() as u64,
        deferred_tools_count: deferred_tools_count as u64,
        input_tokens: messages.iter().map(|message| message.input_tokens).sum(),
        output_tokens: messages.iter().map(|message| message.output_tokens).sum(),
        cached_input_tokens: messages
            .iter()
            .map(|message| message.cache_usage_tokens().0)
            .sum(),
    }
}
