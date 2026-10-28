use super::from_tasks_to_is_weles_thread::*;
use super::hit_test_for_workspace_to_now_millis::*;
use crate::state::chat::{AgentThread, ChatState};
use crate::state::modal::{ModalState, ThreadPickerTab, ThreadSearchState};
use crate::state::subagents::SubAgentsState;
use crate::state::task::TaskState;
use crate::state::workspace::WorkspaceState;
use crate::theme::ThemeTokens;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};
use zorai_protocol::AGENT_NAME_RAROG;

pub(super) fn is_svarog_agent_name(agent_name: &str) -> bool {
    matches!(
        agent_name.trim().to_ascii_lowercase().as_str(),
        "svarog" | "swarog" | "main"
    )
}

pub(super) fn is_svarog_thread(thread: &AgentThread) -> bool {
    thread
        .agent_name
        .as_deref()
        .is_some_and(is_svarog_agent_name)
}

#[cfg(test)]
pub(crate) fn thread_display_title(thread: &AgentThread) -> String {
    thread_display_title_inner(thread, None, None)
}

#[cfg(test)]
pub(crate) fn thread_display_title_for_tasks(thread: &AgentThread, tasks: &TaskState) -> String {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    thread_display_title_inner(thread, Some(&goal_index), None)
}

pub(crate) fn thread_display_title_for_workspace(
    thread: &AgentThread,
    tasks: &TaskState,
    workspace: &WorkspaceState,
) -> String {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    thread_display_title_inner(thread, Some(&goal_index), Some(&workspace_index))
}

pub(super) fn thread_display_title_inner(
    thread: &AgentThread,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> String {
    if thread.id == "concierge" || thread.title.eq_ignore_ascii_case("concierge") {
        crate::agent_aliases::display_agent_name(AGENT_NAME_RAROG)
    } else if is_workspace_thread_with_index(thread, workspace_index) {
        let role = thread
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(crate::agent_aliases::display_agent_name)
            .unwrap_or_else(|| "Workspace".to_string());
        format!("workspace: {role} · {}", thread.title)
    } else if is_goal_thread_with_index(thread, goal_index) {
        let role = thread
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(crate::agent_aliases::display_agent_name)
            .unwrap_or_else(|| "Goal".to_string());
        format!("goal: {role} · {}", thread.title)
    } else {
        thread.title.clone()
    }
}

#[cfg(test)]
pub(crate) fn filtered_threads<'a>(
    chat: &'a ChatState,
    modal: &'a ModalState,
    subagents: &SubAgentsState,
) -> Vec<&'a AgentThread> {
    filtered_threads_inner(chat, modal, subagents, None, None)
}

#[cfg(test)]
pub(crate) fn filtered_threads_for_tasks<'a>(
    chat: &'a ChatState,
    modal: &'a ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
) -> Vec<&'a AgentThread> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    filtered_threads_inner(chat, modal, subagents, Some(&goal_index), None)
}

pub(crate) fn filtered_threads_for_workspace<'a>(
    chat: &'a ChatState,
    modal: &'a ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    workspace: &WorkspaceState,
) -> Vec<&'a AgentThread> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    filtered_threads_inner(
        chat,
        modal,
        subagents,
        Some(&goal_index),
        Some(&workspace_index),
    )
}

pub(super) fn filtered_threads_inner<'a>(
    chat: &'a ChatState,
    modal: &'a ModalState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> Vec<&'a AgentThread> {
    if let Some(search) = modal.thread_search() {
        return search
            .hits
            .iter()
            .map(|hit| {
                chat.threads()
                    .iter()
                    .find(|thread| thread.id == hit.thread.id)
                    .unwrap_or(&hit.thread)
            })
            .collect();
    }
    let query = modal.command_query();
    chat.threads()
        .iter()
        .filter(|thread| !is_hidden_handoff_thread(thread))
        .filter(|thread| match modal.thread_picker_tab() {
            ThreadPickerTab::Swarog => {
                !is_rarog_thread(thread)
                    && !is_internal_thread(thread)
                    && !is_gateway_thread(thread, chat)
                    && !is_weles_thread(thread)
                    && !is_workspace_thread_with_index(thread, workspace_index)
                    && !is_goal_thread_with_index(thread, goal_index)
                    && !is_playground_thread(thread)
                    && is_svarog_thread(thread)
            }
            ThreadPickerTab::Rarog => is_rarog_thread(thread),
            ThreadPickerTab::Weles => !is_playground_thread(thread) && is_weles_thread(thread),
            ThreadPickerTab::Goals => {
                !is_workspace_thread_with_index(thread, workspace_index)
                    && is_goal_thread_with_index(thread, goal_index)
            }
            ThreadPickerTab::Workspace => is_workspace_thread_with_index(thread, workspace_index),
            ThreadPickerTab::Playgrounds => is_playground_thread(thread),
            ThreadPickerTab::Internal => is_internal_thread(thread),
            ThreadPickerTab::Gateway => is_gateway_thread(thread, chat),
            ThreadPickerTab::Agent(agent_id) => thread_matches_agent_tab(
                thread,
                chat,
                &agent_id,
                subagents,
                goal_index,
                workspace_index,
            ),
        })
        .filter(|thread| thread_matches_query(thread, query, goal_index, workspace_index))
        .collect()
}

#[cfg(test)]
pub(super) fn tab_cells(chat: &ChatState, subagents: &SubAgentsState) -> Vec<ThreadPickerTabCell> {
    tab_cells_inner(chat, subagents, None, None)
}

pub(super) fn tab_cells_inner(
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    workspace_index: Option<&WorkspaceThreadIndex>,
) -> Vec<ThreadPickerTabCell> {
    let mut x = 0;
    tab_specs_inner(chat, subagents, goal_index, workspace_index)
        .into_iter()
        .map(|spec| {
            let tab = spec.tab;
            let label = spec.label;
            let cell = ThreadPickerTabCell {
                tab,
                label,
                start: x,
            };
            x = x.saturating_add(cell.label.chars().count() as u16 + TAB_GAP);
            cell
        })
        .collect()
}

pub(super) fn tab_scroll_offset(
    area_width: u16,
    cells: &[ThreadPickerTabCell],
    selected: &ThreadPickerTab,
) -> u16 {
    if area_width == 0 {
        return 0;
    }

    let Some(selected_cell) = cells.iter().find(|cell| &cell.tab == selected) else {
        return 0;
    };
    let total_width = cells
        .last()
        .map(|cell| cell.start.saturating_add(cell.label.chars().count() as u16))
        .unwrap_or(0);
    let max_offset = total_width.saturating_sub(area_width);
    let selected_width = selected_cell.label.chars().count() as u16;
    let desired_offset = if selected_width >= area_width {
        selected_cell.start
    } else {
        selected_cell
            .start
            .saturating_add(selected_width)
            .saturating_sub(area_width)
    };

    desired_offset.min(max_offset)
}

pub(super) fn visible_tab_cells(
    area: Rect,
    cells: &[ThreadPickerTabCell],
    scroll: u16,
) -> Vec<(ThreadPickerTab, Rect, String)> {
    let viewport_end = scroll.saturating_add(area.width);

    cells
        .iter()
        .filter_map(|cell| {
            let cell_width = cell.label.chars().count() as u16;
            let cell_end = cell.start.saturating_add(cell_width);
            let visible_start = cell.start.max(scroll);
            let visible_end = cell_end.min(viewport_end);

            if visible_start >= visible_end {
                return None;
            }

            let skip = visible_start.saturating_sub(cell.start) as usize;
            let width = visible_end.saturating_sub(visible_start);
            let label = cell.label.chars().skip(skip).take(width as usize).collect();
            let rect = Rect::new(
                area.x.saturating_add(visible_start.saturating_sub(scroll)),
                area.y,
                width,
                area.height,
            );

            Some((cell.tab.clone(), rect, label))
        })
        .collect()
}

pub(crate) fn visible_window(
    cursor: usize,
    item_count: usize,
    list_height: usize,
) -> (usize, usize) {
    if item_count == 0 || list_height == 0 {
        return (0, 0);
    }

    let height = list_height.min(item_count);
    let max_start = item_count.saturating_sub(height);
    let start = cursor
        .saturating_sub(height.saturating_sub(1))
        .min(max_start);
    (start, height)
}

pub(super) fn synthetic_row_label(tab: ThreadPickerTab) -> &'static str {
    match tab {
        ThreadPickerTab::Goals => "Goal threads are created automatically",
        ThreadPickerTab::Workspace => "Workspace threads are created from workspace tasks",
        ThreadPickerTab::Playgrounds => "Playgrounds are created automatically",
        ThreadPickerTab::Gateway => "Gateway threads are created automatically",
        ThreadPickerTab::Agent(_) => "+ New conversation",
        _ => "+ New conversation",
    }
}

pub fn render_for_workspace(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    workspace: &WorkspaceState,
    theme: &ThemeTokens,
    loading_tab: Option<ThreadPickerTab>,
    tick_counter: u64,
) {
    let block = Block::default()
        .title(" THREADS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 5 {
        return;
    }

    let layout = thread_picker_layout(inner);
    let search_row = layout.search;
    let separator_row = layout.separator;
    let list_row = layout.list;
    let hints_row = layout.hints;
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    let selected_tab = modal.thread_picker_tab();

    if let (Some(sidebar), Some(divider)) = (layout.sidebar, layout.divider) {
        let rows = sidebar_rows(tab_specs_inner(
            chat,
            subagents,
            Some(&goal_index),
            Some(&workspace_index),
        ));
        let label_width = (sidebar.width as usize).saturating_sub(4);
        let lines = rows
            .iter()
            .map(|row| match row {
                ThreadPickerSidebarRow::Blank => Line::raw(""),
                ThreadPickerSidebarRow::Group(title) => {
                    Line::from(Span::styled(format!(" {title}"), theme.fg_dim))
                }
                ThreadPickerSidebarRow::Tab(tab, label) => {
                    let label = if loading_tab.as_ref() == Some(tab) {
                        format!("{label} {}", thread_picker_spinner_frame(tick_counter))
                    } else {
                        label.clone()
                    };
                    let label = crate::widgets::message::truncate_to_width(&label, label_width);
                    if *tab == selected_tab {
                        Line::from(vec![
                            Span::styled(" \u{25b8} ", theme.accent_primary),
                            Span::styled(label, theme.fg_active),
                        ])
                    } else {
                        Line::from(vec![Span::raw("   "), Span::styled(label, theme.fg_dim)])
                    }
                }
            })
            .collect::<Vec<_>>();
        let scroll = sidebar_scroll(sidebar.height, &rows, &selected_tab);
        frame.render_widget(
            Paragraph::new(lines).scroll((scroll.min(u16::MAX as usize) as u16, 0)),
            sidebar,
        );
        frame.render_widget(
            Paragraph::new(
                (0..divider.height)
                    .map(|_| Line::from(Span::styled("\u{2502}", theme.fg_dim)))
                    .collect::<Vec<_>>(),
            ),
            divider,
        );
    }

    if let Some(tabs_row) = layout.tabs {
        let tab_cells = tab_cells_inner(chat, subagents, Some(&goal_index), Some(&workspace_index));
        let tab_scroll = tab_scroll_offset(tabs_row.width, &tab_cells, &selected_tab);
        let tab_line = Line::from(
            tab_cells
                .iter()
                .enumerate()
                .flat_map(|(index, cell)| {
                    let label = if loading_tab.as_ref() == Some(&cell.tab) {
                        format!(
                            "{} {}",
                            cell.label,
                            thread_picker_spinner_frame(tick_counter)
                        )
                    } else {
                        cell.label.clone()
                    };
                    let style = if cell.tab == selected_tab {
                        theme.accent_primary
                    } else {
                        theme.fg_dim
                    };
                    let mut spans = vec![Span::styled(label, style)];
                    if index + 1 < tab_cells.len() {
                        spans.push(Span::raw(" "));
                    }
                    spans
                })
                .collect::<Vec<_>>(),
        );
        frame.render_widget(Paragraph::new(tab_line).scroll((0, tab_scroll)), tabs_row);
    }

    let query = modal.command_query();
    let input_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            if query.is_empty() {
                "Search threads..."
            } else {
                query
            },
            theme.fg_active,
        ),
        if query.is_empty() {
            Span::raw("")
        } else {
            Span::raw("\u{2588}")
        },
    ]);
    frame.render_widget(Paragraph::new(input_line), search_row);

    let sep = match modal.thread_search() {
        Some(search) => thread_search_status_line(search, modal, separator_row.width, theme),
        None => Line::from(Span::styled(
            "\u{2500}".repeat(separator_row.width as usize),
            theme.fg_dim,
        )),
    };
    frame.render_widget(Paragraph::new(sep), separator_row);

    let active_id = chat.active_thread_id();
    let cursor = modal.picker_cursor();
    let list_h = list_row.height as usize;
    let inner_w = list_row.width as usize;

    // Do not build metadata and formatted titles for every cached thread. The
    // picker can hold hundreds of summaries; only one viewport is rendered.
    // Filter to stable indices first, then format the visible window below.
    let query = modal.command_query();
    let filtered = if modal.thread_search().is_some() {
        filtered_threads_inner(
            chat,
            modal,
            subagents,
            Some(&goal_index),
            Some(&workspace_index),
        )
    } else {
        filtered_thread_indices(chat, modal, subagents, &goal_index, &workspace_index, query)
            .into_iter()
            .filter_map(|index| chat.threads().get(index))
            .collect()
    };
    let total_items = filtered.len() + 1;
    let (visible_start, visible_len) = visible_window(cursor, total_items, list_h);
    let visible_threads = filtered
        .iter()
        .skip(visible_start.saturating_sub(1))
        .take(visible_len)
        .copied()
        .collect::<Vec<_>>();
    let status_index = ThreadPickerStatusIndex::from_state_for_threads(
        chat,
        tasks,
        visible_threads.iter().copied(),
    );

    let list_items: Vec<ListItem> = (0..list_h)
        .map(|i| {
            if i < visible_len {
                let absolute_index = visible_start + i;
                if absolute_index == 0 {
                    let row_label = synthetic_row_label(modal.thread_picker_tab());
                    let is_selected = cursor == 0;
                    if is_selected {
                        ListItem::new(Line::from(vec![Span::raw(format!("  {row_label}"))]))
                            .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                    } else {
                        ListItem::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(row_label, theme.fg_dim),
                        ]))
                    }
                } else {
                    let visible_index = absolute_index.saturating_sub(visible_start.max(1));
                    if let Some(thread) = visible_threads.get(visible_index).copied() {
                        let is_selected = cursor == absolute_index;
                        let is_active = active_id == Some(thread.id.as_str());

                        let dot_style = if is_active {
                            theme.accent_success
                        } else {
                            theme.fg_dim
                        };
                        let status = status_index.status_for(thread);
                        let status_style = match status {
                            ThreadPickerStatus::Running => theme.accent_success,
                            ThreadPickerStatus::Paused => theme.accent_secondary,
                            ThreadPickerStatus::Stopped => theme.accent_danger,
                            ThreadPickerStatus::Idle => theme.fg_dim,
                        };
                        let status_label = format!("[{}]", status.label());

                        let time_str = format_time_ago(thread.updated_at);
                        let tokens = thread.total_input_tokens + thread.total_output_tokens;
                        let token_str = format_tokens(tokens);

                        let display_title = thread_display_title_inner(
                            thread,
                            Some(&goal_index),
                            Some(&workspace_index),
                        );
                        let max_title = inner_w.saturating_sub(28).saturating_sub(
                            unicode_width::UnicodeWidthStr::width(status_label.as_str()),
                        );
                        let title = if max_title > 3 {
                            crate::widgets::message::truncate_to_width(&display_title, max_title)
                        } else {
                            display_title
                        };

                        if is_selected {
                            ListItem::new(Line::from(vec![
                                Span::styled("\u{25cf}", dot_style),
                                Span::raw(" "),
                                Span::raw(status_label.clone()),
                                Span::raw(" "),
                                Span::raw(title),
                                Span::raw("  "),
                                Span::raw(time_str),
                                Span::raw("  "),
                                Span::raw(token_str),
                            ]))
                            .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                        } else {
                            ListItem::new(Line::from(vec![
                                Span::raw("  "),
                                Span::styled("\u{25cf}", dot_style),
                                Span::raw(" "),
                                Span::styled(status_label, status_style),
                                Span::raw(" "),
                                Span::styled(title, theme.fg_active),
                                Span::raw("  "),
                                Span::styled(time_str, theme.fg_dim),
                                Span::raw("  "),
                                Span::styled(token_str, theme.fg_dim),
                            ]))
                        }
                    } else {
                        ListItem::new(Line::raw(""))
                    }
                }
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, list_row);

    let mut hints = vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" source  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" select  ", theme.fg_dim),
    ];
    if cursor > 0 {
        hints.push(Span::styled("Del", theme.fg_active));
        hints.push(Span::styled(" delete  ", theme.fg_dim));
        hints.push(Span::styled("Ctrl+S", theme.fg_active));
        hints.push(Span::styled(" stop/resume  ", theme.fg_dim));
    }
    hints.push(Span::styled("Shift+R", theme.fg_active));
    hints.push(Span::styled(" refresh  ", theme.fg_dim));
    hints.push(Span::styled("Esc", theme.fg_active));
    hints.push(Span::styled(" close", theme.fg_dim));
    let hints = Line::from(hints);
    frame.render_widget(Paragraph::new(hints), hints_row);
}

#[cfg(test)]
pub fn hit_test(
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    mouse: Position,
) -> Option<ThreadPickerHitTarget> {
    let tasks = TaskState::default();
    let workspace = WorkspaceState::new();
    hit_test_for_workspace(area, chat, modal, subagents, &tasks, &workspace, mouse)
}

fn filtered_thread_indices(
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    goal_index: &GoalThreadIndex,
    workspace_index: &WorkspaceThreadIndex,
    query: &str,
) -> Vec<usize> {
    chat.threads()
        .iter()
        .enumerate()
        .filter(|(_, thread)| !is_hidden_handoff_thread(thread))
        .filter(|(_, thread)| match modal.thread_picker_tab() {
            ThreadPickerTab::Swarog => {
                !is_rarog_thread(thread)
                    && !is_internal_thread(thread)
                    && !is_gateway_thread(thread, chat)
                    && !is_weles_thread(thread)
                    && !is_workspace_thread_with_index(thread, Some(workspace_index))
                    && !is_goal_thread_with_index(thread, Some(goal_index))
                    && !is_playground_thread(thread)
                    && is_svarog_thread(thread)
            }
            ThreadPickerTab::Rarog => is_rarog_thread(thread),
            ThreadPickerTab::Weles => !is_playground_thread(thread) && is_weles_thread(thread),
            ThreadPickerTab::Goals => {
                !is_workspace_thread_with_index(thread, Some(workspace_index))
                    && is_goal_thread_with_index(thread, Some(goal_index))
            }
            ThreadPickerTab::Workspace => {
                is_workspace_thread_with_index(thread, Some(workspace_index))
            }
            ThreadPickerTab::Playgrounds => is_playground_thread(thread),
            ThreadPickerTab::Internal => is_internal_thread(thread),
            ThreadPickerTab::Gateway => is_gateway_thread(thread, chat),
            ThreadPickerTab::Agent(ref agent_id) => thread_matches_agent_tab(
                thread,
                chat,
                agent_id,
                subagents,
                Some(goal_index),
                Some(workspace_index),
            ),
        })
        .filter(|(_, thread)| {
            thread_matches_query(thread, query, Some(goal_index), Some(workspace_index))
        })
        .map(|(index, _)| index)
        .collect()
}

pub(super) fn thread_search_status_line(
    search: &ThreadSearchState,
    modal: &ModalState,
    width: u16,
    theme: &ThemeTokens,
) -> Line<'static> {
    let mode = if modal.thread_search_query() != Some(search.query.as_str()) {
        "searching\u{2026}".to_string()
    } else if search.hits.is_empty() {
        "no matches".to_string()
    } else if search.semantic {
        "semantic + keyword".to_string()
    } else if let Some(error) = search.semantic_error.as_deref() {
        format!("keyword (semantic failed: {error})")
    } else {
        "keyword".to_string()
    };
    let snippet = modal
        .picker_cursor()
        .checked_sub(1)
        .and_then(|index| search.hits.get(index))
        .and_then(|hit| hit.snippet.as_deref())
        .map(|snippet| format!(" \u{2502} {snippet}"))
        .unwrap_or_default();
    let line =
        crate::widgets::message::truncate_to_width(&format!(" {mode}{snippet}"), width as usize);
    Line::from(Span::styled(line, theme.fg_dim))
}

fn thread_picker_spinner_frame(tick_counter: u64) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    FRAMES[((tick_counter / 2) as usize) % FRAMES.len()]
}
