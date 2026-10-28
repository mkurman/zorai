use super::super::build_rendered_lines_to_build_visible_window_from_snapshot_to_apply::*;
use super::super::render_streaming_markdown_to_message_block_style_to_message_action::*;
use super::super::*;
use crate::state::chat::ChatState;
use crate::theme::ThemeTokens;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;
pub(crate) fn resolved_scroll(
    chat: &ChatState,
    total_lines: usize,
    inner_height: usize,
    message_line_ranges: &[(usize, usize)],
) -> usize {
    let max_scroll = total_lines.saturating_sub(inner_height);
    let mut scroll = chat.scroll_offset().min(max_scroll);

    if chat.scroll_locked() {
        return scroll;
    }

    if let Some(pin_idx) = chat.pinned_message_top() {
        if let Some(&(pin_start, _)) = message_line_ranges.get(pin_idx) {
            scroll = total_lines
                .saturating_sub(pin_start.saturating_add(inner_height))
                .min(max_scroll);
            return scroll;
        }
    }

    scroll
}

pub(crate) fn content_inner(area: Rect) -> Rect {
    area
}

pub(crate) fn scrollbar_layout_from_metrics(
    area: Rect,
    total_lines: usize,
    scroll: usize,
) -> Option<ChatScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let viewport = area.height as usize;
    if total_lines <= viewport {
        return None;
    }

    let max_scroll = total_lines.saturating_sub(viewport);
    let scroll = scroll.min(max_scroll);
    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let scrollbar = Rect::new(
        area.x
            .saturating_add(area.width)
            .saturating_sub(SCROLLBAR_WIDTH),
        area.y,
        SCROLLBAR_WIDTH,
        area.height,
    );

    let thumb_height = ((viewport * viewport) / total_lines).max(1).min(viewport) as u16;
    let track_span = scrollbar.height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll == 0 || track_span == 0 {
        0
    } else {
        (((max_scroll.saturating_sub(scroll)) * track_span as usize) + (max_scroll / 2))
            / max_scroll
    } as u16;
    let thumb = Rect::new(
        scrollbar.x,
        scrollbar.y.saturating_add(thumb_offset),
        scrollbar.width,
        thumb_height,
    );

    Some(ChatScrollbarLayout {
        content,
        scrollbar,
        thumb,
        scroll,
        max_scroll,
    })
}

pub(crate) fn scroll_offset_from_thumb_offset(
    thumb_offset: u16,
    track_span: u16,
    max_scroll: usize,
) -> usize {
    if max_scroll == 0 || track_span == 0 {
        return 0;
    }

    let toward_bottom =
        ((thumb_offset as usize * max_scroll) + (track_span as usize / 2)) / track_span as usize;
    max_scroll.saturating_sub(toward_bottom.min(max_scroll))
}

pub(crate) fn scrollbar_layout(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<ChatScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let metrics = build_transcript_metrics(
        chat,
        theme,
        area.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
    let scroll = resolved_scroll(
        chat,
        metrics.total_lines,
        area.height as usize,
        &metrics.message_line_ranges,
    );

    scrollbar_layout_from_metrics(area, metrics.total_lines, scroll)
}

pub(crate) fn latest_messages_overflow_viewport(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
    keep_messages: usize,
) -> bool {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return true;
    }

    let metrics = build_transcript_metrics(
        chat,
        theme,
        area.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
    let kept_start = metrics
        .message_line_ranges
        .len()
        .checked_sub(keep_messages)
        .and_then(|index| metrics.message_line_ranges.get(index))
        .map_or(0, |(start, _)| *start);

    metrics.total_lines.saturating_sub(kept_start) > area.height as usize
}

pub(crate) fn scrollbar_layout_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<ChatScrollbarLayout> {
    let metrics = cached_transcript_metrics(
        &snapshot.0,
        chat,
        padded_content_width(snapshot.0.inner.width as usize),
        current_tick,
        retry_wait_start_selected,
    );
    let scroll = resolved_scroll(
        chat,
        metrics.total_lines,
        snapshot.0.inner.height as usize,
        &metrics.message_line_ranges,
    );
    scrollbar_layout_from_metrics(snapshot.0.inner, metrics.total_lines, scroll)
}

pub(crate) fn scrollbar_scroll_offset_for_pointer(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
    pointer_row: u16,
    grab_offset: u16,
) -> Option<usize> {
    let layout = scrollbar_layout(area, chat, theme, current_tick, retry_wait_start_selected)?;
    let track_span = layout.scrollbar.height.saturating_sub(layout.thumb.height);
    let clamped_row = pointer_row.clamp(
        layout.scrollbar.y,
        layout
            .scrollbar
            .y
            .saturating_add(layout.scrollbar.height)
            .saturating_sub(1),
    );
    let desired_top = clamped_row
        .saturating_sub(layout.scrollbar.y)
        .saturating_sub(grab_offset)
        .min(track_span);

    Some(scroll_offset_from_thumb_offset(
        desired_top,
        track_span,
        layout.max_scroll,
    ))
}

pub(crate) fn scrollbar_scroll_offset_for_pointer_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
    pointer_row: u16,
    grab_offset: u16,
) -> Option<usize> {
    let layout = scrollbar_layout_from_cached_snapshot(
        snapshot,
        chat,
        current_tick,
        retry_wait_start_selected,
    )?;
    let track_span = layout.scrollbar.height.saturating_sub(layout.thumb.height);
    let clamped_row = pointer_row.clamp(
        layout.scrollbar.y,
        layout
            .scrollbar
            .y
            .saturating_add(layout.scrollbar.height)
            .saturating_sub(1),
    );
    let desired_top = clamped_row
        .saturating_sub(layout.scrollbar.y)
        .saturating_sub(grab_offset)
        .min(track_span);

    Some(scroll_offset_from_thumb_offset(
        desired_top,
        track_span,
        layout.max_scroll,
    ))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn visible_lines(
    all_lines: &[RenderedChatLine],
    inner_height: usize,
    scroll: usize,
) -> Vec<RenderedChatLine> {
    let total = all_lines.len();

    if total <= inner_height {
        let mut padded = Vec::with_capacity(inner_height);
        for _ in 0..inner_height.saturating_sub(total) {
            padded.push(RenderedChatLine::padding());
        }
        padded.extend_from_slice(all_lines);
        return padded;
    }

    if scroll == 0 {
        return all_lines[total - inner_height..].to_vec();
    }

    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    all_lines[start..end].to_vec()
}

pub(crate) fn visible_window_bounds(
    total: usize,
    inner_height: usize,
    scroll: usize,
) -> (usize, usize, usize) {
    if total <= inner_height {
        let padding = inner_height.saturating_sub(total);
        return (padding, 0, total);
    }

    if scroll == 0 {
        return (0, total - inner_height, total);
    }

    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    (0, start, end)
}

#[cfg(test)]
pub(crate) fn visible_rendered_lines(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<(Rect, Vec<RenderedChatLine>)> {
    if (chat.active_thread().is_none() && chat.streaming_content().is_empty())
        || area.width == 0
        || area.height == 0
    {
        return None;
    }

    let inner = content_inner(area);
    let metrics = build_transcript_metrics(
        chat,
        theme,
        inner.width as usize,
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
    let rendered = build_rendered_line_window(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
        start_idx,
        end_idx,
        &metrics,
    );
    let mut visible = Vec::with_capacity(inner.height as usize);
    for _ in 0..padding {
        visible.push(RenderedChatLine::padding());
    }
    visible.extend(rendered);

    Some((inner, visible))
}

pub(crate) fn selection_snapshot(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<SelectionSnapshot> {
    selection_snapshot_windowed(
        area,
        chat,
        theme,
        current_tick,
        retry_wait_start_selected,
        None,
    )
}

pub(crate) fn selection_snapshot_windowed(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
    cover: Option<(usize, usize)>,
) -> Option<SelectionSnapshot> {
    let inner = content_inner(area);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    crate::widgets::message::prepare_markdown_render_cache(chat.render_revision());
    let key = render_cache_key(area, chat, current_tick, retry_wait_start_selected);
    let metrics_key = transcript_metrics_cache_key(area, chat);

    let metrics = build_transcript_metrics(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
    if metrics.total_lines == 0 {
        return None;
    }

    let scroll = resolved_scroll(
        chat,
        metrics.total_lines,
        inner.height as usize,
        &metrics.message_line_ranges,
    );
    let (padding, start_idx, end_idx) =
        visible_window_bounds(metrics.total_lines, inner.height as usize, scroll);
    let overscan = (inner.height as usize).div_ceil(10).max(1);
    let mut rendered_start_idx = start_idx.saturating_sub(overscan);
    let mut rendered_end_idx = end_idx.saturating_add(overscan).min(metrics.total_lines);
    if let Some((cover_start, cover_end)) = cover {
        rendered_start_idx = rendered_start_idx.min(cover_start.min(metrics.total_lines));
        rendered_end_idx = rendered_end_idx.max(cover_end.min(metrics.total_lines));
    }
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
    Some(SelectionSnapshot {
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
    })
}

pub(crate) fn snapshot_line_at(
    snapshot: &SelectionSnapshot,
    row: usize,
) -> Option<&RenderedChatLine> {
    let local = row.checked_sub(snapshot.rendered_start_idx)?;
    snapshot.all_lines.get(local)
}

pub(crate) fn snapshot_rendered_end_idx(snapshot: &SelectionSnapshot) -> usize {
    snapshot
        .rendered_start_idx
        .saturating_add(snapshot.all_lines.len())
}

pub(crate) fn nearest_content_row(snapshot: &SelectionSnapshot, row: usize) -> Option<usize> {
    let current = snapshot_line_at(snapshot, row)?;
    let current_has_content = !matches!(current.kind, RenderedLineKind::Padding)
        && rendered_line_content_bounds(current).2 > rendered_line_content_bounds(current).1;
    if current_has_content {
        return Some(row);
    }

    if let Some(message_index) = current.message_index {
        let search_span =
            snapshot_rendered_end_idx(snapshot).saturating_sub(snapshot.rendered_start_idx);
        for distance in 1..search_span {
            if let Some(prev) = row.checked_sub(distance) {
                let Some(line) = snapshot_line_at(snapshot, prev) else {
                    continue;
                };
                if line.message_index == Some(message_index)
                    && !matches!(line.kind, RenderedLineKind::Padding)
                    && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
                {
                    return Some(prev);
                }
            }

            let next = row.saturating_add(distance);
            if let Some(line) = snapshot_line_at(snapshot, next) {
                if line.message_index == Some(message_index)
                    && !matches!(line.kind, RenderedLineKind::Padding)
                    && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
                {
                    return Some(next);
                }
            }
        }
    }

    for next in row + 1..snapshot_rendered_end_idx(snapshot) {
        let Some(line) = snapshot_line_at(snapshot, next) else {
            continue;
        };
        if !matches!(line.kind, RenderedLineKind::Padding)
            && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
        {
            return Some(next);
        }
    }

    for prev in (snapshot.rendered_start_idx..row).rev() {
        let Some(line) = snapshot_line_at(snapshot, prev) else {
            continue;
        };
        if !matches!(line.kind, RenderedLineKind::Padding)
            && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
        {
            return Some(prev);
        }
    }

    None
}

pub(crate) fn nearest_message_content_row(
    snapshot: &SelectionSnapshot,
    row: usize,
) -> Option<usize> {
    let current = snapshot_line_at(snapshot, row)?;
    let message_index = current.message_index?;
    if !matches!(current.kind, RenderedLineKind::Padding) {
        return Some(row);
    }

    let search_span =
        snapshot_rendered_end_idx(snapshot).saturating_sub(snapshot.rendered_start_idx);
    for distance in 1..search_span {
        if let Some(prev) = row.checked_sub(distance) {
            let Some(line) = snapshot_line_at(snapshot, prev) else {
                continue;
            };
            if line.message_index == Some(message_index)
                && !matches!(line.kind, RenderedLineKind::Padding)
            {
                return Some(prev);
            }
        }

        let next = row.saturating_add(distance);
        if let Some(line) = snapshot_line_at(snapshot, next) {
            if line.message_index == Some(message_index)
                && !matches!(line.kind, RenderedLineKind::Padding)
            {
                return Some(next);
            }
        }
    }

    None
}

pub(crate) fn display_slice(text: &str, start_col: usize, end_col: usize) -> String {
    if start_col >= end_col {
        return String::new();
    }

    let mut result = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        let next = col + width;
        let overlaps = if width == 0 {
            col >= start_col && col < end_col
        } else {
            next > start_col && col < end_col
        };
        if overlaps {
            result.push(ch);
        }
        col = next;
        if col >= end_col {
            break;
        }
    }

    result
}

pub(crate) fn highlight_line_range(
    line: &mut Line<'static>,
    start_col: usize,
    end_col: usize,
    highlight: Style,
) {
    if start_col >= end_col {
        return;
    }

    let original_spans = std::mem::take(&mut line.spans);
    let mut spans = Vec::new();
    let mut col = 0usize;

    for span in original_spans {
        let mut before = String::new();
        let mut selected = String::new();
        let mut after = String::new();

        for ch in span.content.chars() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            let next = col + width;
            let overlaps = if width == 0 {
                col >= start_col && col < end_col
            } else {
                next > start_col && col < end_col
            };

            if overlaps {
                selected.push(ch);
            } else if col < start_col {
                before.push(ch);
            } else {
                after.push(ch);
            }

            col = next;
        }

        if !before.is_empty() {
            spans.push(Span::styled(before, span.style));
        }
        if !selected.is_empty() {
            spans.push(Span::styled(selected, span.style.patch(highlight)));
        }
        if !after.is_empty() {
            spans.push(Span::styled(after, span.style));
        }
    }

    line.spans = spans;
}
