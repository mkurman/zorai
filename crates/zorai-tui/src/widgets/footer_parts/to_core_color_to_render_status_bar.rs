use ratatui::prelude::*;
use ratatui::style::{Color, Modifier};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui_core::buffer::Buffer as CoreBuffer;
use ratatui_core::layout::Rect as CoreRect;
use ratatui_core::style::{Color as CoreColor, Modifier as CoreModifier, Style as CoreStyle};
use ratatui_core::widgets::Widget as CoreWidget;
use ratatui_textarea::{CursorMove, TextArea};

use crate::app::Attachment;
use crate::state::input::InputState;
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusBarHitTarget {
    QueuedPrompts,
}

fn to_core_color(color: Color) -> CoreColor {
    match color {
        Color::Reset => CoreColor::Reset,
        Color::Black => CoreColor::Black,
        Color::Red => CoreColor::Red,
        Color::Green => CoreColor::Green,
        Color::Yellow => CoreColor::Yellow,
        Color::Blue => CoreColor::Blue,
        Color::Magenta => CoreColor::Magenta,
        Color::Cyan => CoreColor::Cyan,
        Color::Gray => CoreColor::Gray,
        Color::DarkGray => CoreColor::DarkGray,
        Color::LightRed => CoreColor::LightRed,
        Color::LightGreen => CoreColor::LightGreen,
        Color::LightYellow => CoreColor::LightYellow,
        Color::LightBlue => CoreColor::LightBlue,
        Color::LightMagenta => CoreColor::LightMagenta,
        Color::LightCyan => CoreColor::LightCyan,
        Color::White => CoreColor::White,
        Color::Indexed(idx) => CoreColor::Indexed(idx),
        Color::Rgb(r, g, b) => CoreColor::Rgb(r, g, b),
    }
}

fn to_core_modifier(modifier: Modifier) -> CoreModifier {
    let mut result = CoreModifier::empty();
    if modifier.contains(Modifier::BOLD) {
        result |= CoreModifier::BOLD;
    }
    if modifier.contains(Modifier::DIM) {
        result |= CoreModifier::DIM;
    }
    if modifier.contains(Modifier::ITALIC) {
        result |= CoreModifier::ITALIC;
    }
    if modifier.contains(Modifier::UNDERLINED) {
        result |= CoreModifier::UNDERLINED;
    }
    if modifier.contains(Modifier::SLOW_BLINK) {
        result |= CoreModifier::SLOW_BLINK;
    }
    if modifier.contains(Modifier::RAPID_BLINK) {
        result |= CoreModifier::RAPID_BLINK;
    }
    if modifier.contains(Modifier::REVERSED) {
        result |= CoreModifier::REVERSED;
    }
    if modifier.contains(Modifier::HIDDEN) {
        result |= CoreModifier::HIDDEN;
    }
    if modifier.contains(Modifier::CROSSED_OUT) {
        result |= CoreModifier::CROSSED_OUT;
    }
    result
}

fn to_core_style(style: Style) -> CoreStyle {
    let mut result = CoreStyle::default();
    if let Some(fg) = style.fg {
        result = result.fg(to_core_color(fg));
    }
    if let Some(bg) = style.bg {
        result = result.bg(to_core_color(bg));
    }

    let add = to_core_modifier(style.add_modifier);
    if !add.is_empty() {
        result = result.add_modifier(add);
    }

    let remove = to_core_modifier(style.sub_modifier);
    if !remove.is_empty() {
        result = result.remove_modifier(remove);
    }

    result
}

fn from_core_color(color: CoreColor) -> Color {
    match color {
        CoreColor::Reset => Color::Reset,
        CoreColor::Black => Color::Black,
        CoreColor::Red => Color::Red,
        CoreColor::Green => Color::Green,
        CoreColor::Yellow => Color::Yellow,
        CoreColor::Blue => Color::Blue,
        CoreColor::Magenta => Color::Magenta,
        CoreColor::Cyan => Color::Cyan,
        CoreColor::Gray => Color::Gray,
        CoreColor::DarkGray => Color::DarkGray,
        CoreColor::LightRed => Color::LightRed,
        CoreColor::LightGreen => Color::LightGreen,
        CoreColor::LightYellow => Color::LightYellow,
        CoreColor::LightBlue => Color::LightBlue,
        CoreColor::LightMagenta => Color::LightMagenta,
        CoreColor::LightCyan => Color::LightCyan,
        CoreColor::White => Color::White,
        CoreColor::Indexed(idx) => Color::Indexed(idx),
        CoreColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn from_core_modifier(modifier: CoreModifier) -> Modifier {
    let mut result = Modifier::empty();
    if modifier.contains(CoreModifier::BOLD) {
        result |= Modifier::BOLD;
    }
    if modifier.contains(CoreModifier::DIM) {
        result |= Modifier::DIM;
    }
    if modifier.contains(CoreModifier::ITALIC) {
        result |= Modifier::ITALIC;
    }
    if modifier.contains(CoreModifier::UNDERLINED) {
        result |= Modifier::UNDERLINED;
    }
    if modifier.contains(CoreModifier::SLOW_BLINK) {
        result |= Modifier::SLOW_BLINK;
    }
    if modifier.contains(CoreModifier::RAPID_BLINK) {
        result |= Modifier::RAPID_BLINK;
    }
    if modifier.contains(CoreModifier::REVERSED) {
        result |= Modifier::REVERSED;
    }
    if modifier.contains(CoreModifier::HIDDEN) {
        result |= Modifier::HIDDEN;
    }
    if modifier.contains(CoreModifier::CROSSED_OUT) {
        result |= Modifier::CROSSED_OUT;
    }
    result
}

fn render_textarea_bridge(frame: &mut Frame, area: Rect, textarea: &TextArea<'_>) {
    let core_area = CoreRect::new(area.x, area.y, area.width, area.height);
    let mut core_buffer = CoreBuffer::empty(core_area);

    #[allow(deprecated)]
    textarea.widget().render(core_area, &mut core_buffer);

    let buffer = frame.buffer_mut();
    for y in 0..area.height {
        for x in 0..area.width {
            let Some(core_cell) = core_buffer.cell((area.x + x, area.y + y)) else {
                continue;
            };
            let Some(cell) = buffer.cell_mut((area.x + x, area.y + y)) else {
                continue;
            };
            cell.set_symbol(core_cell.symbol());
            cell.fg = from_core_color(core_cell.fg);
            cell.bg = from_core_color(core_cell.bg);
            cell.modifier = from_core_modifier(core_cell.modifier);
            cell.skip = core_cell.skip;
        }
    }
}

fn animated_placeholder(tick: u64) -> String {
    let placeholders = [
        "Ask anything... plan · solve · ship",
        "Try: /settings to configure your AI",
        "Ctrl+Enter for multi-line input",
        "/attach <file> to include context",
        "Ctrl+P for command palette",
        "/help to see all keyboard shortcuts",
        "Paste a file path to auto-attach it",
        "Ctrl+Z to undo, Ctrl+Y to redo",
        "What would you like to build today?",
        "Describe a bug and I'll investigate",
        "Ask me to explain any code file",
    ];
    let placeholder = placeholders[((tick / 80) as usize) % placeholders.len()];
    format!("▶ {}", placeholder)
}

fn input_placeholder(
    tick: u64,
    attachments: &[Attachment],
    agent_activity: Option<&str>,
    input_notice: Option<(&str, Style)>,
) -> Option<(String, Style)> {
    if !attachments.is_empty() {
        return None;
    }

    if let Some((notice, style)) = input_notice {
        return Some((format!("▶ {}", notice), style));
    }

    if let Some(activity) = agent_activity {
        let spinner_frames = ["⢿", "⣻", "⣽", "⣾", "⣷", "⣯", "⣟", "⡿"];
        let spinner = spinner_frames[((tick / 4) as usize) % spinner_frames.len()];
        return Some((
            format!("{spinner} {activity}..."),
            Style::default().fg(Color::Indexed(178)),
        ));
    }

    Some((
        animated_placeholder(tick),
        Style::default().fg(Color::Indexed(239)),
    ))
}

fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let mut remaining = offset;

    for (row, line) in text.split('\n').enumerate() {
        if remaining <= line.len() {
            let col = line[..remaining].chars().count().min(line.chars().count());
            return (row, col);
        }
        remaining = remaining.saturating_sub(line.len() + 1);
    }

    let last_row = text.split('\n').count().saturating_sub(1);
    let last_col = text
        .split('\n')
        .last()
        .map(|line| line.chars().count())
        .unwrap_or(0);
    (last_row, last_col)
}

fn renderable_textarea(
    input: &InputState,
    theme: &ThemeTokens,
    focused: bool,
    attachments: &[Attachment],
    wrap_width: usize,
    tick: u64,
    agent_activity: Option<&str>,
    input_notice: Option<(&str, Style)>,
) -> TextArea<'static> {
    let (display_buffer, display_cursor) = input.wrapped_display_buffer_and_cursor(wrap_width);
    let mut textarea = TextArea::from(display_buffer.split('\n'));
    textarea.set_style(to_core_style(theme.fg_active));
    textarea.set_cursor_line_style(CoreStyle::default());
    textarea.set_cursor_style(to_core_style(
        theme
            .accent_primary
            .add_modifier(Modifier::REVERSED | Modifier::BOLD),
    ));

    if focused && !display_buffer.is_empty() {
        let (row, col) = offset_to_line_col(&display_buffer, display_cursor);
        textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
    }

    if display_buffer.is_empty() {
        if let Some((text, style)) =
            input_placeholder(tick, attachments, agent_activity, input_notice)
        {
            textarea.set_placeholder_text(text);
            textarea.set_placeholder_style(to_core_style(style));
        }
    }

    textarea
}

fn render_attachments(
    frame: &mut Frame,
    area: Rect,
    attachments: &[Attachment],
    theme: &ThemeTokens,
) {
    if attachments.is_empty() || area.height == 0 {
        return;
    }

    let visible = attachments.len().min(area.height as usize);
    let mut lines = Vec::with_capacity(visible);
    for att in attachments.iter().take(visible) {
        let size_str = if att.size_bytes > 1024 {
            format!("{:.1} KB", att.size_bytes as f64 / 1024.0)
        } else {
            format!("{} B", att.size_bytes)
        };
        lines.push(Line::from(vec![
            Span::styled("× ", theme.fg_dim),
            Span::styled("📎 ", theme.accent_secondary),
            Span::styled(att.filename.clone(), theme.fg_active),
            Span::styled(format!(" ({size_str})"), theme.fg_dim),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

pub fn render_input(
    frame: &mut Frame,
    area: Rect,
    input: &InputState,
    theme: &ThemeTokens,
    focused: bool,
    modal_open: bool,
    attachments: &[Attachment],
    tick: u64,
    agent_activity: Option<&str>,
    input_notice: Option<(&str, Style)>,
) {
    let border_style = if modal_open {
        theme.fg_dim
    } else if focused {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 1 {
        return;
    }

    if modal_open {
        let input_line = Line::from(vec![
            Span::raw(" "),
            Span::styled("▶", theme.fg_dim),
            Span::styled(" (modal open)", theme.fg_dim),
        ]);
        frame.render_widget(Paragraph::new(vec![input_line]), inner);
        return;
    }

    let attachment_rows = attachments
        .len()
        .min(inner.height.saturating_sub(1) as usize) as u16;
    let chunks = if attachment_rows > 0 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(attachment_rows), Constraint::Min(1)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(inner)
    };

    if attachment_rows > 0 {
        render_attachments(frame, chunks[0], attachments, theme);
    }

    let raw_textarea_area = *chunks.last().unwrap_or(&inner);
    let textarea_area = Rect::new(
        raw_textarea_area.x.saturating_add(1),
        raw_textarea_area.y,
        raw_textarea_area.width.saturating_sub(2),
        raw_textarea_area.height,
    );
    if textarea_area.height == 0 {
        return;
    }

    let textarea = renderable_textarea(
        input,
        theme,
        focused,
        attachments,
        textarea_area.width.max(1) as usize,
        tick,
        agent_activity,
        input_notice,
    );
    render_textarea_bridge(frame, textarea_area, &textarea);
}

pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    theme: &ThemeTokens,
    connected: bool,
    has_error: bool,
    error_active: bool,
    tick: u64,
    error_tick: u64,
    voice_recording: bool,
    voice_playing: bool,
    queued_count: usize,
    _status_line: &str,
) {
    let mut spans = vec![Span::raw(" ")];

    if connected {
        spans.push(Span::styled("●", theme.accent_success));
        spans.push(Span::styled(" daemon", theme.fg_dim));
    } else {
        spans.push(Span::styled("●", theme.accent_danger));
        spans.push(Span::styled(" daemon", theme.fg_dim));
    }

    if has_error {
        let error_color = if error_active {
            let elapsed = tick.saturating_sub(error_tick);
            let pulse_phase = (elapsed / 10) % 2;
            if pulse_phase == 0 {
                Style::default().fg(Color::Indexed(203))
            } else {
                Style::default().fg(Color::Indexed(88))
            }
        } else {
            theme.accent_danger
        };
        spans.push(Span::raw("  "));
        spans.push(Span::styled("●", error_color));
        spans.push(Span::styled(" error", theme.fg_dim));
    }

    if voice_recording {
        let rec_style = if (tick / 8) % 2 == 0 {
            theme.accent_danger
        } else {
            Style::default().fg(Color::Indexed(203))
        };
        spans.push(Span::raw("  "));
        spans.push(Span::styled("●", rec_style));
        spans.push(Span::styled(" REC", rec_style));
    }

    if voice_playing {
        let playing_style = if (tick / 8) % 2 == 0 {
            theme.accent_secondary
        } else {
            theme.fg_active
        };
        spans.push(Span::raw("  "));
        spans.push(Span::styled("🔊 PLAYING", playing_style));
    }

    if queued_count > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("●", theme.accent_secondary));
        spans.push(Span::styled(
            format!(" queued({queued_count})"),
            theme.fg_dim,
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled("ctrl+q", theme.fg_active));
        spans.push(Span::styled(":queue", theme.fg_dim));
    }

    spans.push(Span::raw("    "));
    spans.push(Span::styled("tab", theme.fg_active));
    spans.push(Span::styled(":focus  ", theme.fg_dim));
    spans.push(Span::styled("ctrl+t", theme.fg_active));
    spans.push(Span::styled(":threads  ", theme.fg_dim));
    spans.push(Span::styled("ctrl+g", theme.fg_active));
    spans.push(Span::styled(":goals  ", theme.fg_dim));
    spans.push(Span::styled("ctrl+a", theme.fg_active));
    spans.push(Span::styled(":approvals  ", theme.fg_dim));
    spans.push(Span::styled("ctrl+n", theme.fg_active));
    spans.push(Span::styled(":notifications  ", theme.fg_dim));
    spans.push(Span::styled("ctrl+p", theme.fg_active));
    spans.push(Span::styled(":cmd  ", theme.fg_dim));
    spans.push(Span::styled("/", theme.fg_active));
    spans.push(Span::styled(":slash  ", theme.fg_dim));
    if has_error {
        spans.push(Span::styled("ctrl+e", theme.accent_danger));
        spans.push(Span::styled(":error  ", theme.fg_dim));
    }
    spans.push(Span::styled("/quit", theme.fg_active));
    spans.push(Span::styled(":exit", theme.fg_dim));

    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(vec![line]), area);
}
