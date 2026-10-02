use super::markdown_table;
use super::*;
use crate::theme::ThemeTokens;
use markdown_table::{is_markdown_table_row, is_markdown_table_start, render_markdown_table};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use zorai_protocol::tool_names;

pub(crate) fn format_weles_review_badge(
    review: &crate::state::chat::WelesReviewMetaVm,
    theme: &ThemeTokens,
) -> (String, Style) {
    match review.verdict.as_str() {
        "block" => ("blocked".to_string(), theme.accent_danger),
        "flag_only" => ("flagged".to_string(), theme.accent_secondary),
        _ if review.weles_reviewed => ("reviewed".to_string(), theme.fg_dim),
        _ => ("unreviewed".to_string(), theme.fg_dim),
    }
}

pub(crate) fn render_weles_review_details(
    review: &crate::state::chat::WelesReviewMetaVm,
    theme: &ThemeTokens,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let detail_width = width.max(1);
    let (badge, badge_style) = format_weles_review_badge(review, theme);
    let mut meta_spans = vec![
        Span::styled("weles: ".to_string(), theme.fg_dim),
        Span::styled(badge, badge_style),
    ];

    if let Some(mode) = review.security_override_mode.as_deref() {
        if !mode.is_empty() {
            meta_spans.push(Span::raw(" "));
            meta_spans.push(Span::styled(
                format!("override={mode}"),
                theme.accent_secondary,
            ));
        }
    }

    if !review.weles_reviewed {
        meta_spans.push(Span::raw(" "));
        meta_spans.push(Span::styled("degraded", theme.accent_secondary));
    }

    if let Some(audit_id) = review.audit_id.as_deref() {
        if !audit_id.is_empty() {
            meta_spans.push(Span::raw(" "));
            meta_spans.push(Span::styled(
                format!(
                    "#{}",
                    audit_id
                        .chars()
                        .rev()
                        .take(8)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                ),
                theme.fg_dim,
            ));
        }
    }
    lines.push(Line::from(meta_spans));

    for reason in &review.reasons {
        for line in wrap_text(reason, detail_width) {
            lines.push(Line::from(vec![
                Span::styled("reason: ".to_string(), theme.fg_dim),
                Span::styled(line, theme.fg_active),
            ]));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolIcon {
    pub(crate) marker: &'static str,
    pub(crate) label: &'static str,
}

pub(crate) fn tool_icon_for(name: &str, arguments: Option<&str>) -> ToolIcon {
    let normalized_name = name.trim().to_ascii_lowercase();

    if is_python_tool(&normalized_name, arguments) {
        return ToolIcon {
            marker: "🐍",
            label: "python",
        };
    }
    if is_web_tool(&normalized_name) {
        return ToolIcon {
            marker: "🌐",
            label: "web",
        };
    }
    if tool_names::GUIDELINE_TOOLS.contains(&normalized_name.as_str()) {
        return ToolIcon {
            marker: "📖",
            label: "guide",
        };
    }
    if tool_names::SKILL_TOOLS.contains(&normalized_name.as_str()) {
        return ToolIcon {
            marker: "🧠",
            label: "skill",
        };
    }
    if normalized_name.contains(tool_names::GENERATED_TOOL_FRAGMENT) {
        return ToolIcon {
            marker: "🧠",
            label: "skill",
        };
    }
    if is_plugin_tool(&normalized_name) {
        return ToolIcon {
            marker: "🔌",
            label: "plugin",
        };
    }
    if is_collaboration_tool(&normalized_name) {
        return ToolIcon {
            marker: "👥",
            label: "collab",
        };
    }
    if is_memory_tool(&normalized_name) {
        return ToolIcon {
            marker: "◈",
            label: "memory",
        };
    }
    if is_git_tool(&normalized_name) {
        return ToolIcon {
            marker: "⑂",
            label: "git",
        };
    }
    if is_file_tool(&normalized_name) {
        return ToolIcon {
            marker: "📄",
            label: "file",
        };
    }
    if is_search_tool(&normalized_name) {
        return ToolIcon {
            marker: "🔎",
            label: "search",
        };
    }
    if is_workspace_tool(&normalized_name) {
        return ToolIcon {
            marker: "▦",
            label: "workspace",
        };
    }
    if is_communication_tool(&normalized_name) {
        return ToolIcon {
            marker: "✉",
            label: "comm",
        };
    }
    if is_audio_tool(&normalized_name) {
        return ToolIcon {
            marker: "♪",
            label: "audio",
        };
    }
    if is_image_tool(&normalized_name) {
        return ToolIcon {
            marker: "🖼",
            label: "image",
        };
    }
    if is_system_tool(&normalized_name) {
        return ToolIcon {
            marker: "⚙",
            label: "system",
        };
    }
    if is_model_tool(&normalized_name) {
        return ToolIcon {
            marker: "◇",
            label: "model",
        };
    }
    if is_agent_tool(&normalized_name) {
        return ToolIcon {
            marker: "🤖",
            label: "agent",
        };
    }
    if is_todo_tool(&normalized_name) {
        return ToolIcon {
            marker: "☑",
            label: "todo",
        };
    }
    if is_goal_tool(&normalized_name) {
        return ToolIcon {
            marker: "🎯",
            label: "goal",
        };
    }
    if is_routine_tool(&normalized_name) {
        return ToolIcon {
            marker: "↻",
            label: "routine",
        };
    }
    if is_trigger_tool(&normalized_name) {
        return ToolIcon {
            marker: "⚡",
            label: "trigger",
        };
    }
    if is_workflow_tool(&normalized_name) {
        return ToolIcon {
            marker: "⇄",
            label: "workflow",
        };
    }
    if is_debate_tool(&normalized_name) {
        return ToolIcon {
            marker: "⚖",
            label: "debate",
        };
    }
    if is_task_tool(&normalized_name) {
        return ToolIcon {
            marker: "◷",
            label: "task",
        };
    }
    if is_thread_tool(&normalized_name) {
        return ToolIcon {
            marker: "🧵",
            label: "thread",
        };
    }
    if is_terminal_tool(&normalized_name) {
        return ToolIcon {
            marker: "⌨",
            label: "terminal",
        };
    }

    ToolIcon {
        marker: "\u{2699}",
        label: "tool",
    }
}

pub(crate) fn is_python_tool(normalized_name: &str, arguments: Option<&str>) -> bool {
    if normalized_name == tool_names::PYTHON_EXECUTE || normalized_name.contains("python") {
        return true;
    }

    let Some(args) = parse_tool_arguments_object(arguments) else {
        return false;
    };
    let language_hint = args
        .get("language_hint")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if language_hint.contains("python") {
        return true;
    }

    let command = args
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    command_uses_python(&command)
}

pub(crate) fn parse_tool_arguments_object(
    arguments: Option<&str>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let arguments = arguments?;
    let value = serde_json::from_str::<serde_json::Value>(arguments).ok()?;
    value.as_object().cloned()
}

pub(crate) fn command_uses_python(command: &str) -> bool {
    !command.is_empty()
        && (command.starts_with("python ")
            || command.starts_with("python3 ")
            || command.starts_with("python -")
            || command.starts_with("python3 -")
            || command.starts_with("uv run python ")
            || command.contains(" python ")
            || command.contains(" python3 "))
}

pub(crate) fn is_web_tool(normalized_name: &str) -> bool {
    tool_names::WEB_TOOLS.contains(&normalized_name)
        || normalized_name.starts_with(tool_names::BROWSER_TOOL_PREFIX)
        || normalized_name.contains(tool_names::WEB_BROWSING_TOOL_FRAGMENT)
}

pub(crate) fn is_terminal_tool(normalized_name: &str) -> bool {
    tool_names::TERMINAL_TOOLS.contains(&normalized_name)
        || normalized_name.contains(tool_names::TERMINAL_TOOL_FRAGMENT)
}

pub(crate) fn is_file_tool(normalized_name: &str) -> bool {
    tool_names::FILE_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_git_tool(normalized_name: &str) -> bool {
    tool_names::GIT_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_search_tool(normalized_name: &str) -> bool {
    tool_names::SEARCH_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_memory_tool(normalized_name: &str) -> bool {
    tool_names::MEMORY_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_workspace_tool(normalized_name: &str) -> bool {
    tool_names::WORKSPACE_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_communication_tool(normalized_name: &str) -> bool {
    tool_names::COMMUNICATION_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_audio_tool(normalized_name: &str) -> bool {
    tool_names::AUDIO_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_image_tool(normalized_name: &str) -> bool {
    tool_names::IMAGE_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_system_tool(normalized_name: &str) -> bool {
    tool_names::SYSTEM_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_model_tool(normalized_name: &str) -> bool {
    tool_names::MODEL_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_agent_tool(normalized_name: &str) -> bool {
    tool_names::AGENT_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_task_tool(normalized_name: &str) -> bool {
    tool_names::TASK_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_todo_tool(normalized_name: &str) -> bool {
    tool_names::TODO_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_goal_tool(normalized_name: &str) -> bool {
    tool_names::GOAL_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_routine_tool(normalized_name: &str) -> bool {
    tool_names::ROUTINE_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_trigger_tool(normalized_name: &str) -> bool {
    tool_names::TRIGGER_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_workflow_tool(normalized_name: &str) -> bool {
    tool_names::WORKFLOW_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_debate_tool(normalized_name: &str) -> bool {
    tool_names::DEBATE_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_collaboration_tool(normalized_name: &str) -> bool {
    tool_names::COLLABORATION_TOOLS.contains(&normalized_name)
}

pub(crate) fn is_plugin_tool(normalized_name: &str) -> bool {
    normalized_name == tool_names::PLUGIN_API_CALL
        || normalized_name.starts_with(tool_names::PLUGIN_TOOL_PREFIX)
        || normalized_name.contains(tool_names::PLUGIN_TOOL_FRAGMENT)
}

pub(crate) fn is_thread_tool(normalized_name: &str) -> bool {
    tool_names::THREAD_TOOLS.contains(&normalized_name)
}

/// Render markdown content into Lines using tui-markdown.
/// Converts from ratatui_core types to ratatui types.
pub(crate) fn render_markdown_pub(content: &str, width: usize) -> Vec<Line<'static>> {
    render_markdown(content, width)
}

thread_local! {
    static MARKDOWN_LINE_CACHE_REVISION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static MARKDOWN_LINE_CACHE: std::cell::RefCell<
        std::collections::HashMap<(u64, usize), Vec<Line<'static>>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

pub(crate) fn prepare_markdown_render_cache(render_revision: u64) {
    MARKDOWN_LINE_CACHE_REVISION.with(|revision| {
        if revision.get() == render_revision {
            return;
        }
        revision.set(render_revision);
        MARKDOWN_LINE_CACHE.with(|cache| cache.borrow_mut().clear());
    });
}

fn markdown_cache_key(content: &str, width: usize) -> (u64, usize) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    (hasher.finish(), width)
}

#[cfg(test)]
thread_local! {
    static MARKDOWN_RENDER_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_markdown_render_call_count() {
    MARKDOWN_RENDER_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn markdown_render_call_count() -> usize {
    MARKDOWN_RENDER_CALLS.with(std::cell::Cell::get)
}

pub(crate) fn normalize_markdown_for_tui(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len());
    let mut active_fence: Option<(char, usize)> = None;

    for segment in content.split_inclusive('\n') {
        let (line, newline) = match segment.strip_suffix('\n') {
            Some(line) => (line, "\n"),
            None => (segment, ""),
        };
        let trimmed = line.trim_start_matches([' ', '\t']);
        let leading_len = line.len().saturating_sub(trimmed.len());
        let leading = &line[..leading_len];

        if let Some((marker, marker_len)) = fence_marker(trimmed) {
            let rest = &trimmed[marker_len..];

            match active_fence {
                Some((active_marker, active_len))
                    if marker == active_marker
                        && marker_len >= active_len
                        && rest.trim().is_empty() =>
                {
                    active_fence = None;
                    normalized.push_str(line);
                    normalized.push_str(newline);
                    continue;
                }
                None => {
                    active_fence = Some((marker, marker_len));
                    if rest.trim().is_empty() {
                        normalized.push_str(leading);
                        normalized.extend(std::iter::repeat_n(marker, marker_len));
                        normalized.push_str("text");
                        normalized.push_str(newline);
                        continue;
                    }
                }
                _ => {}
            }
        }

        normalized.push_str(line);
        normalized.push_str(newline);
    }

    normalized
}

pub(crate) fn fence_marker(line: &str) -> Option<(char, usize)> {
    let mut chars = line.chars();
    let marker = chars.next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let marker_len = line.chars().take_while(|ch| *ch == marker).count();
    if marker_len < 3 {
        return None;
    }

    Some((marker, marker_len))
}

pub(crate) fn render_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    if content.is_empty() {
        return vec![];
    }

    let cache_key = markdown_cache_key(content, width);
    if let Some(cached) = MARKDOWN_LINE_CACHE.with(|cache| cache.borrow().get(&cache_key).cloned())
    {
        return cached;
    }

    #[cfg(test)]
    MARKDOWN_RENDER_CALLS.with(|calls| calls.set(calls.get() + 1));

    let raw_lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut markdown_buffer = String::new();
    let mut idx = 0usize;

    while idx < raw_lines.len() {
        if is_markdown_table_start(&raw_lines, idx) {
            if !markdown_buffer.is_empty() {
                result.extend(render_markdown_segment(&markdown_buffer, width));
                markdown_buffer.clear();
            }
            let start = idx;
            idx += 2;
            while idx < raw_lines.len() && is_markdown_table_row(raw_lines[idx]) {
                idx += 1;
            }
            result.extend(render_markdown_table(&raw_lines[start..idx], width));
            continue;
        }

        markdown_buffer.push_str(raw_lines[idx]);
        if idx + 1 < raw_lines.len() {
            markdown_buffer.push('\n');
        }
        idx += 1;
    }

    if !markdown_buffer.is_empty() {
        result.extend(render_markdown_segment(&markdown_buffer, width));
    }

    MARKDOWN_LINE_CACHE.with(|cache| {
        cache.borrow_mut().insert(cache_key, result.clone());
    });

    result
}
