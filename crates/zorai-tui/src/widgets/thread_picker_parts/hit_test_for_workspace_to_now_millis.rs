use super::from_tasks_to_is_weles_thread::*;
use super::is_svarog_agent_name_to_hit_test::*;
use crate::state::chat::ChatState;
use crate::state::modal::ModalState;
use crate::state::subagents::SubAgentsState;
use crate::state::task::TaskState;
use crate::state::workspace::WorkspaceState;
use crate::widgets::token_format::format_token_count;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders};

pub fn hit_test_for_workspace(
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    workspace: &WorkspaceState,
    mouse: Position,
) -> Option<ThreadPickerHitTarget> {
    if !area.contains(mouse) {
        return None;
    }

    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    if inner.height < 5 {
        return None;
    }
    let layout = thread_picker_layout(inner);
    let list_row = layout.list;
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    let workspace_index = WorkspaceThreadIndex::from_workspace(workspace, tasks);
    let selected_tab = modal.thread_picker_tab();

    if let Some(sidebar) = layout.sidebar.filter(|sidebar| sidebar.contains(mouse)) {
        let rows = sidebar_rows(tab_specs_inner(
            chat,
            subagents,
            Some(&goal_index),
            Some(&workspace_index),
        ));
        let row = mouse.y.saturating_sub(sidebar.y) as usize
            + sidebar_scroll(sidebar.height, &rows, &selected_tab);
        return match rows.get(row) {
            Some(ThreadPickerSidebarRow::Tab(tab, _)) => {
                Some(ThreadPickerHitTarget::Tab(tab.clone()))
            }
            _ => None,
        };
    }

    if let Some(tabs_row) = layout.tabs.filter(|tabs_row| tabs_row.contains(mouse)) {
        let tab_cells = tab_cells_inner(chat, subagents, Some(&goal_index), Some(&workspace_index));
        let tab_scroll = tab_scroll_offset(tabs_row.width, &tab_cells, &selected_tab);
        for (tab, rect, _) in visible_tab_cells(tabs_row, &tab_cells, tab_scroll) {
            if rect.contains(mouse) {
                return Some(ThreadPickerHitTarget::Tab(tab));
            }
        }
    }

    if list_row.contains(mouse) {
        let total_items = filtered_threads_inner(
            chat,
            modal,
            subagents,
            Some(&goal_index),
            Some(&workspace_index),
        )
        .len()
            + 1;
        let row_idx = mouse.y.saturating_sub(list_row.y) as usize;
        let (visible_start, visible_len) =
            visible_window(modal.picker_cursor(), total_items, list_row.height as usize);
        if row_idx < visible_len {
            return Some(ThreadPickerHitTarget::Item(visible_start + row_idx));
        }
    }

    None
}

/// Format millisecond timestamp as "Xm ago" or "Xh ago" etc.
pub(super) fn format_time_ago(updated_at: u64) -> String {
    if updated_at == 0 {
        return String::new();
    }
    let now = now_millis();
    if now < updated_at {
        return "just now".to_string();
    }
    let diff_secs = (now - updated_at) / 1000;
    if diff_secs < 60 {
        format!("{}s ago", diff_secs)
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h ago", diff_secs / 3600)
    } else {
        format!("{}d ago", diff_secs / 86400)
    }
}

/// Format token count compactly
pub(super) fn format_tokens(tokens: u64) -> String {
    if tokens == 0 {
        return String::new();
    }
    format_token_count(tokens)
}

pub(super) fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
