use super::*;
use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
use crate::widgets::message_operator_question::render_operator_question_message;
use crate::widgets::tool_diff::{
    render_tool_edit_diff, render_tool_structured_json, ToolStructuredValueSource,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
pub(crate) fn render_markdown_segment(content: &str, width: usize) -> Vec<Line<'static>> {
    let normalized = normalize_markdown_for_tui(content);
    let md_text = tui_markdown::from_str(&normalized);
    let mut result = Vec::new();
    for md_line in md_text.lines {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for md_span in md_line.spans {
            let s = convert_style(md_span.style);
            spans.push(Span::styled(md_span.content.to_string(), s));
        }
        result.push(Line::from(spans).style(convert_style(md_line.style)));
    }
    if result.is_empty() {
        wrap_text(content, width)
            .into_iter()
            .map(|s| Line::from(Span::raw(s)))
            .collect()
    } else {
        wrap_styled_lines(result, width)
    }
}

pub(crate) fn convert_style(style: ratatui_core::style::Style) -> Style {
    let mut result = Style::default();

    if let Some(fg) = style.fg {
        result = result.fg(convert_color(fg));
    }
    if let Some(bg) = style.bg {
        result = result.bg(convert_color(bg));
    }

    let add = convert_modifier(style.add_modifier);
    if !add.is_empty() {
        result = result.add_modifier(add);
    }

    let remove = convert_modifier(style.sub_modifier);
    if !remove.is_empty() {
        result = result.remove_modifier(remove);
    }

    result
}

pub(crate) fn convert_modifier(modifier: ratatui_core::style::Modifier) -> Modifier {
    let mut result = Modifier::empty();

    if modifier.contains(ratatui_core::style::Modifier::BOLD) {
        result |= Modifier::BOLD;
    }
    if modifier.contains(ratatui_core::style::Modifier::DIM) {
        result |= Modifier::DIM;
    }
    if modifier.contains(ratatui_core::style::Modifier::ITALIC) {
        result |= Modifier::ITALIC;
    }
    if modifier.contains(ratatui_core::style::Modifier::UNDERLINED) {
        result |= Modifier::UNDERLINED;
    }
    if modifier.contains(ratatui_core::style::Modifier::SLOW_BLINK) {
        result |= Modifier::SLOW_BLINK;
    }
    if modifier.contains(ratatui_core::style::Modifier::RAPID_BLINK) {
        result |= Modifier::RAPID_BLINK;
    }
    if modifier.contains(ratatui_core::style::Modifier::REVERSED) {
        result |= Modifier::REVERSED;
    }
    if modifier.contains(ratatui_core::style::Modifier::HIDDEN) {
        result |= Modifier::HIDDEN;
    }
    if modifier.contains(ratatui_core::style::Modifier::CROSSED_OUT) {
        result |= Modifier::CROSSED_OUT;
    }

    result
}

pub(crate) fn convert_color(c: ratatui_core::style::Color) -> Color {
    match c {
        ratatui_core::style::Color::Reset => Color::Reset,
        ratatui_core::style::Color::Black => Color::Black,
        ratatui_core::style::Color::Red => Color::Red,
        ratatui_core::style::Color::Green => Color::Green,
        ratatui_core::style::Color::Yellow => Color::Yellow,
        ratatui_core::style::Color::Blue => Color::Blue,
        ratatui_core::style::Color::Magenta => Color::Magenta,
        ratatui_core::style::Color::Cyan => Color::Cyan,
        ratatui_core::style::Color::Gray => Color::Gray,
        ratatui_core::style::Color::White => Color::White,
        ratatui_core::style::Color::Indexed(i) => Color::Indexed(i),
        ratatui_core::style::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
        _ => Color::Reset,
    }
}

/// Set of message indices whose reasoning blocks are expanded
pub type ExpandedReasoning = std::collections::HashSet<usize>;
/// Set of message indices whose tool details are expanded
pub type ExpandedTools = std::collections::HashSet<usize>;

pub(crate) fn is_meta_cognition_message(msg: &AgentMessage) -> bool {
    matches!(msg.role, MessageRole::Assistant | MessageRole::System)
        && is_meta_cognition_content(&msg.content)
}

pub(crate) fn is_collapsible_system_notice_message(msg: &AgentMessage) -> bool {
    collapsible_system_notice_label(msg).is_some()
}

pub(crate) fn collapsible_system_notice_label(msg: &AgentMessage) -> Option<&'static str> {
    if msg.message_kind == "compaction_artifact" {
        if compaction_artifact_was_manual(msg) {
            Some("Manual compaction")
        } else {
            Some("Auto compaction")
        }
    } else if is_meta_cognition_message(msg) {
        Some("🕵🏻‍♂️ Meta-cognition")
    } else if msg.role == MessageRole::System {
        background_operation_finished_label(&msg.content)
    } else {
        None
    }
}

pub(crate) fn compaction_artifact_was_manual(msg: &AgentMessage) -> bool {
    msg.content
        .lines()
        .any(|line| line.trim_start().starts_with("Trigger: manual-request"))
}

pub(crate) fn collapsible_system_notice_detail(msg: &AgentMessage) -> Option<String> {
    collapsible_system_notice_label(msg)?;

    if msg.message_kind == "compaction_artifact" {
        Some(compaction_artifact_content(msg))
    } else {
        Some(msg.content.clone())
    }
}

pub(crate) fn compaction_artifact_content(msg: &AgentMessage) -> String {
    let visible_header = msg.content.trim();
    let payload = msg
        .compaction_payload
        .as_deref()
        .map(str::trim)
        .filter(|payload| !payload.is_empty());

    match payload {
        Some(payload) if visible_header.is_empty() => payload.to_string(),
        Some(payload) if visible_header.contains(payload) => visible_header.to_string(),
        Some(payload) => format!("{visible_header}\n\nContent:\n{payload}"),
        None => msg.content.clone(),
    }
}

pub(crate) fn is_meta_cognition_content(content: &str) -> bool {
    content
        .trim_start()
        .starts_with("Meta-cognitive intervention")
}

pub(crate) fn background_operation_finished_label(content: &str) -> Option<&'static str> {
    let content = content.trim_start();
    // Compaction summary messages reuse the "Background operation finished."
    // prefix so they pipe through this collapsible notice path, but the daemon
    // tags the second line with the strategy/trigger so we can detect them and
    // give them a distinct label that matches the user's mental model.
    if content.contains("compaction applied:") {
        if content.contains("Manual compaction applied:") {
            return Some("🗜 Manual compaction completed");
        }
        return Some("🗜 Auto compaction completed");
    }
    if content.starts_with("Background operations finished.") {
        Some("🖥️ Background operations finished")
    } else if content.starts_with("Background operation finished.") {
        Some("🖥️ Background operation finished")
    } else {
        None
    }
}

pub(crate) fn toggle_glyph(expanded: bool) -> &'static str {
    if expanded {
        "\u{25be}"
    } else {
        "\u{25b6}"
    }
}

fn render_collapsible_detail_lines(
    content: &str,
    detail_width: usize,
    dark_blue: Style,
) -> Vec<Line<'static>> {
    render_markdown(content, detail_width)
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::styled("\u{2502}", dark_blue), Span::raw(" ")];
            spans.extend(line.spans);
            Line::from(spans).style(line.style)
        })
        .collect()
}

pub(crate) fn estimated_collapsible_system_notice_line_count(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    content_width: usize,
    expanded: &ExpandedReasoning,
) -> usize {
    let reasoning_expanded = matches!(mode, TranscriptMode::Full) || expanded.contains(&msg_index);
    let detail_width = content_width.saturating_sub(2).max(1);
    let mut count = 1usize;

    if msg.message_kind == "compaction_artifact" {
        count = count.saturating_add(wrap_text(msg.content.trim(), detail_width).len().max(1));
        if reasoning_expanded {
            let visible = msg.content.trim();
            if let Some(payload) = msg
                .compaction_payload
                .as_deref()
                .map(str::trim)
                .filter(|payload| !payload.is_empty() && !visible.contains(payload))
            {
                count = count.saturating_add(1);
                count = count.saturating_add(wrap_text(payload, detail_width).len().max(1));
            }
        }
        return count;
    }

    if reasoning_expanded {
        let detail = collapsible_system_notice_detail(msg).unwrap_or_default();
        count = count.saturating_add(wrap_text(&detail, detail_width).len().max(1));
    }

    count
}

/// Convert a message into ratatui Lines (all owned/static)
pub(crate) fn message_to_lines(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match mode {
        TranscriptMode::Compact => render_compact(
            msg,
            msg_index,
            theme,
            width,
            expanded,
            expanded_tools,
            &mut lines,
        ),
        TranscriptMode::Tools => render_tools_only(msg, theme, width, &mut lines),
        TranscriptMode::Full => render_full(
            msg,
            msg_index,
            theme,
            width,
            expanded,
            expanded_tools,
            &mut lines,
        ),
    }

    lines
}

pub(crate) fn render_compact(
    msg: &AgentMessage,
    msg_index: usize,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
    lines: &mut Vec<Line<'static>>,
) {
    let content_width = width.max(1);
    let image_lines = inline_image_attachment_lines(msg, content_width, theme);

    if let Some(operator_question_lines) =
        render_operator_question_message(msg, theme, content_width)
    {
        lines.extend(operator_question_lines);
        return;
    }

    if msg.role == MessageRole::Tool {
        if let Some(name) = &msg.tool_name {
            let status = msg.tool_status.as_deref().unwrap_or("done");
            let (status_text, status_style) = format_tool_status(status, theme);
            let is_expanded = expanded_tools.contains(&msg_index);
            let tool_icon = tool_icon_for(name, msg.tool_arguments.as_deref());
            let mut header_spans = vec![
                Span::styled(toggle_glyph(is_expanded), theme.fg_dim),
                Span::raw(" "),
                Span::styled(tool_icon.marker, theme.accent_assistant),
                Span::raw("  "),
                Span::styled(name.clone(), theme.fg_dim),
                Span::raw(" "),
                Span::styled(status_text, status_style),
            ];

            if let Some(review) = msg.weles_review.as_ref() {
                let (badge, badge_style) = format_weles_review_badge(review, theme);
                header_spans.push(Span::raw(" "));
                header_spans.push(Span::styled(badge, badge_style));
            }
            lines.push(Line::from(header_spans));

            if is_expanded {
                let detail_indent = 4;
                let detail_width = width.saturating_sub(detail_indent + 1);

                if let Some(review) = msg.weles_review.as_ref() {
                    render_weles_review_details(review, theme, detail_width, lines);
                }

                if let Some(args) = &msg.tool_arguments {
                    if !args.is_empty() {
                        if let Some(diff_lines) =
                            render_tool_edit_diff(name, args, theme, detail_width.max(1))
                        {
                            lines.push(Line::from(vec![Span::styled("changes:", theme.fg_dim)]));
                            lines.extend(diff_lines);
                        } else if let Some(structured_args) = render_tool_structured_json(
                            name,
                            ToolStructuredValueSource::Arguments,
                            args,
                            theme,
                            detail_width.max(1),
                        ) {
                            lines.push(Line::from(vec![Span::styled("args:", theme.fg_dim)]));
                            lines.extend(structured_args);
                        } else {
                            let mut rendered_arg_lines =
                                wrap_text(args, detail_width.max(1)).into_iter();
                            if let Some(first_line) = rendered_arg_lines.next() {
                                lines.push(Line::from(vec![
                                    Span::styled("args: ", theme.fg_dim),
                                    Span::styled(first_line, theme.fg_active),
                                ]));
                                for line in rendered_arg_lines {
                                    lines.push(Line::from(vec![
                                        Span::styled("      ", theme.fg_dim),
                                        Span::styled(line, theme.fg_active),
                                    ]));
                                }
                            }
                        }
                    }
                }

                let result_text = &msg.content;
                if !result_text.is_empty() {
                    if let Some(structured_result) = render_tool_structured_json(
                        name,
                        ToolStructuredValueSource::Result,
                        result_text,
                        theme,
                        detail_width.max(1),
                    ) {
                        lines.push(Line::from(vec![Span::styled("result:", theme.fg_dim)]));
                        lines.extend(structured_result);
                    } else {
                        let mut result_line_index = 0usize;
                        for result_line in result_text.lines() {
                            let wrapped_result = wrap_text(result_line, detail_width.max(1));
                            for wrapped_line in wrapped_result {
                                let prefix = if result_line_index == 0 {
                                    "result: "
                                } else {
                                    "        "
                                };
                                lines.push(Line::from(vec![
                                    Span::styled(prefix.to_string(), theme.fg_dim),
                                    Span::styled(wrapped_line, theme.fg_active),
                                ]));
                                result_line_index += 1;
                            }
                        }
                    }
                }
            }
        }
        return;
    }

    if let Some(label) = collapsible_system_notice_label(msg) {
        let is_compaction = msg.message_kind == "compaction_artifact";
        let is_expanded = expanded.contains(&msg_index);
        let detail_width = width.saturating_sub(2).max(1);
        let dark_blue = Style::default().fg(Color::Indexed(24));
        lines.push(Line::from(vec![Span::styled(
            format!("{} {label}", toggle_glyph(is_expanded)),
            theme.meta_cognitive,
        )]));

        if is_compaction {
            let visible = msg.content.trim();
            for detail_line in wrap_text(visible, detail_width) {
                lines.push(Line::from(vec![
                    Span::styled("\u{2502}", dark_blue),
                    Span::raw(" "),
                    Span::styled(detail_line, theme.fg_dim),
                ]));
            }
            if is_expanded {
                let payload = msg
                    .compaction_payload
                    .as_deref()
                    .map(str::trim)
                    .filter(|payload| !payload.is_empty() && !visible.contains(payload));
                if let Some(payload) = payload {
                    lines.push(Line::from(vec![
                        Span::styled("\u{2502}", dark_blue),
                        Span::raw(" "),
                    ]));
                    lines.extend(render_collapsible_detail_lines(
                        payload,
                        detail_width,
                        dark_blue,
                    ));
                }
            }
        } else if is_expanded {
            let detail = collapsible_system_notice_detail(msg).unwrap_or_default();
            lines.extend(render_collapsible_detail_lines(
                &detail,
                detail_width,
                dark_blue,
            ));
        }

        if !image_lines.is_empty() {
            lines.extend(image_lines);
        }
        return;
    }

    let content = &msg.content;
    if content.is_empty() && image_lines.is_empty() && msg.role != MessageRole::Assistant {
        return;
    }
    if content.is_empty() && image_lines.is_empty() && msg.reasoning.is_none() {
        return;
    }

    let md_lines: Vec<Line<'static>> = if msg.role == MessageRole::Assistant {
        render_markdown(content, content_width)
    } else if msg.role == MessageRole::User {
        wrap_text(content, content_width)
            .into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_active)))
            .collect()
    } else if msg.role == MessageRole::System {
        render_markdown(content, content_width)
    } else {
        wrap_text(content, content_width)
            .into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_active)))
            .collect()
    };

    let has_reasoning = msg.role == MessageRole::Assistant
        && msg
            .reasoning
            .as_deref()
            .is_some_and(|reasoning| !reasoning.is_empty());

    if has_reasoning {
        let reasoning = msg.reasoning.as_deref().unwrap_or_default();
        let is_expanded = expanded.contains(&msg_index);
        if is_expanded {
            lines.push(Line::from(vec![Span::styled(
                format!("{} Reasoning", toggle_glyph(true)),
                theme.fg_dim,
            )]));
            let reasoning_width = width.saturating_sub(2).max(1);
            let dark_blue = Style::default().fg(Color::Indexed(24));
            for rline in wrap_text(reasoning, reasoning_width) {
                lines.push(Line::from(vec![
                    Span::styled("\u{2502}", dark_blue),
                    Span::raw(" "),
                    Span::styled(rline, theme.fg_dim),
                ]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                format!("{} Reasoning", toggle_glyph(false)),
                theme.fg_dim,
            )]));
        }

        lines.extend(md_lines);
    } else {
        lines.extend(md_lines);
    }

    if !image_lines.is_empty() {
        lines.extend(image_lines);
    }
}

pub(crate) fn inline_image_attachment_lines(
    msg: &AgentMessage,
    width: usize,
    theme: &ThemeTokens,
) -> Vec<Line<'static>> {
    let Some(path) = crate::widgets::chat::message_image_preview_path(msg) else {
        return Vec::new();
    };
    crate::widgets::image_preview::render_image_preview_lines(&path, width, 12, theme)
}

pub(crate) fn render_tools_only(
    msg: &AgentMessage,
    theme: &ThemeTokens,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    if msg.role != MessageRole::Tool && msg.tool_name.is_none() {
        return;
    }

    if let Some(name) = &msg.tool_name {
        let status = msg.tool_status.as_deref().unwrap_or("done");
        let (status_text, status_style) = format_tool_status(status, theme);
        let args_preview = msg.tool_arguments.as_deref().unwrap_or("");
        let tool_icon = tool_icon_for(name, msg.tool_arguments.as_deref());
        let max_args = width.saturating_sub(30);
        let args_short = truncate_to_width(args_preview, max_args);

        let mut spans = vec![
            Span::styled(tool_icon.marker, theme.accent_assistant),
            Span::raw("  "),
            Span::styled(name.clone(), theme.fg_active),
            Span::raw(" "),
            Span::styled(status_text, status_style),
        ];

        if !args_short.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(args_short.to_string(), theme.fg_dim));
        }

        lines.push(Line::from(spans));
    }
}

pub(crate) fn render_full(
    msg: &AgentMessage,
    msg_index: usize,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
    lines: &mut Vec<Line<'static>>,
) {
    let mut full_expanded = expanded.clone();
    full_expanded.insert(msg_index);
    let mut full_tools = expanded_tools.clone();
    full_tools.insert(msg_index);
    render_compact(
        msg,
        msg_index,
        theme,
        width,
        &full_expanded,
        &full_tools,
        lines,
    );
}

pub(crate) fn format_tool_status(status: &str, theme: &ThemeTokens) -> (&'static str, Style) {
    match status {
        "completed" | "done" | "success" => ("\u{2713} done", theme.accent_success),
        "error" | "failed" => ("\u{2717} error", theme.accent_danger),
        _ => ("\u{25cf} running", theme.accent_secondary),
    }
}

#[cfg(test)]
mod label_tests {
    use super::background_operation_finished_label;

    #[test]
    fn auto_compaction_summary_gets_compaction_specific_label() {
        // Why this matters: the daemon piggy-backs on the existing
        // "Background operation finished." prefix so this collapsible-notice
        // path picks up the message, but compaction summaries deserve a
        // distinct label so users can tell them apart at a glance from
        // generic background-tool finishes.
        let content = "Background operation finished.\n\nAuto compaction applied: ~180000 \u{2192} ~5000 tokens (target 160000), trigger token-threshold, strategy heuristic. Earlier history collapsed at checkpoint #20.";
        assert_eq!(
            background_operation_finished_label(content),
            Some("\u{1F5DC} Auto compaction completed")
        );
    }

    #[test]
    fn manual_compaction_summary_distinguishes_from_auto() {
        let content = "Background operation finished.\n\nManual compaction applied: ~180000 \u{2192} ~5000 tokens (target 160000), trigger manual-request, strategy heuristic. Earlier history collapsed at checkpoint #20.";
        assert_eq!(
            background_operation_finished_label(content),
            Some("\u{1F5DC} Manual compaction completed")
        );
    }

    #[test]
    fn generic_background_op_keeps_existing_label() {
        let content =
            "Background operation finished.\n\nOperation result saved to file.\n- operations: 1";
        assert_eq!(
            background_operation_finished_label(content),
            Some("\u{1F5A5}\u{FE0F} Background operation finished")
        );
    }

    #[test]
    fn unrelated_content_returns_none() {
        assert_eq!(background_operation_finished_label("hello"), None);
    }
}
