use super::super::render_streaming_markdown_to_message_block_style_to_message_action::*;
use super::super::resolved_scroll_to_highlight_line_range_to_selected_text_to_selection::*;
use super::super::selection_point_from_snapshot_to_render::*;
use super::super::*;
use crate::state::chat::{AgentMessage, ChatState, MessageRole, RetryPhase, TranscriptMode};
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
#[cfg(test)]
pub(crate) fn build_rendered_lines(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> (Vec<RenderedChatLine>, Vec<(usize, usize)>) {
    #[cfg(test)]
    BUILD_RENDERED_LINES_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut all_lines = Vec::new();
    let mut message_line_ranges = Vec::new();
    let mode = chat.transcript_mode();
    let expanded = chat.expanded_reasoning();
    let expanded_tools = chat.expanded_tools();
    let content_width = padded_content_width(inner_width);

    if chat.active_thread_older_page_pending() {
        all_lines.push(older_history_loading_line(theme, inner_width, current_tick));
    }

    if let Some(thread) = chat.active_thread() {
        let responder_labels = assistant_responder_labels(thread);
        for (idx, msg) in thread.messages.iter().enumerate() {
            let start = all_lines.len();
            let mut msg_lines = crate::widgets::message::message_to_lines(
                msg,
                idx,
                mode,
                theme,
                content_width,
                &expanded,
                &expanded_tools,
            );
            let rendered_message_line_count = msg_lines.len();
            let mut kinds = classify_message_lines(
                msg,
                idx,
                mode,
                inner_width,
                rendered_message_line_count,
                &expanded,
                &expanded_tools,
            );
            if msg.role == MessageRole::Assistant {
                if let Some(label) = responder_labels.get(idx).and_then(|value| value.as_deref()) {
                    msg_lines.insert(0, responder_label_line(label, theme));
                    kinds.insert(0, RenderedLineKind::MessageBody);
                }
            }
            if let Some(first_line) = msg_lines.first_mut() {
                append_tool_file_chip(first_line, msg, theme);
                append_tool_skill_chip(first_line, msg, theme);
            }

            if kinds.len() < msg_lines.len() {
                kinds.resize(msg_lines.len(), RenderedLineKind::MessageBody);
            } else if kinds.len() > msg_lines.len() {
                kinds.truncate(msg_lines.len());
            }

            let block_style = message_block_style(msg, theme);
            for _ in 0..MESSAGE_PADDING_Y {
                all_lines.push(RenderedChatLine {
                    line: blank_message_line(inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::Padding,
                });
            }

            for (line, kind) in msg_lines.into_iter().zip(kinds.into_iter()) {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(line, inner_width, block_style),
                    message_index: Some(idx),
                    kind,
                });
            }

            let inline_action_line = if selected_message_shows_inline_actions(chat, msg, idx) {
                message_action_line(
                    chat,
                    idx,
                    msg,
                    chat.selected_message_action(),
                    theme,
                    current_tick,
                )
            } else {
                None
            };

            if let Some(action_line) = inline_action_line {
                if selected_message_shows_inline_actions(chat, msg, idx) {
                    all_lines.push(RenderedChatLine {
                        line: blank_message_line(inner_width, block_style),
                        message_index: Some(idx),
                        kind: RenderedLineKind::Padding,
                    });
                }
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(action_line, inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::ActionBar,
                });
                all_lines.push(RenderedChatLine {
                    line: blank_message_line(inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::Padding,
                });
            } else {
                for _ in 0..MESSAGE_PADDING_Y {
                    all_lines.push(RenderedChatLine {
                        line: blank_message_line(inner_width, block_style),
                        message_index: Some(idx),
                        kind: RenderedLineKind::Padding,
                    });
                }
            }
            let end = all_lines.len();
            message_line_ranges.push((start, end));
        }
    }

    let assistant_style = Style::default();
    if !chat.streaming_reasoning().is_empty() {
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                Line::from(vec![Span::styled("\u{25be} Reasoning...", theme.fg_dim)]),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });

        let dark_blue = Style::default().fg(Color::Indexed(24));
        let wrap_width = content_width.saturating_sub(2).max(1);
        for reasoning_line in chat.streaming_reasoning().lines() {
            let wrapped_lines = wrap_text(reasoning_line, wrap_width);
            let wrapped_lines = if wrapped_lines.is_empty() {
                vec![String::new()]
            } else {
                wrapped_lines
            };
            for wrapped in wrapped_lines {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(
                        Line::from(vec![
                            Span::styled("\u{2502}", dark_blue),
                            Span::raw(" "),
                            Span::styled(wrapped, theme.fg_dim),
                        ]),
                        inner_width,
                        assistant_style,
                    ),
                    message_index: None,
                    kind: RenderedLineKind::Streaming,
                });
            }
        }
    }

    if !chat.streaming_content().is_empty() {
        let content = chat.streaming_content();
        if chat.streaming_reasoning().is_empty() {
            all_lines.push(RenderedChatLine {
                line: blank_message_line(inner_width, assistant_style),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
        let wrap_width = content_width;
        let mut wrapped_lines = render_streaming_markdown(content, wrap_width);
        if wrapped_lines.is_empty() {
            wrapped_lines.push(Line::default());
        }

        let last_idx = wrapped_lines.len().saturating_sub(1);
        for (idx, mut md_line) in wrapped_lines.into_iter().enumerate() {
            if idx == last_idx {
                md_line.spans.push(Span::raw("\u{2588}"));
            }
            all_lines.push(RenderedChatLine {
                line: pad_message_line(md_line, inner_width, assistant_style),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
    }

    if let Some(status) = chat.retry_status() {
        let summary = match status.phase {
            RetryPhase::Retrying => format!(
                "[zorai] retry {}/{} in {}s · {}",
                status.attempt,
                if status.max_retries == 0 {
                    "∞".to_string()
                } else {
                    status.max_retries.to_string()
                },
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
            RetryPhase::Waiting => format!(
                "[zorai] retrying automatically in {}s · {}",
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
        };
        let wrapped_summary = wrap_text(&summary, content_width);
        let wrapped_summary = if wrapped_summary.is_empty() {
            vec![String::new()]
        } else {
            wrapped_summary
        };
        let wrapped_message = wrap_text(&status.message, content_width);
        let wrapped_message = if wrapped_message.is_empty() {
            vec![String::new()]
        } else {
            wrapped_message
        };
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::RetryStatus,
        });
        for line in wrapped_summary {
            all_lines.push(RenderedChatLine {
                line: pad_message_line(
                    Line::from(vec![Span::styled(line, theme.fg_dim)]),
                    inner_width,
                    assistant_style,
                ),
                message_index: None,
                kind: RenderedLineKind::RetryStatus,
            });
        }
        for line in wrapped_message {
            all_lines.push(RenderedChatLine {
                line: pad_message_line(
                    Line::from(vec![Span::raw(line)]),
                    inner_width,
                    assistant_style,
                ),
                message_index: None,
                kind: RenderedLineKind::RetryStatus,
            });
        }
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                retry_action_line(status, theme, current_tick, retry_wait_start_selected),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::RetryAction,
        });
    }

    (all_lines, message_line_ranges)
}

pub(crate) struct TranscriptMetrics {
    pub(crate) total_lines: usize,
    pub(crate) message_line_ranges: Vec<(usize, usize)>,
    pub(crate) responder_labels: Vec<Option<String>>,
}

pub(crate) fn build_transcript_metrics(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> TranscriptMetrics {
    #[cfg(test)]
    BUILD_TRANSCRIPT_METRICS_CALLS.with(|calls| calls.set(calls.get() + 1));

    crate::widgets::message::prepare_markdown_render_cache(chat.render_revision());

    let mut total_lines = 0usize;
    let mut message_line_ranges = Vec::new();
    let mode = chat.transcript_mode();
    let expanded = chat.expanded_reasoning();
    let expanded_tools = chat.expanded_tools();
    let content_width = padded_content_width(inner_width);
    total_lines = total_lines.saturating_add(older_history_loading_line_count(chat));

    let mut responder_labels = Vec::new();
    if let Some(thread) = chat.active_thread() {
        responder_labels = assistant_responder_labels(thread);
        let message_count = thread.messages.len();
        let estimate_offscreen_messages = message_count > 20;
        for (idx, msg) in thread.messages.iter().enumerate() {
            let exact_message_metrics = !estimate_offscreen_messages
                || idx + 1 == message_count
                || chat.selected_message() == Some(idx);
            let start = total_lines;
            total_lines = total_lines.saturating_add(estimated_message_block_line_count(
                chat,
                msg,
                idx,
                mode,
                theme,
                content_width,
                &expanded,
                &expanded_tools,
                exact_message_metrics,
                responder_labels
                    .get(idx)
                    .and_then(|value| value.as_deref())
                    .is_some(),
            ));
            message_line_ranges.push((start, total_lines));
        }
    }

    total_lines = total_lines.saturating_add(estimated_streaming_and_retry_line_count(
        chat,
        content_width,
        current_tick,
        retry_wait_start_selected,
    ));

    TranscriptMetrics {
        total_lines,
        message_line_ranges,
        responder_labels,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn estimated_message_block_line_count(
    chat: &ChatState,
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    theme: &ThemeTokens,
    content_width: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
    exact_message_metrics: bool,
    has_responder_label: bool,
) -> usize {
    let message_lines = estimated_message_content_line_count(
        msg,
        msg_index,
        mode,
        theme,
        content_width,
        expanded,
        expanded_tools,
        exact_message_metrics,
    );
    if message_lines == 0 {
        return 0;
    }

    MESSAGE_PADDING_Y
        .saturating_add(usize::from(has_responder_label))
        .saturating_add(message_lines)
        .saturating_add(MESSAGE_PADDING_Y)
        .saturating_add(selected_inline_action_line_count(chat, msg, msg_index))
}

pub(crate) fn selected_message_shows_inline_actions(
    chat: &ChatState,
    msg: &AgentMessage,
    msg_index: usize,
) -> bool {
    chat.selected_message() == Some(msg_index) && !selected_message_is_last_actionable(chat, msg)
}

pub(crate) fn selected_message_is_last_actionable(chat: &ChatState, msg: &AgentMessage) -> bool {
    !msg.actions.is_empty()
        && chat.active_actions().first().map(|action| &action.label)
            == msg.actions.first().map(|action| &action.label)
}

pub(crate) fn selected_inline_action_line_count(
    chat: &ChatState,
    msg: &AgentMessage,
    msg_index: usize,
) -> usize {
    if selected_message_shows_inline_actions(chat, msg, msg_index) {
        2
    } else {
        0
    }
}

pub(crate) fn selected_inline_action_message_index(chat: &ChatState) -> Option<usize> {
    let msg_index = chat.selected_message()?;
    let msg = chat.active_thread()?.messages.get(msg_index)?;
    selected_message_shows_inline_actions(chat, msg, msg_index).then_some(msg_index)
}

pub(crate) fn estimated_message_content_line_count(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    theme: &ThemeTokens,
    content_width: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
    exact_message_metrics: bool,
) -> usize {
    let image_line_count = message_image_preview_path(msg).map_or(0, |_| 12);

    match mode {
        TranscriptMode::Tools => {
            usize::from(msg.role == MessageRole::Tool || msg.tool_name.is_some())
        }
        TranscriptMode::Compact | TranscriptMode::Full => {
            let tools_expanded =
                matches!(mode, TranscriptMode::Full) || expanded_tools.contains(&msg_index);
            let reasoning_expanded =
                matches!(mode, TranscriptMode::Full) || expanded.contains(&msg_index);

            if msg.role == MessageRole::Tool {
                if msg.tool_name.is_none() {
                    return 0;
                }
                if tools_expanded {
                    return crate::widgets::message::message_to_lines(
                        msg,
                        msg_index,
                        mode,
                        theme,
                        content_width,
                        expanded,
                        expanded_tools,
                    )
                    .len();
                }
                return 1;
            }

            if msg.is_operator_question {
                return crate::widgets::message::message_to_lines(
                    msg,
                    msg_index,
                    mode,
                    theme,
                    content_width,
                    expanded,
                    expanded_tools,
                )
                .len();
            }

            if crate::widgets::message::is_collapsible_system_notice_message(msg) {
                if exact_message_metrics {
                    return crate::widgets::message::message_to_lines(
                        msg,
                        msg_index,
                        mode,
                        theme,
                        content_width,
                        expanded,
                        expanded_tools,
                    )
                    .len();
                }
                return crate::widgets::message::estimated_collapsible_system_notice_line_count(
                    msg,
                    msg_index,
                    mode,
                    content_width,
                    expanded,
                );
            }

            if msg.content.is_empty() && image_line_count == 0 && msg.role != MessageRole::Assistant
            {
                return 0;
            }
            if msg.content.is_empty() && image_line_count == 0 && msg.reasoning.is_none() {
                return 0;
            }

            let has_reasoning = msg.role == MessageRole::Assistant
                && msg
                    .reasoning
                    .as_deref()
                    .is_some_and(|reasoning| !reasoning.is_empty());
            let mut count = 0usize;
            if has_reasoning {
                count = count.saturating_add(1);
                if reasoning_expanded {
                    let reasoning_width = content_width.saturating_sub(2).max(1);
                    count = count.saturating_add(
                        wrap_text(
                            msg.reasoning.as_deref().unwrap_or_default(),
                            reasoning_width,
                        )
                        .len()
                        .max(1),
                    );
                }
            }

            let content_lines = if msg.content.is_empty() {
                0
            } else if (msg.role == MessageRole::Assistant || msg.role == MessageRole::System)
                && exact_message_metrics
            {
                crate::widgets::message::render_markdown_pub(&msg.content, content_width).len()
            } else {
                wrap_text(&msg.content, content_width).len()
            };
            count
                .saturating_add(content_lines.max(usize::from(!msg.content.is_empty())))
                .saturating_add(image_line_count)
                .max(usize::from(msg.role == MessageRole::Assistant))
        }
    }
}

pub(crate) fn estimated_streaming_and_retry_line_count(
    chat: &ChatState,
    content_width: usize,
    _current_tick: u64,
    _retry_wait_start_selected: bool,
) -> usize {
    let mut count = 0usize;

    if !chat.streaming_reasoning().is_empty() {
        count = count.saturating_add(2);
        let wrap_width = content_width.saturating_sub(2).max(1);
        for reasoning_line in chat.streaming_reasoning().lines() {
            count = count.saturating_add(wrap_text(reasoning_line, wrap_width).len().max(1));
        }
    }

    if !chat.streaming_content().is_empty() {
        if chat.streaming_reasoning().is_empty() {
            count = count.saturating_add(1);
        }
        count = count.saturating_add(
            render_streaming_markdown(chat.streaming_content(), content_width)
                .len()
                .max(1),
        );
    }

    if let Some(status) = chat.retry_status() {
        let summary = match status.phase {
            RetryPhase::Retrying => format!(
                "[zorai] retry {}/{} in {}s · {}",
                status.attempt,
                if status.max_retries == 0 {
                    "∞".to_string()
                } else {
                    status.max_retries.to_string()
                },
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
            RetryPhase::Waiting => format!(
                "[zorai] retrying automatically in {}s · {}",
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
        };
        count = count
            .saturating_add(1)
            .saturating_add(wrap_text(&summary, content_width).len().max(1))
            .saturating_add(wrap_text(&status.message, content_width).len().max(1))
            .saturating_add(1);
    }

    count
}

pub(crate) fn build_rendered_line_window(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
    window_start: usize,
    window_end: usize,
    metrics: &TranscriptMetrics,
) -> Vec<RenderedChatLine> {
    crate::widgets::message::prepare_markdown_render_cache(chat.render_revision());

    let mut lines = vec![RenderedChatLine::padding(); window_end.saturating_sub(window_start)];
    let mode = chat.transcript_mode();
    let expanded = chat.expanded_reasoning();
    let expanded_tools = chat.expanded_tools();
    let content_width = padded_content_width(inner_width);
    let loading_line_count = older_history_loading_line_count(chat);

    if loading_line_count > 0 && window_start < loading_line_count && window_end > 0 {
        let loading = [older_history_loading_line(theme, inner_width, current_tick)];
        overlay_intersecting_lines(
            &mut lines,
            &loading,
            0,
            loading_line_count,
            window_start,
            window_end,
        );
    }

    if let Some(thread) = chat.active_thread() {
        let visible_messages =
            intersecting_message_range(&metrics.message_line_ranges, window_start, window_end);
        for idx in visible_messages {
            let Some(msg) = thread.messages.get(idx) else {
                continue;
            };
            let Some(&(block_start, block_end)) = metrics.message_line_ranges.get(idx) else {
                continue;
            };

            let block = render_message_block_lines(
                chat,
                msg,
                idx,
                mode,
                theme,
                inner_width,
                content_width,
                &expanded,
                &expanded_tools,
                metrics
                    .responder_labels
                    .get(idx)
                    .and_then(|value| value.as_deref()),
                current_tick,
            );
            overlay_intersecting_lines(
                &mut lines,
                &block,
                block_start,
                block_end,
                window_start,
                window_end,
            );
        }
    }

    let message_total = metrics
        .message_line_ranges
        .last()
        .map(|(_, end)| *end)
        .unwrap_or(loading_line_count);
    if window_end > message_total && metrics.total_lines > message_total {
        let tail = render_streaming_retry_lines(
            chat,
            theme,
            inner_width,
            content_width,
            current_tick,
            retry_wait_start_selected,
        );
        overlay_intersecting_lines(
            &mut lines,
            &tail,
            message_total,
            metrics.total_lines,
            window_start,
            window_end,
        );
    }

    lines
}

pub(crate) fn intersecting_message_range(
    message_line_ranges: &[(usize, usize)],
    window_start: usize,
    window_end: usize,
) -> std::ops::Range<usize> {
    if window_start >= window_end {
        return 0..0;
    }

    let start = message_line_ranges.partition_point(|(_, end)| *end <= window_start);
    let end = message_line_ranges.partition_point(|(start, _)| *start < window_end);
    start..end.max(start)
}

pub(crate) fn older_history_loading_line_count(chat: &ChatState) -> usize {
    usize::from(chat.active_thread_older_page_pending())
}

pub(crate) fn older_history_loading_line(
    theme: &ThemeTokens,
    inner_width: usize,
    current_tick: u64,
) -> RenderedChatLine {
    let frames = ["|", "/", "-", "\\"];
    let spinner = frames[((current_tick / 2) as usize) % frames.len()];
    RenderedChatLine {
        line: pad_message_line(
            Line::from(vec![
                Span::styled(spinner.to_string(), theme.accent_secondary),
                Span::raw(" "),
                Span::styled("Loading previous messages", theme.fg_dim),
            ]),
            inner_width,
            Style::default(),
        ),
        message_index: None,
        kind: RenderedLineKind::Streaming,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_message_block_lines(
    chat: &ChatState,
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    theme: &ThemeTokens,
    inner_width: usize,
    content_width: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
    responder_label: Option<&str>,
    current_tick: u64,
) -> Vec<RenderedChatLine> {
    let mut block = Vec::new();
    let mut msg_lines = crate::widgets::message::message_to_lines(
        msg,
        msg_index,
        mode,
        theme,
        content_width,
        expanded,
        expanded_tools,
    );
    let rendered_message_line_count = msg_lines.len();
    let mut kinds = classify_message_lines(
        msg,
        msg_index,
        mode,
        inner_width,
        rendered_message_line_count,
        expanded,
        expanded_tools,
    );
    if msg.role == MessageRole::Assistant {
        if let Some(label) = responder_label {
            msg_lines.insert(0, responder_label_line(label, theme));
            kinds.insert(0, RenderedLineKind::MessageBody);
        }
    }
    if let Some(first_line) = msg_lines.first_mut() {
        append_tool_file_chip(first_line, msg, theme);
        append_tool_skill_chip(first_line, msg, theme);
    }

    if kinds.len() < msg_lines.len() {
        kinds.resize(msg_lines.len(), RenderedLineKind::MessageBody);
    } else if kinds.len() > msg_lines.len() {
        kinds.truncate(msg_lines.len());
    }

    let block_style = message_block_style(msg, theme);
    for _ in 0..MESSAGE_PADDING_Y {
        block.push(RenderedChatLine {
            line: blank_message_line(inner_width, block_style),
            message_index: Some(msg_index),
            kind: RenderedLineKind::Padding,
        });
    }

    for (line, kind) in msg_lines.into_iter().zip(kinds.into_iter()) {
        block.push(RenderedChatLine {
            line: pad_message_line(line, inner_width, block_style),
            message_index: Some(msg_index),
            kind,
        });
    }

    let inline_action_line = if selected_message_shows_inline_actions(chat, msg, msg_index) {
        message_action_line(
            chat,
            msg_index,
            msg,
            chat.selected_message_action(),
            theme,
            current_tick,
        )
    } else {
        None
    };

    if let Some(action_line) = inline_action_line {
        if selected_message_shows_inline_actions(chat, msg, msg_index) {
            block.push(RenderedChatLine {
                line: blank_message_line(inner_width, block_style),
                message_index: Some(msg_index),
                kind: RenderedLineKind::Padding,
            });
        }
        block.push(RenderedChatLine {
            line: pad_message_line(action_line, inner_width, block_style),
            message_index: Some(msg_index),
            kind: RenderedLineKind::ActionBar,
        });
        block.push(RenderedChatLine {
            line: blank_message_line(inner_width, block_style),
            message_index: Some(msg_index),
            kind: RenderedLineKind::Padding,
        });
    } else {
        for _ in 0..MESSAGE_PADDING_Y {
            block.push(RenderedChatLine {
                line: blank_message_line(inner_width, block_style),
                message_index: Some(msg_index),
                kind: RenderedLineKind::Padding,
            });
        }
    }

    block
}

pub(crate) fn overlay_intersecting_lines(
    output: &mut [RenderedChatLine],
    block: &[RenderedChatLine],
    block_start: usize,
    block_end: usize,
    window_start: usize,
    window_end: usize,
) {
    if block.is_empty() || block_start >= block_end {
        return;
    }

    let start = window_start.max(block_start);
    let end = window_end.min(block_end);
    if start >= end {
        return;
    }

    let trailing_padding = block
        .iter()
        .rev()
        .find(|line| matches!(line.kind, RenderedLineKind::Padding))
        .cloned()
        .unwrap_or_else(RenderedChatLine::padding);
    for absolute_row in start..end {
        let local = absolute_row.saturating_sub(block_start);
        if let Some(slot) = output.get_mut(absolute_row.saturating_sub(window_start)) {
            *slot = block
                .get(local)
                .cloned()
                .unwrap_or_else(|| trailing_padding.clone());
        }
    }
}

pub(crate) fn render_streaming_retry_lines(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
    content_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Vec<RenderedChatLine> {
    let mut all_lines = Vec::new();
    let assistant_style = Style::default();

    if !chat.streaming_reasoning().is_empty() {
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                Line::from(vec![Span::styled("\u{25be} Reasoning...", theme.fg_dim)]),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });

        let dark_blue = Style::default().fg(Color::Indexed(24));
        let wrap_width = content_width.saturating_sub(2).max(1);
        for reasoning_line in chat.streaming_reasoning().lines() {
            let wrapped_lines = wrap_text(reasoning_line, wrap_width);
            let wrapped_lines = if wrapped_lines.is_empty() {
                vec![String::new()]
            } else {
                wrapped_lines
            };
            for wrapped in wrapped_lines {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(
                        Line::from(vec![
                            Span::styled("\u{2502}", dark_blue),
                            Span::raw(" "),
                            Span::styled(wrapped, theme.fg_dim),
                        ]),
                        inner_width,
                        assistant_style,
                    ),
                    message_index: None,
                    kind: RenderedLineKind::Streaming,
                });
            }
        }
    }

    if !chat.streaming_content().is_empty() {
        let content = chat.streaming_content();
        if chat.streaming_reasoning().is_empty() {
            all_lines.push(RenderedChatLine {
                line: blank_message_line(inner_width, assistant_style),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
        let mut md_lines = render_streaming_markdown(content, content_width);
        if md_lines.is_empty() {
            md_lines.push(Line::default());
        }
        let last_idx = md_lines.len().saturating_sub(1);
        for (idx, mut md_line) in md_lines.into_iter().enumerate() {
            if idx == last_idx {
                md_line.spans.push(Span::raw("\u{2588}"));
            }
            all_lines.push(RenderedChatLine {
                line: pad_message_line(md_line, inner_width, assistant_style),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
    }

    if let Some(status) = chat.retry_status() {
        let summary = match status.phase {
            RetryPhase::Retrying => format!(
                "[zorai] retry {}/{} in {}s · {}",
                status.attempt,
                if status.max_retries == 0 {
                    "∞".to_string()
                } else {
                    status.max_retries.to_string()
                },
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
            RetryPhase::Waiting => format!(
                "[zorai] retrying automatically in {}s · {}",
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
        };
        let wrapped_summary = wrap_text(&summary, content_width);
        let wrapped_summary = if wrapped_summary.is_empty() {
            vec![String::new()]
        } else {
            wrapped_summary
        };
        let wrapped_message = wrap_text(&status.message, content_width);
        let wrapped_message = if wrapped_message.is_empty() {
            vec![String::new()]
        } else {
            wrapped_message
        };
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::RetryStatus,
        });
        for line in wrapped_summary {
            all_lines.push(RenderedChatLine {
                line: pad_message_line(
                    Line::from(vec![Span::styled(line, theme.fg_dim)]),
                    inner_width,
                    assistant_style,
                ),
                message_index: None,
                kind: RenderedLineKind::RetryStatus,
            });
        }
        for line in wrapped_message {
            all_lines.push(RenderedChatLine {
                line: pad_message_line(
                    Line::from(vec![Span::raw(line)]),
                    inner_width,
                    assistant_style,
                ),
                message_index: None,
                kind: RenderedLineKind::RetryStatus,
            });
        }
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                retry_action_line(status, theme, current_tick, retry_wait_start_selected),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::RetryAction,
        });
    }

    all_lines
}

pub(crate) const THREAD_HANDOFF_SYSTEM_MARKER: &str = "[[handoff_event]]";

#[derive(serde::Deserialize)]
pub(crate) struct HandoffResponderEvent {
    #[serde(default)]
    pub(crate) from_agent_name: Option<String>,
    #[serde(default)]
    pub(crate) to_agent_name: Option<String>,
}

pub(crate) fn assistant_responder_labels(
    thread: &crate::state::chat::AgentThread,
) -> Vec<Option<String>> {
    #[cfg(test)]
    ASSISTANT_RESPONDER_LABELS_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut labels = vec![None; thread.messages.len()];
    let mut responder = initial_responder_name(thread);
    let participant_ids = thread
        .thread_participants
        .iter()
        .map(|participant| participant.agent_id.trim().to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();

    for (idx, msg) in thread.messages.iter().enumerate() {
        if msg.role == MessageRole::Assistant {
            labels[idx] =
                message_responder_label(msg, &participant_ids).or_else(|| responder.clone());
        }
        if let Some(event) = handoff_responder_event_for_message(msg) {
            if event.to_agent_name.is_some() {
                responder = event
                    .to_agent_name
                    .map(|name| crate::agent_aliases::display_agent_name(&name));
            }
        }
    }

    labels
}

pub(crate) fn responder_label_line(label: &str, theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::styled("● ", responder_accent_style(label, theme)),
        Span::styled("Responder: ", theme.fg_dim),
        Span::styled(label.to_string(), responder_accent_style(label, theme)),
    ])
}

pub(crate) fn responder_accent_style(label: &str, theme: &ThemeTokens) -> Style {
    if !label.starts_with('@') {
        return theme.accent_assistant;
    }

    let palette = [
        theme.accent_primary,
        theme.accent_secondary,
        theme.accent_danger,
        Style::default().fg(Color::Indexed(111)),
        Style::default().fg(Color::Indexed(180)),
    ];
    let hash = label.bytes().fold(0u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    });
    palette[(hash as usize) % palette.len()]
}

pub(crate) fn message_responder_label(
    msg: &AgentMessage,
    participant_ids: &std::collections::HashSet<String>,
) -> Option<String> {
    let author_name = msg
        .author_agent_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(crate::agent_aliases::display_agent_name)?;
    let author_id = msg
        .author_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    if author_id
        .as_deref()
        .is_some_and(|author_id| participant_ids.contains(author_id))
    {
        Some(format!("@{author_name}"))
    } else {
        Some(author_name.to_string())
    }
}

pub(crate) fn initial_responder_name(thread: &crate::state::chat::AgentThread) -> Option<String> {
    thread
        .agent_name
        .clone()
        .or_else(|| {
            thread.messages.iter().find_map(|msg| {
                handoff_responder_event_for_message(msg).and_then(|event| event.from_agent_name)
            })
        })
        .or_else(|| Some(zorai_protocol::AGENT_NAME_SWAROG.to_string()))
        .map(|name| crate::agent_aliases::display_agent_name(&name))
}

pub(crate) fn handoff_responder_event_for_message(
    msg: &AgentMessage,
) -> Option<HandoffResponderEvent> {
    if msg.role != MessageRole::System {
        return None;
    }
    parse_handoff_responder_event(&msg.content)
}

pub(crate) fn parse_handoff_responder_event(content: &str) -> Option<HandoffResponderEvent> {
    let payload = content.strip_prefix(THREAD_HANDOFF_SYSTEM_MARKER)?;
    let json = payload.lines().next()?.trim();
    serde_json::from_str(json).ok()
}

pub struct CachedSelectionSnapshot(pub(crate) SelectionSnapshot);

pub fn build_selection_snapshot(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<CachedSelectionSnapshot> {
    selection_snapshot(area, chat, theme, current_tick, retry_wait_start_selected)
        .map(CachedSelectionSnapshot)
}

pub fn build_selection_snapshot_covering(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<CachedSelectionSnapshot> {
    let cover_start = start.row.min(end.row);
    let cover_end = start.row.max(end.row).saturating_add(1);
    selection_snapshot_windowed(
        area,
        chat,
        theme,
        current_tick,
        retry_wait_start_selected,
        Some((cover_start, cover_end)),
    )
    .map(CachedSelectionSnapshot)
}

pub fn cached_snapshot_matches_area(snapshot: &CachedSelectionSnapshot, area: Rect) -> bool {
    snapshot.0.inner == content_inner(area)
}

pub fn cached_snapshot_matches_render(
    snapshot: &CachedSelectionSnapshot,
    area: Rect,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> bool {
    snapshot.0.key == render_cache_key(area, chat, current_tick, retry_wait_start_selected)
        && cached_snapshot_covers_window(snapshot, chat)
}

pub fn cached_snapshot_matches_render_key(
    snapshot: &CachedSelectionSnapshot,
    area: Rect,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> bool {
    let _ = (current_tick, retry_wait_start_selected);
    snapshot.0.metrics_key == transcript_metrics_cache_key(area, chat)
}

pub fn cached_snapshot_covers_window(snapshot: &CachedSelectionSnapshot, chat: &ChatState) -> bool {
    snapshot_covers_visible_window(&snapshot.0, chat)
}

pub(crate) fn snapshot_covers_visible_window(
    snapshot: &SelectionSnapshot,
    chat: &ChatState,
) -> bool {
    let scroll = resolved_scroll(
        chat,
        snapshot.total_lines,
        snapshot.inner.height as usize,
        &snapshot.message_line_ranges,
    );
    let (_, start_idx, end_idx) =
        visible_window_bounds(snapshot.total_lines, snapshot.inner.height as usize, scroll);

    start_idx >= snapshot.rendered_start_idx && end_idx <= snapshot_rendered_end_idx(snapshot)
}

pub(crate) fn cached_transcript_metrics(
    snapshot: &SelectionSnapshot,
    chat: &ChatState,
    content_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> TranscriptMetrics {
    let message_total = snapshot
        .message_line_ranges
        .last()
        .map(|(_, end)| *end)
        .unwrap_or_else(|| older_history_loading_line_count(chat));
    let total_lines = message_total.saturating_add(estimated_streaming_and_retry_line_count(
        chat,
        content_width,
        current_tick,
        retry_wait_start_selected,
    ));

    TranscriptMetrics {
        total_lines,
        message_line_ranges: snapshot.message_line_ranges.clone(),
        responder_labels: snapshot.responder_labels.clone(),
    }
}

pub fn refresh_cached_snapshot_window(
    snapshot: &CachedSelectionSnapshot,
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<CachedSelectionSnapshot> {
    let key = render_cache_key(area, chat, current_tick, retry_wait_start_selected);
    let metrics_key = transcript_metrics_cache_key(area, chat);
    if snapshot.0.metrics_key != metrics_key {
        return None;
    }

    let inner = content_inner(area);
    if inner.width == 0 || inner.height == 0 || inner != snapshot.0.inner {
        return None;
    }

    let metrics = cached_transcript_metrics(
        &snapshot.0,
        chat,
        padded_content_width(inner.width as usize),
        current_tick,
        retry_wait_start_selected,
    );
    let scroll = resolved_scroll(
        chat,
        metrics.total_lines,
        inner.height as usize,
        &metrics.message_line_ranges,
    );
    let (padding, start_idx, end_idx) =
        visible_window_bounds(metrics.total_lines, inner.height as usize, scroll);
    let overscan = (inner.height as usize).div_ceil(10).max(1);
    let rendered_start_idx = start_idx.saturating_sub(overscan);
    let rendered_end_idx = end_idx.saturating_add(overscan).min(metrics.total_lines);
    let all_lines = build_rendered_line_window(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
        rendered_start_idx,
        rendered_end_idx,
        &metrics,
    );

    Some(CachedSelectionSnapshot(SelectionSnapshot {
        key,
        metrics_key,
        inner,
        all_lines,
        total_lines: metrics.total_lines,
        rendered_start_idx,
        message_line_ranges: metrics.message_line_ranges,
        responder_labels: metrics.responder_labels,
        start_idx,
        end_idx,
        padding,
    }))
}

pub fn selection_point_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    mouse: Position,
) -> Option<SelectionPoint> {
    selection_point_from_snapshot(&snapshot.0, mouse)
}

pub fn selected_text_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    if snapshot.0.total_lines == 0 {
        return None;
    }

    let (start_point, end_point) =
        if start.row < end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    let start_row = start_point
        .row
        .min(snapshot.0.total_lines.saturating_sub(1));
    let end_row = end_point.row.min(snapshot.0.total_lines.saturating_sub(1));
    let start_col = start_point.col;
    let end_col = end_point.col;

    if start_row == end_row && start_col == end_col {
        return None;
    }

    let mut lines = Vec::new();

    for row in start_row..=end_row {
        let rendered = snapshot_line_at(&snapshot.0, row)?;
        let (plain, content_start, content_end) = rendered_line_content_bounds(rendered);
        let content_width = content_end.saturating_sub(content_start);
        let from = if row == start_row {
            start_col.min(content_width)
        } else {
            0
        };
        let to = if row == end_row {
            end_col.min(content_width).max(from)
        } else {
            content_width
        };

        lines.push(display_slice(
            &plain,
            content_start.saturating_add(from),
            content_start.saturating_add(to),
        ));
    }

    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub(crate) fn apply_selected_message_highlight(
    all_lines: &mut [RenderedChatLine],
    selected_msg: Option<usize>,
) {
    let Some(sel_idx) = selected_msg else {
        return;
    };
    let sel_style = Style::default().bg(Color::Indexed(238));
    for rendered in all_lines
        .iter_mut()
        .filter(|line| line.message_index == Some(sel_idx))
    {
        rendered.line.style = rendered.line.style.patch(sel_style);
        for span in &mut rendered.line.spans {
            span.style = span.style.patch(sel_style);
        }
    }
}

pub(crate) fn build_visible_window_from_snapshot(
    snapshot: &SelectionSnapshot,
    chat: &ChatState,
) -> (Vec<RenderedChatLine>, usize, usize, usize) {
    let scroll = resolved_scroll(
        chat,
        snapshot.total_lines,
        snapshot.inner.height as usize,
        &snapshot.message_line_ranges,
    );
    let (padding, start_idx, end_idx) =
        visible_window_bounds(snapshot.total_lines, snapshot.inner.height as usize, scroll);

    let mut visible = Vec::with_capacity(snapshot.inner.height as usize);
    for _ in 0..padding {
        visible.push(RenderedChatLine::padding());
    }
    for row in start_idx..end_idx {
        if let Some(line) = snapshot_line_at(snapshot, row) {
            visible.push(line.clone());
        }
    }
    (visible, padding, start_idx, end_idx)
}
