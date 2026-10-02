use super::render_streaming_markdown_to_message_block_style::*;
use crate::state::chat::{AgentMessage, ChatHitTarget, ChatState, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;
pub(crate) fn message_action_targets(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    current_tick: u64,
) -> Vec<(String, ChatHitTarget)> {
    if !msg.actions.is_empty() {
        return msg
            .actions
            .iter()
            .enumerate()
            .map(|(action_index, action)| {
                (
                    format!("[{}]", action.label),
                    ChatHitTarget::MessageAction {
                        message_index: msg_index,
                        action_index,
                    },
                )
            })
            .collect();
    }

    let copy_label = if chat.is_message_recently_copied(msg_index, current_tick) {
        "[Copied]".to_string()
    } else {
        "[Copy]".to_string()
    };
    let mut actions = Vec::new();

    if ((msg.role == MessageRole::Assistant
        && msg
            .reasoning
            .as_deref()
            .is_some_and(|reasoning| !reasoning.is_empty()))
        || crate::widgets::message::is_collapsible_system_notice_message(msg))
        && matches!(chat.transcript_mode(), TranscriptMode::Compact)
    {
        let toggle_label = if chat.expanded_reasoning().contains(&msg_index) {
            "[Collapse]"
        } else {
            "[Expand]"
        };
        actions.push((
            toggle_label.to_string(),
            ChatHitTarget::ReasoningToggle(msg_index),
        ));
    }

    if msg.role == MessageRole::Tool
        && msg.tool_name.is_some()
        && matches!(chat.transcript_mode(), TranscriptMode::Compact)
    {
        let toggle_label = if chat.expanded_tools().contains(&msg_index) {
            "[Collapse]"
        } else {
            "[Expand]"
        };
        actions.push((
            toggle_label.to_string(),
            ChatHitTarget::ToolToggle(msg_index),
        ));
    }

    actions.push((copy_label, ChatHitTarget::CopyMessage(msg_index)));
    let has_persistent_id = msg.id.as_deref().is_some_and(|id| !id.trim().is_empty());
    if !has_persistent_id {
        return actions;
    }

    let fork_label = if chat.is_message_recently_forking(msg_index, current_tick) {
        "[Forking]".to_string()
    } else {
        "[Fork]".to_string()
    };
    actions.push((fork_label, ChatHitTarget::ForkMessage(msg_index)));
    match msg.role {
        MessageRole::User => {
            actions.push((
                "[Resend]".to_string(),
                ChatHitTarget::ResendMessage(msg_index),
            ));
            actions.push((
                if msg.pinned_for_compaction {
                    "[Unpin]"
                } else {
                    "[Pin]"
                }
                .to_string(),
                if msg.pinned_for_compaction {
                    ChatHitTarget::UnpinMessage(msg_index)
                } else {
                    ChatHitTarget::PinMessage(msg_index)
                },
            ));
        }
        MessageRole::Assistant => {
            actions.push((
                "[Regenerate]".to_string(),
                ChatHitTarget::RegenerateMessage(msg_index),
            ));
            actions.push((
                if msg.pinned_for_compaction {
                    "[Unpin]"
                } else {
                    "[Pin]"
                }
                .to_string(),
                if msg.pinned_for_compaction {
                    ChatHitTarget::UnpinMessage(msg_index)
                } else {
                    ChatHitTarget::PinMessage(msg_index)
                },
            ));
        }
        _ => {}
    }
    if matches!(msg.role, MessageRole::Assistant | MessageRole::Tool) {
        let up_label = if matches!(msg.feedback, Some(zorai_protocol::Reaction::Up)) {
            "[👍✓]"
        } else {
            "[👍]"
        };
        let down_label = if matches!(msg.feedback, Some(zorai_protocol::Reaction::Down)) {
            "[👎✓]"
        } else {
            "[👎]"
        };
        actions.push((up_label.to_string(), ChatHitTarget::ThumbsUp(msg_index)));
        actions.push((down_label.to_string(), ChatHitTarget::ThumbsDown(msg_index)));
    }
    actions.push((
        "[Delete]".to_string(),
        ChatHitTarget::DeleteMessage(msg_index),
    ));
    actions.push((
        "[Export]".to_string(),
        ChatHitTarget::ExportMessage(msg_index),
    ));
    actions
}

pub(crate) fn message_action_line(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    selected_action: usize,
    theme: &ThemeTokens,
    current_tick: u64,
) -> Option<Line<'static>> {
    let actions = message_action_targets(chat, msg_index, msg, current_tick);
    if actions.is_empty() {
        return None;
    }

    let mut spans = Vec::new();
    for (idx, (label, _)) in actions.into_iter().enumerate() {
        let style = if idx == selected_action {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    Some(Line::from(spans))
}

pub(crate) fn action_hit_target(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    content_col: usize,
    current_tick: u64,
) -> Option<ChatHitTarget> {
    let actions = message_action_targets(chat, msg_index, msg, current_tick);
    let mut col = 0usize;
    for (label, target) in actions {
        let width = UnicodeWidthStr::width(label.as_str()).saturating_add(2);
        if content_col >= col && content_col < col.saturating_add(width) {
            return Some(target);
        }
        col = col.saturating_add(width);
    }
    None
}

pub(crate) fn retry_wait_remaining_secs(
    status: &crate::state::chat::RetryStatusVm,
    current_tick: u64,
) -> u64 {
    let elapsed_ticks = current_tick.saturating_sub(status.received_at_tick);
    let elapsed_ms = elapsed_ticks.saturating_mul(crate::app::TUI_TICK_RATE_MS);
    status
        .delay_ms
        .saturating_sub(elapsed_ms)
        .div_ceil(1_000)
        .max(1)
}

pub(crate) fn retry_action_line(
    status: &crate::state::chat::RetryStatusVm,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Line<'static> {
    if crate::state::chat::retry_status_shows_prompt_actions(status) {
        let yes_label = format!("[Yes {}s]", retry_wait_remaining_secs(status, current_tick));
        let yes_style = if retry_wait_start_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let no_style = if retry_wait_start_selected {
            theme.fg_dim
        } else {
            theme.accent_primary
        };
        return Line::from(vec![
            Span::styled(yes_label, yes_style),
            Span::raw(" "),
            Span::styled("[No]", no_style),
        ]);
    }

    Line::from(vec![Span::styled("[Stop]", theme.accent_primary)])
}

pub(crate) fn classify_message_lines(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    width: usize,
    rendered_message_line_count: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
) -> Vec<RenderedLineKind> {
    let content_width = padded_content_width(width);
    let image_line_count = message_image_preview_path(msg)
        .map(|path| {
            crate::widgets::image_preview::render_image_preview_lines(
                &path,
                content_width,
                12,
                &ThemeTokens::default(),
            )
            .len()
        })
        .unwrap_or(0);

    match mode {
        TranscriptMode::Tools => {
            if msg.role != MessageRole::Tool && msg.tool_name.is_none() {
                return Vec::new();
            }
            vec![RenderedLineKind::MessageBody]
        }
        TranscriptMode::Compact | TranscriptMode::Full => {
            let tool_toggle_kind = if matches!(mode, TranscriptMode::Compact) {
                RenderedLineKind::ToolToggle
            } else {
                RenderedLineKind::MessageBody
            };
            let reasoning_toggle_kind = if matches!(mode, TranscriptMode::Compact) {
                RenderedLineKind::ReasoningToggle
            } else {
                RenderedLineKind::MessageBody
            };
            let tools_expanded =
                matches!(mode, TranscriptMode::Full) || expanded_tools.contains(&msg_index);
            let reasoning_expanded =
                matches!(mode, TranscriptMode::Full) || expanded.contains(&msg_index);

            if msg.role == MessageRole::Tool {
                if msg.tool_name.is_none() {
                    return Vec::new();
                }

                let mut kinds = vec![tool_toggle_kind];

                if tools_expanded {
                    if msg
                        .tool_arguments
                        .as_deref()
                        .is_some_and(|args| !args.is_empty())
                    {
                        kinds.push(RenderedLineKind::ToolDetail);
                    }

                    if !msg.content.is_empty() {
                        let result_lines = msg.content.lines().count().min(5);
                        kinds.extend(std::iter::repeat_n(
                            RenderedLineKind::ToolDetail,
                            result_lines,
                        ));
                        if msg.content.lines().count() > 5 {
                            kinds.push(RenderedLineKind::ToolDetail);
                        }
                    }
                }

                return kinds;
            }

            if crate::widgets::message::is_collapsible_system_notice_message(msg) {
                let mut kinds = vec![reasoning_toggle_kind];
                if msg.message_kind == "compaction_artifact" {
                    let detail_width = content_width.saturating_sub(2).max(1);
                    let visible_line_count =
                        wrap_text(msg.content.trim(), detail_width).len().max(1);
                    kinds.extend(std::iter::repeat_n(
                        RenderedLineKind::MessageBody,
                        visible_line_count,
                    ));
                    if reasoning_expanded {
                        let detail_line_count =
                            rendered_message_line_count.saturating_sub(1 + visible_line_count);
                        kinds.extend(std::iter::repeat_n(
                            RenderedLineKind::ReasoningContent,
                            detail_line_count.max(1),
                        ));
                    }
                } else if reasoning_expanded {
                    let detail_line_count = rendered_message_line_count.saturating_sub(1);
                    kinds.extend(std::iter::repeat_n(
                        RenderedLineKind::ReasoningContent,
                        detail_line_count.max(1),
                    ));
                }
                return kinds;
            }

            if msg.content.is_empty() && image_line_count == 0 && msg.role != MessageRole::Assistant
            {
                return Vec::new();
            }
            if msg.content.is_empty() && image_line_count == 0 && msg.reasoning.is_none() {
                return Vec::new();
            }

            let has_reasoning = msg.role == MessageRole::Assistant
                && msg
                    .reasoning
                    .as_deref()
                    .is_some_and(|reasoning| !reasoning.is_empty());

            let content_lines = if msg.content.is_empty() {
                0
            } else if msg.role == MessageRole::Assistant {
                let reasoning_lines = if has_reasoning {
                    let reasoning_expanded =
                        matches!(mode, TranscriptMode::Full) || expanded.contains(&msg_index);
                    if reasoning_expanded {
                        let reasoning_width = content_width.saturating_sub(2).max(1);
                        1 + wrap_text(
                            msg.reasoning.as_deref().unwrap_or_default(),
                            reasoning_width,
                        )
                        .len()
                        .max(1)
                    } else {
                        1
                    }
                } else {
                    0
                };
                rendered_message_line_count
                    .saturating_sub(reasoning_lines)
                    .saturating_sub(image_line_count)
            } else if msg.role == MessageRole::System {
                crate::widgets::message::render_markdown_pub(&msg.content, content_width).len()
            } else {
                wrap_text(&msg.content, content_width).len()
            };

            if has_reasoning {
                let mut kinds = vec![reasoning_toggle_kind];

                if reasoning_expanded {
                    let reasoning_width = content_width.saturating_sub(2).max(1);
                    let reasoning_line_count = wrap_text(
                        msg.reasoning.as_deref().unwrap_or_default(),
                        reasoning_width,
                    )
                    .len();
                    kinds.extend(std::iter::repeat_n(
                        RenderedLineKind::ReasoningContent,
                        reasoning_line_count.max(1),
                    ));
                }

                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::MessageBody,
                    content_lines,
                ));
                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::ImageAttachment,
                    image_line_count,
                ));
                kinds
            } else {
                let mut kinds = vec![RenderedLineKind::MessageBody; content_lines.max(1)];
                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::ImageAttachment,
                    image_line_count,
                ));
                kinds
            }
        }
    }
}
