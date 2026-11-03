use super::super::build_rendered_lines_to_build_visible_window_from_snapshot_to_apply::*;
use super::super::resolved_scroll_to_highlight_line_range_to_selected_text_to_selection::*;
use super::super::*;
use crate::state::chat::{AgentMessage, ChatState, MessageRole};
use crate::theme::ThemeTokens;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
pub(crate) fn render_streaming_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    crate::widgets::message::render_markdown_pub(content, width)
}

use std::path::Path;
use zorai_protocol::tool_names;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolFileChip {
    pub path: String,
    pub label: String,
    pub tool_name: String,
}

pub(crate) fn tool_file_chip(message: &AgentMessage) -> Option<ToolFileChip> {
    let tool_name = message.tool_name.as_deref()?;
    if let Some(path) = message
        .tool_output_preview_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let label = file_name_label(path);
        return Some(ToolFileChip {
            path: path.to_string(),
            label,
            tool_name: tool_name.to_string(),
        });
    }
    if tool_name == tool_names::GENERATE_IMAGE || tool_name == tool_names::GENERATE_VIDEO {
        let path = generated_image_preview_path(message)?;
        let label = file_name_label(&path);
        return Some(ToolFileChip {
            path,
            label,
            tool_name: tool_name.to_string(),
        });
    }
    if tool_name == tool_names::READ_SKILL {
        let path = read_skill_preview_path(message)?;
        let label = file_name_label(&path);
        return Some(ToolFileChip {
            path,
            label,
            tool_name: tool_name.to_string(),
        });
    }
    if tool_name == tool_names::READ_GUIDELINE {
        let path = read_guideline_preview_path(message)?;
        let label = file_name_label(&path);
        return Some(ToolFileChip {
            path,
            label,
            tool_name: tool_name.to_string(),
        });
    }

    if !matches!(
        tool_name,
        tool_names::READ_FILE
            | tool_names::WRITE_FILE
            | tool_names::CREATE_FILE
            | tool_names::APPEND_TO_FILE
            | tool_names::REPLACE_IN_FILE
            | tool_names::APPLY_FILE_PATCH
            | tool_names::APPLY_PATCH
    ) {
        return None;
    }

    let arguments = message.tool_arguments.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let path = direct_tool_path(&value)
        .or_else(|| tool_path_from_cwd_and_filename(&value))
        .or_else(|| {
            if tool_name == tool_names::APPLY_PATCH {
                first_apply_patch_path(arguments)
            } else {
                None
            }
        })?;
    let label = file_name_label(&path);

    Some(ToolFileChip {
        path,
        label,
        tool_name: tool_name.to_string(),
    })
}

pub(crate) fn message_image_preview_path(message: &AgentMessage) -> Option<String> {
    message.content_blocks.iter().find_map(|block| match block {
        crate::state::chat::AgentContentBlock::Image { url, data_url, .. } => url
            .as_deref()
            .and_then(crate::widgets::image_preview::resolve_local_image_path)
            .or_else(|| {
                data_url
                    .as_deref()
                    .and_then(crate::widgets::image_preview::resolve_local_image_path)
            }),
        _ => None,
    })
}

pub(crate) fn generated_image_preview_path(message: &AgentMessage) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(&message.content).ok()?;
    non_empty_string_field(&value, "path").or_else(|| {
        non_empty_string_field(&value, "file_url")
            .and_then(|value| crate::widgets::image_preview::resolve_local_image_path(&value))
    })
}

pub(crate) fn tool_skill_chip(message: &AgentMessage) -> Option<String> {
    let tool_name = message.tool_name.as_deref()?;
    if tool_name != zorai_protocol::tool_names::READ_SKILL {
        return None;
    }

    let arguments = message.tool_arguments.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    non_empty_string_field(&value, "skill")
}

pub(crate) fn direct_tool_path(value: &serde_json::Value) -> Option<String> {
    ["path", "filePath", "file_path"]
        .iter()
        .find_map(|key| non_empty_string_field(value, key))
}

pub(crate) fn tool_path_from_cwd_and_filename(value: &serde_json::Value) -> Option<String> {
    let filename = non_empty_string_field(value, "filename")?;
    if let Some(cwd) = non_empty_string_field(value, "cwd") {
        Some(Path::new(&cwd).join(&filename).display().to_string())
    } else {
        Some(filename)
    }
}

pub(crate) fn non_empty_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn read_skill_preview_path(message: &AgentMessage) -> Option<String> {
    skill_path_from_result_json(&message.content)
        .or_else(|| skill_path_from_result_header(&message.content))
}

pub(crate) fn read_guideline_preview_path(message: &AgentMessage) -> Option<String> {
    guideline_path_from_result_json(&message.content)
        .or_else(|| guideline_path_from_result_header(&message.content))
}

pub(crate) fn skill_path_from_result_json(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let path = non_empty_string_field(&value, "path")?;
    Some(resolve_skill_path_for_preview(
        non_empty_string_field(&value, "skills_root"),
        &path,
    ))
}

pub(crate) fn skill_path_from_result_header(content: &str) -> Option<String> {
    let first_line = content.lines().next()?.trim();
    let raw_path = first_line.strip_prefix("Skill ")?;
    let relative_path = if let Some((path, _)) = raw_path.split_once(" [") {
        path.trim()
    } else {
        raw_path.strip_suffix(':').unwrap_or(raw_path).trim()
    };
    if relative_path.is_empty() {
        return None;
    }

    Some(resolve_skill_path_for_preview(None, relative_path))
}

pub(crate) fn guideline_path_from_result_json(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let path = non_empty_string_field(&value, "path")?;
    Some(resolve_guideline_path_for_preview(
        non_empty_string_field(&value, "guidelines_root"),
        &path,
    ))
}

pub(crate) fn guideline_path_from_result_header(content: &str) -> Option<String> {
    let first_line = content.lines().next()?.trim();
    let raw_path = first_line.strip_prefix("Guideline ")?;
    let relative_path = raw_path.strip_suffix(':').unwrap_or(raw_path).trim();
    if relative_path.is_empty() {
        return None;
    }

    Some(resolve_guideline_path_for_preview(None, relative_path))
}

pub(crate) fn resolve_skill_path_for_preview(skills_root: Option<String>, path: &str) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }

    skills_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(zorai_protocol::zorai_skills_dir)
        .join(path)
        .display()
        .to_string()
}

pub(crate) fn resolve_guideline_path_for_preview(
    guidelines_root: Option<String>,
    path: &str,
) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }

    guidelines_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(zorai_protocol::zorai_guidelines_dir)
        .join(path)
        .display()
        .to_string()
}

pub(crate) fn file_name_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(crate) fn append_tool_file_chip(
    line: &mut Line<'static>,
    message: &AgentMessage,
    theme: &ThemeTokens,
) {
    let Some(chip) = tool_file_chip(message) else {
        return;
    };

    line.spans.push(Span::raw(" "));
    line.spans.push(Span::styled(
        format!("[{}]", chip.label),
        theme.accent_primary,
    ));
}

pub(crate) fn append_tool_skill_chip(
    line: &mut Line<'static>,
    message: &AgentMessage,
    theme: &ThemeTokens,
) {
    if tool_file_chip(message).is_some() {
        return;
    }

    let Some(skill_name) = tool_skill_chip(message) else {
        return;
    };

    line.spans.push(Span::raw(" "));
    line.spans.push(Span::styled(
        format!("[{skill_name}]"),
        theme.accent_primary,
    ));
}

pub(crate) fn first_apply_patch_path(arguments: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let input = value
        .get("input")
        .or_else(|| value.get("patch"))
        .and_then(|raw| raw.as_str())?;
    for line in input.lines() {
        if let Some(path) = line
            .strip_prefix("*** Update File: ")
            .or_else(|| line.strip_prefix("*** Add File: "))
            .or_else(|| line.strip_prefix("*** Delete File: "))
        {
            let path = path.split(" -> ").next().unwrap_or(path).trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderedLineKind {
    MessageBody,
    ImageAttachment,
    ReasoningToggle,
    ReasoningContent,
    ToolToggle,
    ToolDetail,
    ActionBar,
    Padding,
    Streaming,
    RetryStatus,
    RetryAction,
}

#[derive(Debug, Clone)]
pub(crate) struct RenderedChatLine {
    pub(crate) line: Line<'static>,
    pub(crate) message_index: Option<usize>,
    pub(crate) kind: RenderedLineKind,
}

pub(crate) struct SelectionSnapshot {
    pub(crate) key: RenderCacheKey,
    pub(crate) metrics_key: TranscriptMetricsCacheKey,
    pub(crate) inner: Rect,
    pub(crate) all_lines: Vec<RenderedChatLine>,
    pub(crate) total_lines: usize,
    pub(crate) rendered_start_idx: usize,
    pub(crate) message_line_ranges: Vec<(usize, usize)>,
    pub(crate) responder_labels: Vec<Option<String>>,
    pub(crate) start_idx: usize,
    pub(crate) end_idx: usize,
    pub(crate) padding: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatScrollbarLayout {
    pub(crate) content: Rect,
    pub(crate) scrollbar: Rect,
    pub(crate) thumb: Rect,
    pub(crate) scroll: usize,
    pub(crate) max_scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RenderCacheKey {
    pub(crate) inner: Rect,
    pub(crate) render_revision: u64,
    pub(crate) render_epoch: u64,
    pub(crate) retry_wait_start_selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TranscriptMetricsCacheKey {
    pub(crate) inner: Rect,
    pub(crate) transcript_metrics_revision: u64,
    pub(crate) selected_inline_action_message: Option<usize>,
}

pub(crate) fn render_cache_key(
    area: Rect,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> RenderCacheKey {
    let inner = content_inner(area);
    RenderCacheKey {
        inner,
        render_revision: chat.render_revision(),
        render_epoch: chat.render_cache_epoch(current_tick),
        retry_wait_start_selected,
    }
}

pub(crate) fn transcript_metrics_cache_key(
    area: Rect,
    chat: &ChatState,
) -> TranscriptMetricsCacheKey {
    TranscriptMetricsCacheKey {
        inner: content_inner(area),
        transcript_metrics_revision: chat.transcript_metrics_revision(),
        selected_inline_action_message: selected_inline_action_message_index(chat),
    }
}

impl RenderedChatLine {
    pub(crate) fn padding() -> Self {
        Self {
            line: Line::raw(""),
            message_index: None,
            kind: RenderedLineKind::Padding,
        }
    }
}

pub(crate) fn padded_content_width(inner_width: usize) -> usize {
    inner_width.saturating_sub(MESSAGE_PADDING_X * 2).max(1)
}

pub(crate) fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

pub(crate) fn blank_message_line(width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(" ".repeat(width.max(1)), style))
}

pub(crate) fn rendered_line_plain_text(rendered: &RenderedChatLine) -> String {
    rendered
        .line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

pub(crate) fn rendered_line_content_bounds(rendered: &RenderedChatLine) -> (String, usize, usize) {
    let plain = rendered_line_plain_text(rendered);
    let trimmed = plain.trim_end_matches(' ');
    let trimmed_width = UnicodeWidthStr::width(trimmed);
    let content_start = MESSAGE_PADDING_X.min(trimmed_width);
    let content_end = trimmed_width.max(content_start);
    (plain, content_start, content_end)
}

pub(crate) fn pad_message_line(
    mut line: Line<'static>,
    width: usize,
    style: Style,
) -> Line<'static> {
    let mut spans = Vec::new();
    let left = " ".repeat(MESSAGE_PADDING_X);
    spans.push(Span::styled(left, style));

    for span in line.spans.drain(..) {
        spans.push(Span::styled(
            span.content.to_string(),
            style.patch(span.style),
        ));
    }

    let content_width = line_display_width(&Line::from(spans.clone()));
    let right_width = width.saturating_sub(content_width).max(MESSAGE_PADDING_X);
    spans.push(Span::styled(" ".repeat(right_width), style));

    Line::from(spans).style(style.patch(line.style))
}

pub(crate) fn message_block_style(msg: &AgentMessage, theme: &ThemeTokens) -> Style {
    match msg.role {
        MessageRole::User => theme.fg_active.bg(Color::Indexed(236)),
        _ => Style::default(),
    }
}
