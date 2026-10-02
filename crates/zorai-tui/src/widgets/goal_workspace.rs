#![allow(dead_code)]

use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspacePane, GoalWorkspaceState};
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::chat::SelectionPoint;
use crate::widgets::duration_format::format_duration_ms;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthChar;

#[path = "goal_workspace_plan.rs"]
mod plan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalWorkspaceHitTarget {
    ModeTab(GoalWorkspaceMode),
    PlanPromptToggle,
    PlanMainThread(String),
    PlanStep(String),
    PlanTodo { step_id: String, todo_id: String },
    TimelineRow(usize),
    ThreadRow(String),
    FooterAction(GoalWorkspaceAction),
    DetailFile(String),
    DetailCheckpoint(String),
    DetailTask(String),
    DetailThread(String),
    DetailAction(GoalWorkspaceAction),
    DetailTimelineDetails(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalWorkspaceAction {
    ToggleGoalRun,
    OpenActions,
    AcceptReview,
    SoftReject,
    HardReject,
    RefreshGoal,
}

#[derive(Clone, Copy)]
struct GoalWorkspaceLayoutRects {
    summary: Rect,
    footer: Rect,
    plan: Rect,
    timeline: Rect,
    details: Rect,
}

fn workspace_layout(area: Rect) -> Option<GoalWorkspaceLayoutRects> {
    if area.width < 3 || area.height < 8 {
        return None;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(sections[1]);
    Some(GoalWorkspaceLayoutRects {
        summary: sections[0],
        footer: sections[2],
        plan: columns[0],
        timeline: columns[1],
        details: columns[2],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GoalStepConfidence {
    Low,
    Medium,
    High,
}

impl GoalStepConfidence {
    fn symbol(self) -> &'static str {
        match self {
            Self::Low => "˅",
            Self::Medium => "=",
            Self::High => "˄",
        }
    }

    fn style(self, theme: &ThemeTokens) -> Style {
        match self {
            Self::Low => theme.accent_danger,
            Self::Medium => theme.accent_secondary,
            Self::High => theme.accent_success,
        }
    }
}

pub(super) fn split_goal_step_title(title: &str) -> (Option<GoalStepConfidence>, &str) {
    if let Some(rest) = title.strip_prefix("[LOW]") {
        (Some(GoalStepConfidence::Low), rest.trim_start())
    } else if let Some(rest) = title.strip_prefix("[MEDIUM]") {
        (Some(GoalStepConfidence::Medium), rest.trim_start())
    } else if let Some(rest) = title.strip_prefix("[HIGH]") {
        (Some(GoalStepConfidence::High), rest.trim_start())
    } else {
        (None, title)
    }
}

pub(super) fn goal_step_title_matches(step_title: &str, candidate: Option<&str>) -> bool {
    let (_, cleaned_step_title) = split_goal_step_title(step_title);
    candidate.is_some_and(|title| {
        let (_, cleaned_candidate_title) = split_goal_step_title(title);
        title == step_title || cleaned_candidate_title == cleaned_step_title
    })
}

fn goal_step_title_spans(
    title: &str,
    title_style: Style,
    theme: &ThemeTokens,
) -> Vec<Span<'static>> {
    let (confidence, cleaned_title) = split_goal_step_title(title);
    let mut spans = vec![Span::styled(cleaned_title.to_string(), title_style)];
    if let Some(confidence) = confidence {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(confidence.symbol(), confidence.style(theme)));
    }
    spans
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    tick_counter: u64,
) {
    render_with_selection(
        frame,
        area,
        tasks,
        goal_run_id,
        state,
        theme,
        tick_counter,
        None,
    );
}

pub fn render_with_selection(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    tick_counter: u64,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    let Some(layout) = workspace_layout(area) else {
        return;
    };

    let center_inner = Block::default()
        .borders(Borders::ALL)
        .inner(layout.timeline);
    let detail_inner = Block::default().borders(Borders::ALL).inner(layout.details);
    let center_rows = center_rows(
        tasks,
        goal_run_id,
        state,
        center_inner.width as usize,
        theme,
        tick_counter,
    );
    let detail_rows = detail_lines(
        tasks,
        goal_run_id,
        state,
        detail_inner.width as usize,
        theme,
    );

    render_summary(frame, layout.summary, state, theme);
    render_plan(
        frame,
        layout.plan,
        tasks,
        goal_run_id,
        state,
        theme,
        tick_counter,
        mouse_selection,
    );
    render_center_pane(frame, layout.timeline, state, theme, &center_rows);
    render_details(frame, layout.details, state, theme, &detail_rows);
    render_step_footer(frame, layout.footer, tasks, goal_run_id, state, theme);
}

pub fn pane_at(area: Rect, mouse: Position) -> Option<GoalWorkspacePane> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    if rect_contains(layout.summary, mouse) {
        return Some(GoalWorkspacePane::CommandBar);
    }

    if rect_contains(layout.plan, mouse) {
        Some(GoalWorkspacePane::Plan)
    } else if rect_contains(layout.timeline, mouse) {
        Some(GoalWorkspacePane::Timeline)
    } else if rect_contains(layout.details, mouse) {
        Some(GoalWorkspacePane::Details)
    } else {
        None
    }
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<GoalWorkspaceHitTarget> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    if let Some(tab_hit) = summary_hit_test(layout.summary, mouse) {
        return Some(tab_hit);
    }
    if let Some(footer_hit) = footer_hit_test(layout.footer, tasks, goal_run_id, state, mouse) {
        return Some(footer_hit);
    }

    match pane_at(area, mouse)? {
        GoalWorkspacePane::Plan => {
            let plan_area = layout.plan;
            let inner = Block::default().borders(Borders::ALL).inner(plan_area);
            if !rect_contains(inner, mouse) {
                return None;
            }
            let rows = plan_visual_row_targets(tasks, goal_run_id, state, inner.width as usize);
            let row_index = resolved_plan_scroll(rows.len(), inner.height as usize, state)
                .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            rows.get(row_index).cloned().flatten()
        }
        GoalWorkspacePane::Timeline => {
            let inner = Block::default()
                .borders(Borders::ALL)
                .inner(layout.timeline);
            if !rect_contains(inner, mouse) {
                return None;
            }
            let rows = center_visual_targets(tasks, goal_run_id, state, inner.width as usize);
            let row_index = resolved_timeline_scroll(rows.len(), inner.height as usize, state)
                .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            rows.get(row_index).cloned().flatten()
        }
        GoalWorkspacePane::Details => {
            let inner = Block::default().borders(Borders::ALL).inner(layout.details);
            if !rect_contains(inner, mouse) {
                return None;
            }
            let rows = detail_visual_targets(tasks, goal_run_id, state, inner.width as usize);
            let row_index = resolved_detail_scroll(rows.len(), inner.height as usize, state)
                .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            rows.get(row_index).cloned().flatten()
        }
        GoalWorkspacePane::CommandBar => None,
    }
}

pub fn max_plan_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default().borders(Borders::ALL).inner(layout.plan);
    let rows = plan_visual_row_targets(tasks, goal_run_id, state, inner.width as usize);
    rows.len().saturating_sub(inner.height as usize)
}

pub fn timeline_row_count(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    center_targets(tasks, goal_run_id, state).len()
}

pub fn max_timeline_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(layout.timeline);
    center_visual_targets(tasks, goal_run_id, state, inner.width as usize)
        .len()
        .saturating_sub(inner.height as usize)
}

pub fn detail_target_count(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    detail_targets(tasks, goal_run_id, state).len()
}

pub fn max_detail_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default().borders(Borders::ALL).inner(layout.details);
    detail_visual_targets(tasks, goal_run_id, state, inner.width as usize)
        .len()
        .saturating_sub(inner.height as usize)
}

pub fn timeline_viewport_height(area: Rect) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    Block::default()
        .borders(Borders::ALL)
        .inner(layout.timeline)
        .height as usize
}

pub fn detail_viewport_height(area: Rect) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    Block::default()
        .borders(Borders::ALL)
        .inner(layout.details)
        .height as usize
}

pub fn detail_row_for_target(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    target: &GoalWorkspaceHitTarget,
) -> Option<usize> {
    detail_targets(tasks, goal_run_id, state)
        .into_iter()
        .position(|(_, candidate)| candidate == *target)
}

pub fn selection_point_from_mouse(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<SelectionPoint> {
    let (inner, row_index, wrapped_row_index) =
        plan_inner_row_index(area, tasks, goal_run_id, state, mouse)?;
    let rows = plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default());
    let line = &rows.get(row_index)?.line;
    let width = line_display_width(line);
    let col = mouse.x.saturating_sub(inner.x) as usize;
    let (segment_start, segment_end) =
        wrapped_segment_display_bounds(line, inner.width as usize, wrapped_row_index)?;
    Some(SelectionPoint {
        row: row_index,
        col: segment_start
            .saturating_add(col.min(segment_end.saturating_sub(segment_start)))
            .min(width),
    })
}

pub fn selection_points_from_mouse(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    start: Position,
    end: Position,
) -> Option<(SelectionPoint, SelectionPoint)> {
    Some((
        selection_point_from_mouse(area, tasks, goal_run_id, state, start)?,
        selection_point_from_mouse(area, tasks, goal_run_id, state, end)?,
    ))
}

pub fn selected_text(
    _area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let rows = plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default());
    let (start_point, end_point) =
        if start.row < end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    if start_point == end_point {
        return None;
    }

    let mut lines = Vec::new();
    for row in start_point.row..=end_point.row {
        let line = &rows.get(row)?.line;
        let plain = line_plain_text(line);
        let width = line_display_width(line);
        let from = if row == start_point.row {
            start_point.col.min(width)
        } else {
            0
        };
        let to = if row == end_point.row {
            end_point.col.min(width).max(from)
        } else {
            width
        };
        lines.push(display_slice(&plain, from, to));
    }

    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

const MODE_TABS: &[(GoalWorkspaceMode, &str)] = &[
    (GoalWorkspaceMode::Work, "Work"),
    (GoalWorkspaceMode::Review, "Review"),
    (GoalWorkspaceMode::Activity, "Activity"),
    (GoalWorkspaceMode::Threads, "Threads"),
    (GoalWorkspaceMode::Files, "Files"),
];

fn render_summary(frame: &mut Frame, area: Rect, state: &GoalWorkspaceState, theme: &ThemeTokens) {
    let block = Block::default()
        .title(" Goal Mission Control ")
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::CommandBar {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut spans = Vec::new();
    for (index, (mode, label)) in MODE_TABS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if state.mode() == *mode {
            theme.accent_secondary
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(*label, style));
    }
    let text = Line::from(spans);
    frame.render_widget(Paragraph::new(text), inner);
}

fn summary_hit_test(area: Rect, mouse: Position) -> Option<GoalWorkspaceHitTarget> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !rect_contains(inner, mouse) || mouse.y != inner.y {
        return None;
    }

    let mut x = inner.x;
    for (index, (mode, label)) in MODE_TABS.iter().enumerate() {
        if index > 0 {
            x = x.saturating_add(2);
        }
        let width = label.chars().count() as u16;
        if mouse.x >= x && mouse.x < x.saturating_add(width) {
            return Some(GoalWorkspaceHitTarget::ModeTab(*mode));
        }
        x = x.saturating_add(width);
    }
    None
}

fn render_plan(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    tick_counter: u64,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    let block = Block::default()
        .title(" Goal ")
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Plan {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_style = selected_row_style(state.focused_pane() == GoalWorkspacePane::Plan);
    let selected_visual_row =
        plan_visual_row_for_selection(tasks, goal_run_id, state, inner.width as usize);
    let mut visual_row = 0usize;
    let mut plan_rows = plan::build_rows(tasks, goal_run_id, state, theme);
    if let Some((start, end)) = mouse_selection {
        let (start_point, end_point) =
            if start.row < end.row || (start.row == end.row && start.col <= end.col) {
                (start, end)
            } else {
                (end, start)
            };
        let highlight = Style::default().bg(Color::Indexed(31));
        for row in start_point.row..=end_point.row {
            if let Some(plan_row) = plan_rows.get_mut(row) {
                let line_width = line_display_width(&plan_row.line);
                let from = if row == start_point.row {
                    start_point.col.min(line_width)
                } else {
                    0
                };
                let to = if row == end_point.row {
                    end_point.col.min(line_width).max(from)
                } else {
                    line_width
                };
                highlight_line_range(&mut plan_row.line, from, to, highlight);
            }
        }
    }
    let lines = plan_rows
        .into_iter()
        .map(|row| {
            let line = styled_plan_row(row, theme, tick_counter);
            let row_visual_height = wrapped_visual_height(&line, inner.width as usize);
            let is_selected = selected_visual_row
                .map(|selected| selected >= visual_row && selected < visual_row + row_visual_height)
                .unwrap_or(false);
            visual_row = visual_row.saturating_add(row_visual_height);
            if is_selected {
                line.style(selected_style)
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    let scroll = resolved_plan_scroll(
        plan_visual_row_targets(tasks, goal_run_id, state, inner.width as usize).len(),
        inner.height as usize,
        state,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn styled_plan_row(
    row: plan::GoalWorkspacePlanRow,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> Line<'static> {
    let mut spans = row.line.spans;
    if let (Some(marker_state), Some(marker_span_index)) = (row.marker_state, row.marker_span_index)
    {
        let (symbol, style) = plan_marker_display(marker_state, theme, tick_counter);
        if let Some(span) = spans.get_mut(marker_span_index) {
            *span = Span::styled(format!("{symbol} "), style);
        }
    }
    if let (Some(confidence), Some(confidence_span_index)) =
        (row.confidence, row.confidence_span_index)
    {
        if let Some(span) = spans.get_mut(confidence_span_index) {
            *span = Span::styled(confidence.symbol(), confidence.style(theme));
        }
    }
    Line::from(spans)
}

fn plan_marker_display(
    state: plan::GoalWorkspacePlanMarkerState,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> (&'static str, Style) {
    match state {
        plan::GoalWorkspacePlanMarkerState::Pending => ("○", theme.fg_dim),
        plan::GoalWorkspacePlanMarkerState::Completed => ("●", theme.accent_success),
        plan::GoalWorkspacePlanMarkerState::Running => (
            if tick_counter % 2 == 0 { "◉" } else { "●" },
            theme.accent_secondary,
        ),
        plan::GoalWorkspacePlanMarkerState::Error => (
            if tick_counter % 2 == 0 { "◉" } else { "◎" },
            theme.accent_danger,
        ),
    }
}

fn render_placeholder(frame: &mut Frame, area: Rect, title: &str, body: &str, theme: &ThemeTokens) {
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(body)
            .style(theme.fg_dim)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_center_pane(
    frame: &mut Frame,
    area: Rect,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    center_rows: &[WorkspaceVisualRow],
) {
    let block = Block::default()
        .title(center_pane_title(state.mode()))
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Timeline {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_target = distinct_workspace_targets(center_rows)
        .get(state.selected_timeline_row())
        .cloned();
    let mut lines = center_rows
        .iter()
        .map(|row| row.line.clone())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("No data available.", theme.fg_dim)));
    }
    let selected_style = selected_row_style(state.focused_pane() == GoalWorkspacePane::Timeline);
    let scroll = resolved_timeline_scroll(
        workspace_visual_targets(center_rows, inner.width as usize).len(),
        inner.height as usize,
        state,
    );
    for (line, row) in lines.iter_mut().zip(center_rows.iter()) {
        if selected_target.is_some() && row.target == selected_target {
            *line = line.clone().style(selected_style);
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_details(
    frame: &mut Frame,
    area: Rect,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    detail_rows: &[(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)],
) {
    let block = Block::default()
        .title(detail_pane_title(state.mode()))
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Details {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut target_index = 0usize;
    let mut lines = detail_rows
        .iter()
        .cloned()
        .map(|(_, target, line)| {
            if target.is_some() {
                let current_target_index = target_index;
                target_index = target_index.saturating_add(1);
                if current_target_index == state.selected_detail_row() {
                    line.style(selected_row_style(
                        state.focused_pane() == GoalWorkspacePane::Details,
                    ))
                } else {
                    line
                }
            } else {
                line
            }
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No details available.",
            theme.fg_dim,
        )));
    }
    let scroll = resolved_detail_scroll(
        detail_visual_targets_from_lines(detail_rows, inner.width as usize).len(),
        inner.height as usize,
        state,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_step_footer(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Goal Actions ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let spans = footer_segments(tasks, goal_run_id, state, theme)
        .into_iter()
        .map(|segment| Span::styled(segment.text, segment.style))
        .collect::<Vec<_>>();
    let line = if spans.is_empty() {
        Line::from(Span::styled("No step selected.", theme.fg_dim))
    } else {
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), inner);
}

#[derive(Clone)]
struct WorkspaceVisualRow {
    target: Option<GoalWorkspaceHitTarget>,
    line: Line<'static>,
}

fn center_pane_title(mode: GoalWorkspaceMode) -> &'static str {
    match mode {
        GoalWorkspaceMode::Work => " Worker progress ",
        GoalWorkspaceMode::Review => " Supervisor review ",
        GoalWorkspaceMode::Activity => " Run activity ",
        GoalWorkspaceMode::Threads => " Threads ",
        GoalWorkspaceMode::Files => " Files ",
    }
}

#[derive(Clone)]
struct FooterSegment {
    text: String,
    style: Style,
    target: Option<GoalWorkspaceHitTarget>,
}

fn footer_segments(
    tasks: &TaskState,
    goal_run_id: &str,
    _state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) -> Vec<FooterSegment> {
    let mut segments = Vec::new();
    let run = tasks.goal_run_by_id(goal_run_id);
    if run.is_none() {
        return segments;
    }
    let status_label = if run.is_some_and(|run| {
        matches!(
            run.status,
            Some(crate::state::task::GoalRunStatus::AwaitingReview)
        )
    }) {
        "Supervisor review".to_string()
    } else {
        latest_worker_todos(tasks, goal_run_id)
            .into_iter()
            .find(|todo| {
                matches!(
                    todo.status,
                    Some(crate::state::task::TodoStatus::InProgress)
                )
            })
            .map(|todo| todo.content)
            .unwrap_or_else(|| "Goal".to_string())
    };
    segments.push(FooterSegment {
        text: status_label,
        style: theme.fg_active,
        target: None,
    });
    segments.push(FooterSegment {
        text: "  ".to_string(),
        style: theme.fg_dim,
        target: None,
    });

    if let Some(run) = run {
        if let Some((label, style)) = goal_toggle_action_label(run, theme) {
            segments.push(FooterSegment {
                text: format!("{label} Ctrl+S"),
                style,
                target: Some(GoalWorkspaceHitTarget::FooterAction(
                    GoalWorkspaceAction::ToggleGoalRun,
                )),
            });
            segments.push(FooterSegment {
                text: "  ".to_string(),
                style: theme.fg_dim,
                target: None,
            });
        }

        let mut actions = vec![(
            GoalWorkspaceAction::OpenActions,
            "[Actions]",
            "A",
            theme.accent_primary,
        )];
        if matches!(
            run.status,
            Some(crate::state::task::GoalRunStatus::AwaitingReview)
        ) {
            actions.extend([
                (
                    GoalWorkspaceAction::AcceptReview,
                    "[Accept]",
                    "Y",
                    theme.accent_success,
                ),
                (
                    GoalWorkspaceAction::SoftReject,
                    "[Soft reject]",
                    "S",
                    theme.accent_secondary,
                ),
                (
                    GoalWorkspaceAction::HardReject,
                    "[Hard reject]",
                    "H",
                    theme.accent_danger,
                ),
            ]);
        }
        for (action, label, hotkey, style) in actions {
            segments.push(FooterSegment {
                text: format!("{label} {hotkey}"),
                style,
                target: Some(GoalWorkspaceHitTarget::FooterAction(action)),
            });
            segments.push(FooterSegment {
                text: "  ".to_string(),
                style: theme.fg_dim,
                target: None,
            });
        }
    }

    while segments
        .last()
        .is_some_and(|segment| segment.target.is_none() && segment.text.trim().is_empty())
    {
        segments.pop();
    }
    segments
}

fn footer_hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<GoalWorkspaceHitTarget> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !rect_contains(inner, mouse) || mouse.y != inner.y {
        return None;
    }

    let mut x = inner.x;
    for segment in footer_segments(tasks, goal_run_id, state, &ThemeTokens::default()) {
        let width = segment
            .text
            .chars()
            .map(|ch| ch.width().unwrap_or(0) as u16)
            .sum::<u16>();
        if mouse.x >= x && mouse.x < x.saturating_add(width) {
            return segment.target;
        }
        x = x.saturating_add(width);
    }
    None
}

fn detail_pane_title(mode: GoalWorkspaceMode) -> &'static str {
    match mode {
        GoalWorkspaceMode::Work => " Work details ",
        GoalWorkspaceMode::Review => " Review details ",
        GoalWorkspaceMode::Activity => " Activity details ",
        GoalWorkspaceMode::Threads => " Thread details ",
        GoalWorkspaceMode::Files => " File details ",
    }
}

fn center_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> Vec<WorkspaceVisualRow> {
    match state.mode() {
        GoalWorkspaceMode::Work => work_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::Review => review_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::Activity => {
            timeline_rows(tasks, goal_run_id, width, theme, tick_counter)
        }
        GoalWorkspaceMode::Threads => thread_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::Files => goal_file_rows(goal_run_id, width, theme),
    }
}

fn center_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<GoalWorkspaceHitTarget> {
    let mut targets = Vec::new();
    for row in center_rows(tasks, goal_run_id, state, 80, &ThemeTokens::default(), 0) {
        let Some(target) = row.target else {
            continue;
        };
        if targets.last() != Some(&target) {
            targets.push(target);
        }
    }
    targets
}

#[derive(Clone)]
enum ProgressItem {
    DossierSummary,
    ResumeDecision,
    DeliveryUnit(usize),
    Checkpoint(String),
}

#[derive(Clone)]
enum UsageItem {
    Aggregate,
    Model(usize),
    CurrentOwner,
    PlannerOwner,
    Assignment(usize),
    Task(String),
}

#[derive(Clone)]
enum ActiveAgentItem {
    CurrentOwner,
    PlannerOwner,
    Assignment(usize),
    Thread(String),
}

#[derive(Clone)]
enum ThreadItem {
    Entry(String),
}

#[derive(Clone)]
enum AttentionItem {
    Approvals,
    Status,
    LastError,
    ProjectionError,
}

fn latest_worker_todos(tasks: &TaskState, goal_run_id: &str) -> Vec<crate::state::task::TodoItem> {
    let mut todos: Vec<_> = tasks
        .goal_step_todos_index(goal_run_id)
        .into_values()
        .flatten()
        .collect();
    if !todos.is_empty() {
        todos.sort_by_key(|todo| todo.position);
        return todos;
    }
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    for event in run.events.iter().rev() {
        if !event.todo_snapshot.is_empty() {
            return event.todo_snapshot.clone();
        }
    }
    Vec::new()
}

fn work_rows(tasks: &TaskState, goal_run_id: &str, theme: &ThemeTokens) -> Vec<WorkspaceVisualRow> {
    let mut rows = Vec::new();
    let worker_thread = tasks
        .goal_run_by_id(goal_run_id)
        .and_then(|run| run.thread_id.clone());
    for task in tasks.tasks().iter().filter(|task| {
        task.goal_run_id.as_deref() == Some(goal_run_id)
            && (worker_thread
                .as_deref()
                .is_some_and(|thread_id| task.thread_id.as_deref() == Some(thread_id))
                || task.parent_task_id.is_none())
    }) {
        let activity = match task.status {
            Some(crate::state::task::TaskStatus::InProgress) => {
                format!("working {}%", task.progress)
            }
            Some(status) => format!("{status:?}").to_ascii_lowercase(),
            None => "queued".to_string(),
        };
        rows.push(WorkspaceVisualRow {
            target: task
                .thread_id
                .clone()
                .map(GoalWorkspaceHitTarget::ThreadRow),
            line: Line::from(vec![
                Span::styled("[worker] ", theme.fg_dim),
                Span::styled(task.title.clone(), theme.fg_active),
                Span::raw("  "),
                Span::styled(activity, theme.fg_dim),
            ]),
        });
    }
    for todo in latest_worker_todos(tasks, goal_run_id) {
        rows.push(WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::PlanTodo {
                step_id: String::new(),
                todo_id: todo.id.clone(),
            }),
            line: Line::from(vec![
                Span::styled(todo_status_chip(todo.status), theme.fg_dim),
                Span::raw(" "),
                Span::styled(todo.content, theme.fg_active),
            ]),
        });
    }
    if let Some(error) = tasks
        .goal_run_by_id(goal_run_id)
        .and_then(|run| run.last_error.clone())
        .filter(|error| !error.trim().is_empty())
    {
        rows.push(WorkspaceVisualRow {
            target: None,
            line: Line::from(vec![
                Span::styled("[error] ", theme.accent_danger),
                Span::styled(error, theme.accent_danger),
            ]),
        });
    }
    if rows.is_empty() {
        rows.push(WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled(
                "Worker has not recorded progress yet.",
                theme.fg_dim,
            )),
        });
    }
    rows
}

fn review_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No review state available.", theme.fg_dim)),
        }];
    };
    let mut rows = Vec::new();
    if matches!(
        run.status,
        Some(crate::state::task::GoalRunStatus::AwaitingReview)
    ) {
        let report = run
            .pending_review_report
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("The worker asked for supervisor review.");
        for (index, segment) in wrap_plain_text(report, 52).into_iter().enumerate() {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(0)),
                line: if index == 0 {
                    Line::from(vec![
                        Span::styled("[report] ", theme.accent_secondary),
                        Span::styled(segment, theme.fg_active),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(segment, theme.fg_active),
                    ])
                },
            });
        }
    } else {
        rows.push(WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled(
                "No supervisor review is pending. Completeness is decided only by Accept.",
                theme.fg_dim,
            )),
        });
    }
    for (event_index, event) in run.events.iter().rev().enumerate() {
        if !is_review_event(event) {
            continue;
        }
        rows.push(WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
            line: Line::from(Span::styled(event.message.clone(), theme.fg_active)),
        });
        if let Some(details) = event.details.as_deref() {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                line: Line::from(vec![
                    Span::raw("  "),
                    Span::styled(details.to_string(), theme.fg_dim),
                ]),
            });
        }
    }
    rows
}

fn is_review_event(event: &crate::state::task::GoalRunEvent) -> bool {
    let haystack = format!("{} {}", event.phase, event.message).to_ascii_lowercase();
    haystack.contains("review")
        || haystack.contains("accept")
        || haystack.contains("reject")
        || haystack.contains("supervisor")
}

fn timeline_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    width: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No timeline available.", theme.fg_dim)),
        }];
    };
    if run.events.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("Waiting for run events.", theme.fg_dim)),
        }];
    }

    let usable_width = width.saturating_sub(2).max(8);
    let mut rows = Vec::new();
    for (event_index, event) in run.events.iter().rev().enumerate() {
        let (indicator, indicator_style, body_style) =
            timeline_event_visuals(run, event, event_index, theme, tick_counter);
        let label = if event.message.trim().is_empty() {
            "event".to_string()
        } else {
            event.message.clone()
        };
        for (wrapped_index, segment) in wrap_plain_text(&label, usable_width)
            .into_iter()
            .enumerate()
        {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                line: if wrapped_index == 0 {
                    Line::from(vec![
                        Span::styled(format!("{indicator} "), indicator_style),
                        Span::styled(segment, body_style),
                    ])
                } else {
                    Line::from(vec![Span::raw("  "), Span::styled(segment, body_style)])
                },
            });
        }
        if let Some(details) = event.details.as_deref() {
            for segment in wrap_plain_text(details, usable_width.saturating_sub(2).max(8)) {
                rows.push(WorkspaceVisualRow {
                    target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                    line: Line::from(vec![Span::raw("  "), Span::styled(segment, theme.fg_dim)]),
                });
            }
        }
        for todo in &event.todo_snapshot {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                line: Line::from(vec![
                    Span::raw("  "),
                    Span::styled(todo_status_chip(todo.status), theme.fg_dim),
                    Span::raw(" "),
                    Span::styled(todo.content.clone(), body_style),
                ]),
            });
        }
    }
    rows
}

fn progress_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let items = progress_items(tasks, goal_run_id);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No progress data available.", theme.fg_dim)),
        }];
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let line = match item {
                ProgressItem::DossierSummary => Line::from(vec![
                    Span::styled("[dossier] ", theme.fg_dim),
                    Span::styled("Execution Dossier", theme.fg_active),
                ]),
                ProgressItem::ResumeDecision => Line::from(vec![
                    Span::styled("[resume] ", theme.fg_dim),
                    Span::styled("Resume Decision", theme.fg_active),
                ]),
                ProgressItem::DeliveryUnit(unit_index) => {
                    let unit = tasks
                        .goal_run_by_id(goal_run_id)
                        .and_then(|run| run.dossier.as_ref())
                        .and_then(|dossier| dossier.units.get(unit_index));
                    if let Some(unit) = unit {
                        Line::from(vec![
                            Span::styled(format!("[{}] ", unit.status), theme.fg_dim),
                            Span::styled(unit.title.clone(), theme.fg_active),
                        ])
                    } else {
                        Line::from(Span::styled("Missing delivery unit", theme.fg_dim))
                    }
                }
                ProgressItem::Checkpoint(checkpoint_id) => {
                    let checkpoint = tasks
                        .checkpoints_for_goal_run(goal_run_id)
                        .iter()
                        .find(|checkpoint| checkpoint.id == checkpoint_id);
                    if let Some(checkpoint) = checkpoint {
                        Line::from(vec![
                            Span::styled(
                                format!("[{}] ", checkpoint.checkpoint_type),
                                theme.fg_dim,
                            ),
                            Span::styled(
                                checkpoint
                                    .step_index
                                    .map(|idx| format!("step {}", idx + 1))
                                    .unwrap_or_else(|| "goal".to_string()),
                                theme.fg_active,
                            ),
                        ])
                    } else {
                        Line::from(Span::styled("Missing checkpoint", theme.fg_dim))
                    }
                }
            };
            WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
                line,
            }
        })
        .collect()
}

fn usage_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let items = usage_items(tasks, goal_run_id);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No usage data available.", theme.fg_dim)),
        }];
    }
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };

    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let line = match item {
                UsageItem::Aggregate => {
                    let mut spans = vec![
                        Span::styled("Goal total  ", theme.fg_active),
                        Span::styled("prompt ", theme.fg_dim),
                        Span::styled(format_count(run.total_prompt_tokens), theme.fg_active),
                        Span::styled("  completion ", theme.fg_dim),
                        Span::styled(format_count(run.total_completion_tokens), theme.fg_active),
                    ];
                    if let Some(cost) = run.estimated_cost_usd {
                        spans.push(Span::styled("  cost ", theme.fg_dim));
                        spans.push(Span::styled(format_cost(cost), theme.fg_active));
                    }
                    Line::from(spans)
                }
                UsageItem::Model(model_index) => {
                    if let Some(usage) = run.model_usage.get(model_index) {
                        let mut spans = vec![
                            Span::styled(
                                format!("{}/{}", usage.provider, usage.model),
                                theme.fg_active,
                            ),
                            Span::styled("  ", theme.fg_dim),
                            Span::styled(format!("{} req", usage.request_count), theme.fg_dim),
                            Span::styled("  in ", theme.fg_dim),
                            Span::styled(format_count(usage.prompt_tokens), theme.fg_dim),
                            Span::styled("  out ", theme.fg_dim),
                            Span::styled(format_count(usage.completion_tokens), theme.fg_dim),
                        ];
                        if let Some(cost) = usage.estimated_cost_usd {
                            spans.push(Span::styled("  ", theme.fg_dim));
                            spans.push(Span::styled(format_cost(cost), theme.fg_dim));
                        }
                        if let Some(duration_ms) = usage.duration_ms {
                            spans.push(Span::styled("  ", theme.fg_dim));
                            spans.push(Span::styled(format_duration_ms(duration_ms), theme.fg_dim));
                        }
                        Line::from(spans)
                    } else {
                        Line::from(Span::styled("Missing model usage", theme.fg_dim))
                    }
                }
                UsageItem::CurrentOwner => {
                    owner_usage_line("Current", run.current_step_owner_profile.as_ref(), theme)
                }
                UsageItem::PlannerOwner => {
                    owner_usage_line("Planner", run.planner_owner_profile.as_ref(), theme)
                }
                UsageItem::Assignment(assignment_index) => {
                    if let Some(assignment) = runtime_assignments(run).get(assignment_index) {
                        let detail = if assignment.inherit_from_main {
                            "inherits main".to_string()
                        } else {
                            format!("{}/{}", assignment.provider, assignment.model)
                        };
                        Line::from(vec![
                            Span::styled("Role ", theme.fg_dim),
                            Span::styled(assignment.role_id.clone(), theme.fg_active),
                            Span::styled("  ", theme.fg_dim),
                            Span::styled(detail, theme.fg_dim),
                        ])
                    } else {
                        Line::from(Span::styled("Missing assignment", theme.fg_dim))
                    }
                }
                UsageItem::Task(task_id) => {
                    if let Some(task) = tasks.task_by_id(&task_id) {
                        let kind = if is_goal_subagent_task(task) {
                            "Subagent"
                        } else {
                            "Task"
                        };
                        Line::from(vec![
                            Span::styled(format!("{kind} "), theme.fg_dim),
                            Span::styled(task.title.clone(), theme.fg_active),
                            Span::styled("  ", theme.fg_dim),
                            Span::styled(task_status_label(task.status), theme.fg_dim),
                        ])
                    } else {
                        Line::from(Span::styled("Missing task", theme.fg_dim))
                    }
                }
            };
            WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
                line,
            }
        })
        .collect()
}

fn active_agent_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let items = active_agent_items(tasks, goal_run_id);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No runtime owner metadata.", theme.fg_dim)),
        }];
    }
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let line = match item {
            ActiveAgentItem::CurrentOwner => Line::from(vec![
                Span::styled("Current ", theme.fg_dim),
                Span::styled(
                    run.current_step_owner_profile
                        .as_ref()
                        .map(|owner| owner.agent_label.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    theme.fg_active,
                ),
            ]),
            ActiveAgentItem::PlannerOwner => Line::from(vec![
                Span::styled("Planner ", theme.fg_dim),
                Span::styled(
                    run.planner_owner_profile
                        .as_ref()
                        .map(|owner| owner.agent_label.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    theme.fg_active,
                ),
            ]),
            ActiveAgentItem::Assignment(assignment_index) => {
                let assignment = runtime_assignments(run).get(assignment_index).cloned();
                if let Some(assignment) = assignment {
                    Line::from(vec![
                        Span::styled(format!("[{}] ", assignment.role_id), theme.fg_dim),
                        Span::styled(assignment.model, theme.fg_active),
                    ])
                } else {
                    Line::from(Span::styled("Missing assignment", theme.fg_dim))
                }
            }
            ActiveAgentItem::Thread(thread_id) => Line::from(vec![
                Span::styled("[thread] ", theme.fg_dim),
                Span::styled(thread_id, theme.fg_active),
            ]),
        };
        rows.push(WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
            line,
        });
    }
    rows
}

fn thread_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let entries = goal_thread_entries(tasks, run);
    if entries.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No linked threads available.", theme.fg_dim)),
        }];
    }
    entries
        .into_iter()
        .map(|entry| WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::ThreadRow(entry.thread_id.clone())),
            line: Line::from(vec![
                Span::styled("[thread] ", theme.fg_dim),
                Span::styled(entry.label, theme.fg_active),
                Span::raw("  "),
                Span::styled(entry.thread_id, theme.accent_primary),
            ]),
        })
        .collect()
}

fn attention_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let items = attention_items(run);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No blockers or review items.", theme.fg_dim)),
        }];
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let line = match item {
                AttentionItem::Approvals => Line::from(vec![
                    Span::styled("Approvals ", theme.fg_dim),
                    Span::styled(run.approval_count.to_string(), theme.fg_active),
                ]),
                AttentionItem::Status => Line::from(vec![
                    Span::styled("Status ", theme.fg_dim),
                    Span::styled(
                        format!("{:?}", run.status).to_ascii_lowercase(),
                        theme.fg_active,
                    ),
                ]),
                AttentionItem::LastError => Line::from(vec![
                    Span::styled("Last error ", theme.fg_dim),
                    Span::styled("available", theme.accent_danger),
                ]),
                AttentionItem::ProjectionError => Line::from(vec![
                    Span::styled("Projection error ", theme.fg_dim),
                    Span::styled("available", theme.accent_danger),
                ]),
            };
            WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
                line,
            }
        })
        .collect()
}

pub fn detail_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, GoalWorkspaceHitTarget)> {
    detail_lines(tasks, goal_run_id, state, 80, &ThemeTokens::default())
        .into_iter()
        .filter_map(|(_, target, _)| target)
        .enumerate()
        .collect()
}

fn selected_goal_step(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<crate::state::task::GoalRunStep> {
    state
        .selected_plan_item()
        .and_then(|selection| match selection {
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id }
            | crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, .. } => {
                Some(step_id.as_str())
            }
            crate::state::goal_workspace::GoalPlanSelection::PromptToggle
            | crate::state::goal_workspace::GoalPlanSelection::MainThread { .. } => None,
        })
        .and_then(|step_id| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .find(|step| step.id == step_id)
                .cloned()
        })
        .or_else(|| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .next()
                .cloned()
        })
}

fn detail_lines(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
    theme: &ThemeTokens,
) -> Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)> {
    let mut rows = Vec::new();
    let mut visual_row = 0usize;
    let run = tasks.goal_run_by_id(goal_run_id);
    match state.mode() {
        GoalWorkspaceMode::Work => {
            push_detail_header(&mut rows, &mut visual_row, "Worker progress", theme);
            let work = work_rows(tasks, goal_run_id, theme);
            if let Some(row) = work.get(state.selected_timeline_row()) {
                push_detail_line(&mut rows, &mut visual_row, None, row.line.clone());
            } else {
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled("No worker progress selected.", theme.fg_dim)),
                );
            }
            if let Some(error) = run.and_then(|run| run.last_error.as_deref()) {
                push_detail_wrapped(
                    &mut rows,
                    &mut visual_row,
                    error,
                    theme.accent_danger,
                    width,
                );
            }
        }
        GoalWorkspaceMode::Review => {
            push_detail_header(&mut rows, &mut visual_row, "Supervisor review", theme);
            if let Some(run) = run {
                if matches!(
                    run.status,
                    Some(crate::state::task::GoalRunStatus::AwaitingReview)
                ) {
                    let report = run
                        .pending_review_report
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("The worker asked for supervisor review.");
                    push_detail_wrapped(&mut rows, &mut visual_row, report, theme.fg_active, width);
                } else {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(Span::styled(
                            "No supervisor review is pending.",
                            theme.fg_dim,
                        )),
                    );
                }
            }
        }
        GoalWorkspaceMode::Activity => {
            if let Some((selected_event_index, event)) = selected_event(tasks, goal_run_id, state) {
                push_detail_header(&mut rows, &mut visual_row, "Selected activity", theme);
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled(event.message.clone(), theme.fg_active)),
                );
                if let Some(details) = event.details.as_deref() {
                    push_detail_wrapped(&mut rows, &mut visual_row, details, theme.fg_dim, width);
                }
                for todo in &event.todo_snapshot {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(vec![
                            Span::styled(todo_status_chip(todo.status), theme.fg_dim),
                            Span::raw(" "),
                            Span::styled(todo.content.clone(), theme.fg_active),
                        ]),
                    );
                }
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    Some(GoalWorkspaceHitTarget::DetailTimelineDetails(
                        selected_event_index,
                    )),
                    Line::from(vec![
                        Span::styled("[details]", theme.accent_primary),
                        Span::raw("  "),
                        Span::styled("show activity item context", theme.fg_dim),
                    ]),
                );
            } else {
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled("Waiting for run events.", theme.fg_dim)),
                );
            }
        }
        GoalWorkspaceMode::Files => {
            push_detail_header(&mut rows, &mut visual_row, "Selected File", theme);
            if let Some(file) = selected_goal_projection_file(goal_run_id, state) {
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(vec![
                        Span::styled("Path ", theme.fg_dim),
                        Span::styled(file.relative_path.clone(), theme.fg_active),
                    ]),
                );
                push_detail_wrapped(
                    &mut rows,
                    &mut visual_row,
                    &file.absolute_path,
                    theme.fg_dim,
                    width,
                );
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(vec![
                        Span::styled("Size ", theme.fg_dim),
                        Span::styled(
                            format!("{} bytes", file.size_bytes.unwrap_or(0)),
                            theme.fg_active,
                        ),
                    ]),
                );
                push_detail_blank(&mut rows, &mut visual_row);
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled(
                        "Press Enter to open the preview.",
                        theme.fg_dim,
                    )),
                );
            } else {
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled("No goal files yet.", theme.fg_dim)),
                );
            }
        }
        GoalWorkspaceMode::Threads => {
            if let Some(run) = run {
                let items = thread_items(tasks, run);
                if let Some(ThreadItem::Entry(thread_id)) = items.get(state.selected_timeline_row())
                {
                    push_detail_header(&mut rows, &mut visual_row, "Thread", theme);
                    if let Some(entry) = goal_thread_entries(tasks, run)
                        .into_iter()
                        .find(|entry| &entry.thread_id == thread_id)
                    {
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            None,
                            Line::from(vec![
                                Span::styled(entry.label, theme.fg_active),
                                Span::raw("  "),
                                Span::styled(entry.thread_id.clone(), theme.fg_dim),
                            ]),
                        );
                        push_detail_wrapped(
                            &mut rows,
                            &mut visual_row,
                            &entry.summary,
                            theme.fg_dim,
                            width,
                        );
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailThread(
                                entry.thread_id.clone(),
                            )),
                            Line::from(vec![
                                Span::styled("[open] ", theme.accent_primary),
                                Span::styled(entry.thread_id, theme.fg_active),
                            ]),
                        );
                    }
                }
            }
        }
    }
    rows
}

fn progress_items(tasks: &TaskState, goal_run_id: &str) -> Vec<ProgressItem> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for checkpoint in tasks.checkpoints_for_goal_run(goal_run_id) {
        items.push(ProgressItem::Checkpoint(checkpoint.id.clone()));
    }
    if run.dossier.is_some() {
        items.push(ProgressItem::DossierSummary);
    }
    if run
        .dossier
        .as_ref()
        .and_then(|dossier| dossier.latest_resume_decision.as_ref())
        .is_some()
    {
        items.push(ProgressItem::ResumeDecision);
    }
    if let Some(dossier) = run.dossier.as_ref() {
        for index in 0..dossier.units.len() {
            items.push(ProgressItem::DeliveryUnit(index));
        }
    }
    items
}

fn usage_items(tasks: &TaskState, goal_run_id: &str) -> Vec<UsageItem> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if run.total_prompt_tokens > 0
        || run.total_completion_tokens > 0
        || run.estimated_cost_usd.is_some()
    {
        items.push(UsageItem::Aggregate);
    }
    for index in 0..run.model_usage.len() {
        items.push(UsageItem::Model(index));
    }
    if run.current_step_owner_profile.is_some() {
        items.push(UsageItem::CurrentOwner);
    }
    if run.planner_owner_profile.is_some() {
        items.push(UsageItem::PlannerOwner);
    }
    for index in 0..runtime_assignments(run).len() {
        items.push(UsageItem::Assignment(index));
    }
    for task in tasks
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
    {
        items.push(UsageItem::Task(task.id.clone()));
    }
    items
}

fn active_agent_items(tasks: &TaskState, goal_run_id: &str) -> Vec<ActiveAgentItem> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if run.current_step_owner_profile.is_some() {
        items.push(ActiveAgentItem::CurrentOwner);
    }
    if run.planner_owner_profile.is_some() {
        items.push(ActiveAgentItem::PlannerOwner);
    }
    for index in 0..runtime_assignments(run).len() {
        items.push(ActiveAgentItem::Assignment(index));
    }
    for thread_id in goal_thread_targets(tasks, run) {
        items.push(ActiveAgentItem::Thread(thread_id));
    }
    items
}

fn thread_items(tasks: &TaskState, run: &crate::state::task::GoalRun) -> Vec<ThreadItem> {
    goal_thread_entries(tasks, run)
        .into_iter()
        .map(|entry| ThreadItem::Entry(entry.thread_id))
        .collect()
}

fn attention_items(run: &crate::state::task::GoalRun) -> Vec<AttentionItem> {
    let mut items = Vec::new();
    if run.last_error.is_some() {
        items.push(AttentionItem::LastError);
    }
    if run
        .dossier
        .as_ref()
        .and_then(|dossier| dossier.projection_error.as_deref())
        .is_some()
    {
        items.push(AttentionItem::ProjectionError);
    }
    items.push(AttentionItem::Approvals);
    items.push(AttentionItem::Status);
    items
}

fn runtime_assignments(
    run: &crate::state::task::GoalRun,
) -> &[crate::state::task::GoalAgentAssignment] {
    if !run.runtime_assignment_list.is_empty() {
        &run.runtime_assignment_list
    } else {
        &run.launch_assignment_snapshot
    }
}

fn selected_event<'a>(
    tasks: &'a TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<(usize, &'a crate::state::task::GoalRunEvent)> {
    let event_index = state.selected_timeline_row();
    let run = tasks.goal_run_by_id(goal_run_id)?;
    run.events
        .iter()
        .rev()
        .nth(event_index)
        .map(|event| (event_index, event))
}

fn related_tasks_for_step<'a>(
    tasks: &'a TaskState,
    run: &crate::state::task::GoalRun,
    step: &crate::state::task::GoalRunStep,
) -> Vec<&'a crate::state::task::AgentTask> {
    let related: Vec<_> = tasks
        .tasks()
        .iter()
        .filter(|task| {
            task.goal_run_id.as_deref() == Some(run.id.as_str())
                && (task.goal_step_title.as_deref() == Some(step.title.as_str())
                    || step
                        .task_id
                        .as_deref()
                        .is_some_and(|task_id| task.id == task_id))
        })
        .collect();
    if !related.is_empty() {
        return related;
    }
    if !run.child_task_ids.is_empty() {
        return run
            .child_task_ids
            .iter()
            .filter_map(|task_id| tasks.task_by_id(task_id))
            .collect();
    }
    tasks
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
        .collect()
}

fn goal_thread_targets(tasks: &TaskState, run: &crate::state::task::GoalRun) -> Vec<String> {
    tasks.goal_thread_ids(&run.id)
}

#[derive(Clone)]
pub(crate) struct GoalThreadEntry {
    label: String,
    thread_id: String,
    summary: String,
}

pub(crate) fn goal_thread_entries(
    tasks: &TaskState,
    run: &crate::state::task::GoalRun,
) -> Vec<GoalThreadEntry> {
    let mut entries = Vec::new();
    let mut known = std::collections::BTreeSet::new();
    let push_entry = |entries: &mut Vec<GoalThreadEntry>,
                      known: &mut std::collections::BTreeSet<String>,
                      label: String,
                      thread_id: String,
                      summary: String| {
        if thread_id.is_empty() || !known.insert(thread_id.clone()) {
            return;
        }
        if !entries
            .iter()
            .any(|entry: &GoalThreadEntry| entry.thread_id == thread_id)
        {
            entries.push(GoalThreadEntry {
                label,
                thread_id,
                summary,
            });
        }
    };

    let worker_thread_id = run
        .thread_id
        .clone()
        .or_else(|| run.active_thread_id.clone())
        .or_else(|| run.execution_thread_ids.first().cloned())
        .or_else(|| fallback_worker_thread_id(tasks, run));
    if let Some(thread_id) = worker_thread_id.clone() {
        push_entry(
            &mut entries,
            &mut known,
            "Worker".to_string(),
            thread_id,
            "Sole goal worker thread.".to_string(),
        );
    }

    let worker_thread = worker_thread_id.as_deref();
    if let Some(thread_id) = tasks
        .tasks()
        .iter()
        .find(|task| {
            task.goal_run_id.as_deref() == Some(run.id.as_str())
                && task.thread_id.as_deref() == worker_thread
        })
        .and_then(|task| task.parent_thread_id.clone())
        .filter(|thread_id| worker_thread != Some(thread_id.as_str()))
    {
        push_entry(
            &mut entries,
            &mut known,
            "Owner".to_string(),
            thread_id,
            "Owner supervisor thread.".to_string(),
        );
    }

    for thread_id in [run.active_thread_id.clone(), run.root_thread_id.clone()]
        .into_iter()
        .flatten()
        .chain(run.execution_thread_ids.iter().cloned())
    {
        known.insert(thread_id);
    }

    let mut grew = true;
    while grew {
        grew = false;
        for task in tasks.tasks() {
            let Some(thread_id) = task.thread_id.as_deref() else {
                continue;
            };
            if known.contains(thread_id) {
                continue;
            }
            let belongs = task.goal_run_id.as_deref() == Some(run.id.as_str())
                || task
                    .parent_thread_id
                    .as_deref()
                    .is_some_and(|parent| known.contains(parent));
            if !belongs {
                continue;
            }
            push_entry(
                &mut entries,
                &mut known,
                task.title.clone(),
                thread_id.to_string(),
                "Spawned helper thread.".to_string(),
            );
            grew = true;
        }
    }
    entries
}

fn fallback_worker_thread_id(
    tasks: &TaskState,
    run: &crate::state::task::GoalRun,
) -> Option<String> {
    let mut goal_tasks = tasks
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
        .filter_map(|task| {
            task.thread_id.as_ref().map(|thread_id| {
                let priority = match task.status {
                    Some(crate::state::task::TaskStatus::InProgress) => 0,
                    Some(crate::state::task::TaskStatus::AwaitingApproval) => 1,
                    Some(crate::state::task::TaskStatus::Queued) => 2,
                    _ => 3,
                };
                (
                    priority,
                    std::cmp::Reverse(task.created_at),
                    thread_id.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    goal_tasks.sort();
    goal_tasks
        .into_iter()
        .next()
        .map(|(_, _, thread_id)| thread_id)
}

fn push_detail_header(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    label: &str,
    theme: &ThemeTokens,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(Span::styled(label.to_string(), theme.accent_secondary)),
    );
}

fn push_detail_blank(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
) {
    push_detail_line(rows, visual_row, None, Line::from(Span::raw("")));
}

fn push_detail_line(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    target: Option<GoalWorkspaceHitTarget>,
    line: Line<'static>,
) {
    rows.push((*visual_row, target, line));
    *visual_row = visual_row.saturating_add(1);
}

fn push_detail_wrapped(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    text: &str,
    style: Style,
    width: usize,
) {
    for line in wrap_plain_text(text, width.max(8)) {
        push_detail_line(
            rows,
            visual_row,
            None,
            Line::from(Span::styled(line, style)),
        );
    }
}

fn push_owner_profile(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    owner: &crate::state::task::GoalRuntimeOwnerProfile,
    theme: &ThemeTokens,
    width: usize,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(Span::styled(owner.agent_label.clone(), theme.fg_active)),
    );
    push_detail_wrapped(
        rows,
        visual_row,
        &format!(
            "{} / {} / {}",
            owner.provider,
            owner.model,
            owner
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "default".to_string())
        ),
        theme.fg_dim,
        width,
    );
}

fn push_assignment(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    assignment: &crate::state::task::GoalAgentAssignment,
    theme: &ThemeTokens,
    width: usize,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(vec![
            Span::styled(assignment.role_id.clone(), theme.fg_active),
            Span::raw(" "),
            Span::styled(
                if assignment.enabled {
                    "[enabled]"
                } else {
                    "[disabled]"
                },
                theme.fg_dim,
            ),
        ]),
    );
    push_detail_wrapped(
        rows,
        visual_row,
        &format!(
            "{} / {} / {}",
            assignment.provider,
            assignment.model,
            assignment
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "default".to_string())
        ),
        theme.fg_dim,
        width,
    );
}

fn goal_toggle_action_label(
    run: &crate::state::task::GoalRun,
    theme: &ThemeTokens,
) -> Option<(String, Style)> {
    match run.status {
        Some(crate::state::task::GoalRunStatus::Paused) => {
            Some(("[Resume]".to_string(), theme.accent_success))
        }
        Some(crate::state::task::GoalRunStatus::Queued)
        | Some(crate::state::task::GoalRunStatus::Planning)
        | Some(crate::state::task::GoalRunStatus::Running)
        | Some(crate::state::task::GoalRunStatus::AwaitingApproval)
        | Some(crate::state::task::GoalRunStatus::AwaitingReview) => {
            Some(("[Pause]".to_string(), theme.accent_secondary))
        }
        _ => None,
    }
}

fn goal_status_label(status: Option<crate::state::task::GoalRunStatus>) -> &'static str {
    match status {
        Some(crate::state::task::GoalRunStatus::Queued) => "queued",
        Some(crate::state::task::GoalRunStatus::Planning) => "planning",
        Some(crate::state::task::GoalRunStatus::Running) => "running",
        Some(crate::state::task::GoalRunStatus::AwaitingApproval) => "awaiting approval",
        Some(crate::state::task::GoalRunStatus::AwaitingReview) => "awaiting review",
        Some(crate::state::task::GoalRunStatus::Paused) => "paused",
        Some(crate::state::task::GoalRunStatus::Blocked) => "blocked",
        Some(crate::state::task::GoalRunStatus::Contained) => "contained",
        Some(crate::state::task::GoalRunStatus::Compensated) => "compensated",
        Some(crate::state::task::GoalRunStatus::PartiallyCompensated) => "partially compensated",
        Some(crate::state::task::GoalRunStatus::BreakGlass) => "break-glass",
        Some(crate::state::task::GoalRunStatus::Completed) => "completed",
        Some(crate::state::task::GoalRunStatus::Failed) => "failed",
        Some(crate::state::task::GoalRunStatus::Cancelled) => "cancelled",
        None => "queued",
    }
}

fn task_status_label(status: Option<crate::state::task::TaskStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TaskStatus::InProgress) => "running",
        Some(crate::state::task::TaskStatus::Completed) => "completed",
        Some(crate::state::task::TaskStatus::AwaitingApproval) => "awaiting approval",
        Some(crate::state::task::TaskStatus::Blocked) => "blocked",
        Some(crate::state::task::TaskStatus::Failed)
        | Some(crate::state::task::TaskStatus::FailedAnalyzing) => "failed",
        Some(crate::state::task::TaskStatus::BudgetExceeded) => "budget exceeded",
        Some(crate::state::task::TaskStatus::Cancelled) => "cancelled",
        _ => "queued",
    }
}

fn format_count(value: u64) -> String {
    let raw = value.to_string();
    let mut grouped = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.chars().rev().collect()
}

fn format_cost(cost: f64) -> String {
    if cost.abs() >= 1.0 {
        format!("${cost:.2}")
    } else {
        format!("${cost:.4}")
    }
}

fn owner_usage_line(
    label: &str,
    owner: Option<&crate::state::task::GoalRuntimeOwnerProfile>,
    theme: &ThemeTokens,
) -> Line<'static> {
    if let Some(owner) = owner {
        Line::from(vec![
            Span::styled(format!("{label} "), theme.fg_dim),
            Span::styled(owner.agent_label.clone(), theme.fg_active),
            Span::styled("  ", theme.fg_dim),
            Span::styled(format!("{}/{}", owner.provider, owner.model), theme.fg_dim),
        ])
    } else {
        Line::from(Span::styled(format!("{label} unknown"), theme.fg_dim))
    }
}

fn is_goal_subagent_task(task: &crate::state::task::AgentTask) -> bool {
    task.parent_task_id.is_some() || task.parent_thread_id.is_some()
}

fn short_checkpoint_id(id: &str) -> String {
    if id.chars().count() <= 18 {
        return id.to_string();
    }
    let tail: String = id
        .chars()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

fn todo_status_chip(status: Option<crate::state::task::TodoStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TodoStatus::InProgress) => "[~]",
        Some(crate::state::task::TodoStatus::Completed) => "[x]",
        Some(crate::state::task::TodoStatus::Blocked) => "[!]",
        _ => "[ ]",
    }
}

fn todo_task_chip(status: Option<crate::state::task::TaskStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TaskStatus::InProgress) => "[~]",
        Some(crate::state::task::TaskStatus::Completed) => "[x]",
        Some(crate::state::task::TaskStatus::Blocked)
        | Some(crate::state::task::TaskStatus::Failed)
        | Some(crate::state::task::TaskStatus::FailedAnalyzing)
        | Some(crate::state::task::TaskStatus::BudgetExceeded) => "[!]",
        _ => "[ ]",
    }
}

#[derive(Clone)]
struct GoalProjectionFileEntry {
    absolute_path: String,
    relative_path: String,
    size_bytes: Option<u64>,
}

fn goal_file_rows(goal_run_id: &str, width: usize, theme: &ThemeTokens) -> Vec<WorkspaceVisualRow> {
    let files = goal_projection_files(goal_run_id);
    if files.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No goal files yet.", theme.fg_dim)),
        }];
    }

    files
        .into_iter()
        .map(|file| WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::DetailFile(
                file.absolute_path.clone(),
            )),
            line: Line::from(vec![
                Span::styled("• ", theme.accent_secondary),
                Span::styled(
                    truncate_tail(&file.relative_path, width.saturating_sub(2).max(8)),
                    theme.fg_active,
                ),
            ]),
        })
        .collect()
}

fn selected_goal_projection_file(
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<GoalProjectionFileEntry> {
    let files = goal_projection_files(goal_run_id);
    if files.is_empty() {
        None
    } else {
        files
            .get(
                state
                    .selected_timeline_row()
                    .min(files.len().saturating_sub(1)),
            )
            .cloned()
    }
}

thread_local! {
    static GOAL_PROJECTION_FILES_CACHE: std::cell::RefCell<
        std::collections::HashMap<String, (std::time::Instant, Vec<GoalProjectionFileEntry>)>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

const GOAL_PROJECTION_FILES_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(1);

fn goal_projection_files(goal_run_id: &str) -> Vec<GoalProjectionFileEntry> {
    if let Some(files) = GOAL_PROJECTION_FILES_CACHE.with(|cache| {
        cache
            .borrow()
            .get(goal_run_id)
            .filter(|(loaded_at, _)| loaded_at.elapsed() < GOAL_PROJECTION_FILES_CACHE_TTL)
            .map(|(_, files)| files.clone())
    }) {
        return files;
    }
    let Ok(data_dir) = zorai_protocol::ensure_zorai_data_dir() else {
        return Vec::new();
    };
    let files = goal_projection_files_in_root(&data_dir.join("goals").join(goal_run_id));
    GOAL_PROJECTION_FILES_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            goal_run_id.to_string(),
            (std::time::Instant::now(), files.clone()),
        );
    });
    files
}

fn goal_projection_files_in_root(goal_root: &Path) -> Vec<GoalProjectionFileEntry> {
    let mut absolute_paths = Vec::new();
    collect_goal_projection_files(goal_root, &mut absolute_paths);
    absolute_paths.sort();
    absolute_paths
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(goal_root).ok()?;
            let metadata = std::fs::metadata(&path).ok();
            Some(GoalProjectionFileEntry {
                absolute_path: path.to_string_lossy().to_string(),
                relative_path: normalized_goal_relative_path(relative),
                size_bytes: metadata.map(|value| value.len()),
            })
        })
        .collect()
}

fn collect_goal_projection_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_goal_projection_files(&path, files);
        } else if file_type.is_file() {
            files.push(path);
        }
    }
}

fn normalized_goal_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn truncate_tail(text: &str, max_len: usize) -> String {
    crate::widgets::message::truncate_tail_to_width(text, max_len)
}

fn goal_files_for_selected_step<'a>(
    tasks: &'a TaskState,
    goal_run_id: &str,
    step_index: usize,
) -> Vec<&'a crate::state::task::WorkContextEntry> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let Some(thread_id) = run.thread_id.as_deref() else {
        return Vec::new();
    };

    let step_files = tasks.goal_step_files(goal_run_id, thread_id, step_index);
    if !step_files.is_empty() {
        return step_files;
    }

    let Some(context) = tasks.work_context_for_thread(thread_id) else {
        return Vec::new();
    };
    context
        .entries
        .iter()
        .filter(|entry| match entry.goal_run_id.as_deref() {
            Some(entry_goal_run_id) => entry_goal_run_id == goal_run_id,
            None => true,
        })
        .collect()
}

fn rect_contains(area: Rect, mouse: Position) -> bool {
    mouse.x >= area.x
        && mouse.x < area.x.saturating_add(area.width)
        && mouse.y >= area.y
        && mouse.y < area.y.saturating_add(area.height)
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut wrapped = Vec::new();
    for raw_line in text.lines() {
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if candidate.chars().count() > width && !current.is_empty() {
                wrapped.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        if current.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.push(current);
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

fn timeline_event_visuals(
    run: &crate::state::task::GoalRun,
    event: &crate::state::task::GoalRunEvent,
    event_index: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> (char, Style, Style) {
    let is_current_step_event = event
        .step_index
        .is_some_and(|index| index == run.current_step_index);
    let is_latest_event = event_index == 0;
    let is_live_row = matches!(
        run.status,
        Some(
            crate::state::task::GoalRunStatus::Planning
                | crate::state::task::GoalRunStatus::Running
        )
    ) && (is_current_step_event || is_latest_event);

    if is_live_row {
        return (
            spinner_frame(tick_counter),
            theme.accent_primary,
            theme.fg_active,
        );
    }

    match run.status {
        Some(crate::state::task::GoalRunStatus::AwaitingApproval)
        | Some(crate::state::task::GoalRunStatus::AwaitingReview)
        | Some(crate::state::task::GoalRunStatus::Paused)
            if is_latest_event =>
        {
            ('‖', theme.accent_secondary, theme.fg_active)
        }
        Some(crate::state::task::GoalRunStatus::Completed) if is_latest_event => {
            ('✓', theme.accent_success, theme.fg_active)
        }
        Some(crate::state::task::GoalRunStatus::Failed)
        | Some(crate::state::task::GoalRunStatus::Cancelled)
            if is_latest_event =>
        {
            ('✕', theme.accent_danger, theme.fg_active)
        }
        _ => ('•', theme.accent_secondary, theme.fg_active),
    }
}

fn spinner_frame(tick_counter: u64) -> char {
    match tick_counter % 4 {
        0 => '⠋',
        1 => '⠙',
        2 => '⠹',
        _ => '⠸',
    }
}

fn selected_row_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(Color::Indexed(236))
    } else {
        Style::default()
    }
}

fn resolved_plan_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.plan_scroll())
}

fn resolved_timeline_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.timeline_scroll())
}

fn resolved_detail_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.detail_scroll())
}

pub fn timeline_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Option<usize> {
    let selected_target = center_targets(tasks, goal_run_id, state)
        .get(state.selected_timeline_row())
        .cloned()?;
    center_visual_targets(tasks, goal_run_id, state, width)
        .iter()
        .position(|target| *target == Some(selected_target.clone()))
}

pub fn timeline_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, GoalWorkspaceHitTarget)> {
    center_targets(tasks, goal_run_id, state)
        .into_iter()
        .enumerate()
        .collect()
}

pub fn plan_selection_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, crate::state::goal_workspace::GoalPlanSelection)> {
    plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default())
        .into_iter()
        .enumerate()
        .filter_map(|(index, row)| row.selection.map(|selection| (index, selection)))
        .collect()
}

pub fn plan_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Option<usize> {
    let selected = state.selected_plan_item().cloned();
    let rows = plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default());
    let mut visual_row = 0usize;
    let mut selection_index = 0usize;

    for row in rows {
        let row_height = wrapped_visual_height(&row.line, width);
        if selected.is_some() && row.selection == selected {
            return Some(visual_row);
        }
        if row.selection.is_some() {
            if selected.is_none() && selection_index == state.selected_plan_row() {
                return Some(visual_row);
            }
            selection_index = selection_index.saturating_add(1);
        }
        visual_row = visual_row.saturating_add(row_height);
    }

    None
}

pub fn detail_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Option<usize> {
    let selected_target = detail_targets(tasks, goal_run_id, state)
        .into_iter()
        .find_map(|(index, target)| (index == state.selected_detail_row()).then_some(target))?;
    detail_visual_targets(tasks, goal_run_id, state, width)
        .iter()
        .position(|target| *target == Some(selected_target.clone()))
}

fn plan_inner_row_index(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<(Rect, usize, usize)> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let plan_area = layout.plan;
    if mouse.x < plan_area.x
        || mouse.x >= plan_area.x.saturating_add(plan_area.width)
        || mouse.y < plan_area.y
        || mouse.y >= plan_area.y.saturating_add(plan_area.height)
    {
        return None;
    }

    let inner = Block::default().borders(Borders::ALL).inner(plan_area);
    if mouse.x < inner.x
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y < inner.y
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let rows = plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default());
    let visual_targets = plan_visual_row_targets(tasks, goal_run_id, state, inner.width as usize);
    let visual_row = resolved_plan_scroll(visual_targets.len(), inner.height as usize, state)
        .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
    let (row_index, wrapped_row_index) =
        plan_row_for_visual_row(&rows, inner.width as usize, visual_row)?;
    Some((inner, row_index, wrapped_row_index))
}

fn line_plain_text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn line_display_width(line: &Line<'static>) -> usize {
    display_width(&line_plain_text(line))
}

fn display_width(text: &str) -> usize {
    text.chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn display_slice(text: &str, start_col: usize, end_col: usize) -> String {
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

fn highlight_line_range(
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

fn wrap_display_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut wrapped = Vec::new();

    for raw_line in text.lines() {
        if raw_line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in raw_line.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if current_width.saturating_add(ch_width) > width && !current.is_empty() {
                wrapped.push(current);
                current = String::new();
                current_width = 0;
            }
            current.push(ch);
            current_width = current_width.saturating_add(ch_width);
        }

        if current.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.push(current);
        }
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn wrapped_visual_height(line: &Line<'static>, width: usize) -> usize {
    wrap_display_text(&line_plain_text(line), width).len()
}

fn wrapped_segment_display_bounds(
    line: &Line<'static>,
    width: usize,
    wrapped_row_index: usize,
) -> Option<(usize, usize)> {
    let wrapped = wrap_display_text(&line_plain_text(line), width);
    let mut start = 0usize;
    for (index, segment) in wrapped.iter().enumerate() {
        let segment_width = display_width(segment);
        let end = start.saturating_add(segment_width);
        if index == wrapped_row_index {
            return Some((start, end));
        }
        start = end;
    }
    None
}

fn plan_row_for_visual_row(
    rows: &[plan::GoalWorkspacePlanRow],
    width: usize,
    visual_row: usize,
) -> Option<(usize, usize)> {
    let mut current_visual_row = 0usize;
    for (row_index, row) in rows.iter().enumerate() {
        let row_height = wrapped_visual_height(&row.line, width);
        if visual_row < current_visual_row.saturating_add(row_height) {
            return Some((row_index, visual_row.saturating_sub(current_visual_row)));
        }
        current_visual_row = current_visual_row.saturating_add(row_height);
    }
    None
}

fn plan_visual_row_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Vec<Option<GoalWorkspaceHitTarget>> {
    let mut rows = Vec::new();
    for row in plan::build_rows(tasks, goal_run_id, state, &ThemeTokens::default()) {
        let height = wrapped_visual_height(&row.line, width);
        for _ in 0..height {
            rows.push(row.target.clone());
        }
    }
    rows
}

fn distinct_workspace_targets(rows: &[WorkspaceVisualRow]) -> Vec<GoalWorkspaceHitTarget> {
    let mut targets = Vec::new();
    for row in rows {
        let Some(target) = row.target.as_ref() else {
            continue;
        };
        if targets.last() != Some(target) {
            targets.push(target.clone());
        }
    }
    targets
}

fn workspace_visual_targets(
    visual_rows: &[WorkspaceVisualRow],
    width: usize,
) -> Vec<Option<GoalWorkspaceHitTarget>> {
    let mut rows = Vec::new();
    for row in visual_rows {
        let height = wrapped_visual_height(&row.line, width);
        rows.extend(std::iter::repeat(row.target.clone()).take(height));
    }
    rows
}

fn detail_visual_targets_from_lines(
    detail_rows: &[(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)],
    width: usize,
) -> Vec<Option<GoalWorkspaceHitTarget>> {
    let mut rows = Vec::new();
    for (_, target, line) in detail_rows {
        let height = wrapped_visual_height(line, width);
        rows.extend(std::iter::repeat(target.clone()).take(height));
    }
    rows
}

fn center_visual_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Vec<Option<GoalWorkspaceHitTarget>> {
    let visual_rows = center_rows(tasks, goal_run_id, state, width, &ThemeTokens::default(), 0);
    workspace_visual_targets(&visual_rows, width)
}

fn detail_visual_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
) -> Vec<Option<GoalWorkspaceHitTarget>> {
    let detail_rows = detail_lines(tasks, goal_run_id, state, width, &ThemeTokens::default());
    detail_visual_targets_from_lines(&detail_rows, width)
}

#[cfg(test)]
#[path = "tests/goal_workspace.rs"]
mod tests;
