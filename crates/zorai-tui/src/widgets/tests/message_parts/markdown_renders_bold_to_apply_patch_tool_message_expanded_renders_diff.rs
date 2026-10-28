use super::super::*;
use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
use ratatui::style::Modifier;
use ratatui::text::Line;

pub(super) fn empty_expanded() -> ExpandedReasoning {
    ExpandedReasoning::new()
}

pub(super) fn empty_tools() -> ExpandedTools {
    ExpandedTools::new()
}

pub(super) fn plain_lines(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn markdown_renders_bold() {
    let lines = render_markdown("**bold text** normal", 80);
    assert!(!lines.is_empty(), "Markdown should produce lines");
    let has_bold = lines.iter().any(|line| {
        line.spans.iter().any(|span| {
            span.style
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        })
    });
    let debug: Vec<Vec<String>> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| format!("'{}' mods={:?}", span.content, span.style.add_modifier))
                .collect()
        })
        .collect();
    assert!(has_bold, "Expected BOLD in markdown output: {:?}", debug);
}

#[test]
fn markdown_heading_keeps_line_style() {
    let lines = render_markdown("## Heading", 80);
    assert!(!lines.is_empty());
    assert!(
        lines[0].style.add_modifier.contains(Modifier::BOLD),
        "Expected heading line style to keep bold modifier, got {:?}",
        lines[0].style
    );
}

#[test]
fn markdown_wraps_to_requested_width() {
    let lines = render_markdown("**alpha beta gamma delta**", 10);
    assert!(
        lines.len() > 1,
        "Expected markdown to wrap, got {:?}",
        lines
    );
}

#[test]
fn markdown_empty_code_fence_language_is_normalized() {
    let sanitized = normalize_markdown_for_tui("before\n```\nplain text\n```\nafter");

    assert!(
        sanitized.contains("```text\nplain text\n```"),
        "expected empty fence language to be normalized, got {sanitized:?}"
    );
}

#[test]
fn markdown_tables_render_as_columns() {
    let lines = render_markdown(
        "| Skill | Size | Purpose |\n|---|---|---|\n| zorai-rust-dev.md | 3.4KB | Build and test Rust crates |",
        80,
    );
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(
        plain.iter().any(|line| line.contains("│")),
        "Expected rendered column separators, got {:?}",
        plain
    );
    assert!(
        plain.iter().all(|line| !line.contains("|---")),
        "Expected markdown separator row to be rendered, got {:?}",
        plain
    );
}

#[test]
fn markdown_tables_wrap_long_cells_instead_of_truncating() {
    let lines = render_markdown(
        "| Spec | Idea | Why |\n|---|---|---|\n| NEGATIVE_KNOWLEDGE | The agent should track negative knowledge explicitly instead of compressing it into binary success and failure states | This preserves the actual content for the operator |",
        40,
    );
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert!(
        plain.len() > 3,
        "Expected wrapped multi-line table rows, got {:?}",
        plain
    );
    assert!(
        plain.iter().all(|line| !line.contains('…')),
        "Expected wrapped cells without truncation, got {:?}",
        plain
    );
    let joined = plain.join("\n");
    assert!(
        joined.contains("The agent") && joined.contains("negative") && joined.contains("states"),
        "Expected wrapped table output to preserve the long cell content, got {:?}",
        plain
    );
}

#[test]
fn wrap_text_basic() {
    let lines = wrap_text("hello world foo bar", 12);
    assert_eq!(lines, vec!["hello world", "foo bar"]);
}

#[test]
fn wrap_text_preserves_newlines() {
    let lines = wrap_text("line1\nline2", 80);
    assert_eq!(lines, vec!["line1", "line2"]);
}

#[test]
fn user_message_has_badge() {
    let msg = AgentMessage {
        role: MessageRole::User,
        content: "Hello".into(),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    assert!(!lines.is_empty());
}

#[test]
fn tool_icon_classifies_action_families() {
    use zorai_protocol::tool_names;

    assert_eq!(tool_icon_for(tool_names::WEB_SEARCH, None).label, "web");
    assert_eq!(tool_icon_for(tool_names::WEB_SEARCH, None).marker, "🌐");
    assert_eq!(tool_icon_for(tool_names::FETCH_URL, None).label, "web");
    assert_eq!(
        tool_icon_for(tool_names::BROWSER_NAVIGATE, None).label,
        "web"
    );
    assert_eq!(
        tool_icon_for(tool_names::READ_GUIDELINE, None).label,
        "guide"
    );
    assert_eq!(tool_icon_for(tool_names::READ_GUIDELINE, None).marker, "📖");
    assert_eq!(
        tool_icon_for(tool_names::DISCOVER_SKILLS, None).label,
        "skill"
    );
    assert_eq!(
        tool_icon_for(tool_names::DISCOVER_SKILLS, None).marker,
        "🧠"
    );
    assert_eq!(
        tool_icon_for(tool_names::PYTHON_EXECUTE, None).label,
        "python"
    );
    assert_eq!(
        tool_icon_for(tool_names::BASH_COMMAND, None).label,
        "terminal"
    );
    assert_eq!(tool_icon_for(tool_names::BASH_COMMAND, None).marker, "⌨");
    assert_eq!(
        tool_icon_for(
            tool_names::BASH_COMMAND,
            Some(r#"{"command":"python3 -c \"print('ok')\""}"#)
        )
        .label,
        "python"
    );

    for (tool_name, expected_label) in [
        (tool_names::READ_FILE, "file"),
        (tool_names::APPLY_PATCH, "file"),
        (tool_names::SEARCH_FILES, "search"),
        (tool_names::SEARCH_HISTORY, "search"),
        (tool_names::READ_MEMORY, "memory"),
        (tool_names::AGENT_QUERY_MEMORY, "memory"),
        (tool_names::LIST_WORKSPACES, "workspace"),
        (tool_names::SPLIT_PANE, "workspace"),
        (tool_names::SEND_SLACK_MESSAGE, "comm"),
        (tool_names::NOTIFY_USER, "comm"),
        (tool_names::SPEECH_TO_TEXT, "audio"),
        (tool_names::ANALYZE_IMAGE, "image"),
        (tool_names::GET_SYSTEM_INFO, "system"),
        (tool_names::GET_COST_SUMMARY, "system"),
        (tool_names::GET_GIT_STATUS, "git"),
        (tool_names::GET_GIT_LINE_STATUSES, "git"),
        (tool_names::LIST_PROVIDERS, "model"),
        (tool_names::SWITCH_MODEL, "model"),
        (tool_names::SPAWN_SUBAGENT, "agent"),
        (tool_names::HANDOFF_THREAD_AGENT, "agent"),
        (tool_names::UPDATE_TODO, "todo"),
        (tool_names::GET_TODOS, "todo"),
        (tool_names::LIST_TODOS, "todo"),
        (tool_names::ENQUEUE_TASK, "task"),
        (tool_names::START_GOAL_RUN, "goal"),
        (tool_names::CREATE_ROUTINE, "routine"),
        (tool_names::ADD_TRIGGER, "trigger"),
        (tool_names::RUN_WORKFLOW_PACK, "workflow"),
        (tool_names::RUN_DEBATE, "debate"),
        (tool_names::BROADCAST_CONTRIBUTION, "collab"),
        (tool_names::PLUGIN_API_CALL, "plugin"),
        (tool_names::SYNTHESIZE_TOOL, "skill"),
        (tool_names::LIST_THREADS, "thread"),
    ] {
        assert_eq!(
            tool_icon_for(tool_name, None).label,
            expected_label,
            "{tool_name}"
        );
    }

    assert_eq!(tool_icon_for(tool_names::GET_GIT_STATUS, None).marker, "⑂");
    assert_eq!(tool_icon_for(tool_names::UPDATE_TODO, None).marker, "☑");
    assert_eq!(tool_icon_for(tool_names::ANALYZE_IMAGE, None).marker, "🖼");
}

#[test]
fn tool_message_shows_action_icon() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some(zorai_protocol::tool_names::WEB_SEARCH.into()),
        tool_status: Some("done".into()),
        content: "some output here".into(),
        ..Default::default()
    };
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &empty_tools(),
    );
    assert_eq!(lines.len(), 1);
    let plain = plain_lines(&lines).join("");
    assert!(
        plain.contains("Web Search"),
        "expected action-specific web icon label, got: {plain}"
    );
}

#[test]
fn tool_message_expanded_shows_details() {
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some("ls -la /home/user".into()),
        content: "total 208\ndrwxr-xr-x 15 user user 4096 Jan 1 00:00 .".into(),
        ..Default::default()
    };
    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &exp_tools,
    );
    assert!(
        lines.len() > 1,
        "Expanded tool should have more than 1 line, got {}",
        lines.len()
    );
}

#[test]
fn tool_message_expanded_preserves_full_arguments_and_result() {
    let long_args = serde_json::json!({
        "command": "python - <<'PY'\n".to_string() + &"x".repeat(120) + "\nPY",
    })
    .to_string();
    let long_result = (0..8)
        .map(|index| format!("line-{index}: {}", "y".repeat(40)))
        .collect::<Vec<_>>()
        .join("\n");
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("bash_command".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(long_args.clone()),
        content: long_result.clone(),
        ..Default::default()
    };

    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        50,
        &empty_expanded(),
        &exp_tools,
    );

    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("python -"),
        "missing argument prefix: {plain}"
    );
    assert!(plain.contains("<<'PY'"), "missing heredoc marker: {plain}");
    assert!(
        plain.chars().filter(|ch| *ch == 'x').count() >= 100,
        "missing long argument body: {plain}"
    );
    assert!(
        plain.contains("line-7:"),
        "missing later result lines: {plain}"
    );
    assert!(
        plain.contains(&"y".repeat(30)),
        "missing long result body: {plain}"
    );
    assert!(
        !plain.contains("..."),
        "expanded tool output should not be truncated: {plain}"
    );
}

#[test]
fn apply_patch_tool_message_expanded_renders_diff_like_sections() {
    let patch = [
        "*** Begin Patch",
        "*** Update File: /tmp/example.rs",
        "@@ fn example()",
        "-    let before = 1;",
        "+    let after = 2;",
        "*** End Patch",
    ]
    .join("\n");
    let msg = AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("apply_patch".into()),
        tool_status: Some("done".into()),
        tool_arguments: Some(serde_json::json!({ "input": patch }).to_string()),
        content: "Patch applied successfully".into(),
        ..Default::default()
    };

    let mut exp_tools = empty_tools();
    exp_tools.insert(0);
    let lines = message_to_lines(
        &msg,
        0,
        TranscriptMode::Compact,
        &ThemeTokens::default(),
        80,
        &empty_expanded(),
        &exp_tools,
    );

    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("/tmp/example.rs"),
        "expected file header in rendered diff, got: {plain}"
    );
    assert!(
        plain.contains("-    let before = 1;"),
        "expected removed line in rendered diff, got: {plain}"
    );
    assert!(
        plain.contains("+    let after = 2;"),
        "expected added line in rendered diff, got: {plain}"
    );
    assert!(
        !plain.contains("{\"input\":"),
        "expected formatted diff instead of raw JSON arguments, got: {plain}"
    );
}
