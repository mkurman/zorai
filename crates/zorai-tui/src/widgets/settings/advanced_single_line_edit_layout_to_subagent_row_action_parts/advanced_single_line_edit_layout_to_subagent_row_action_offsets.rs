use super::super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
use crate::state::config::ConfigState;
use crate::state::settings::{SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use ratatui::prelude::*;
pub(crate) fn single_line_edit_layout(
    settings: &SettingsState,
    config: &ConfigState,
    field: &str,
) -> Option<(usize, usize)> {
    if let Some(page) = crate::state::settings_registry::settings_page_spec(settings.active_tab()) {
        let rows = page.rows(config, settings.editing_textarea());
        if let Some(offset) = honcho_editor_field_offset(settings.active_tab(), config, field) {
            return Some((rows.len() + offset, HONCHO_EDITOR_VALUE_COL));
        }
        return rows
            .iter()
            .position(|row| {
                matches!(row, crate::state::settings_registry::SettingsPageRow::Field { spec, .. } if spec.name == field)
            })
            .map(|row| (row, crate::state::settings_registry::SETTINGS_PAGE_VALUE_COL));
    }
    match settings.active_tab() {
        SettingsTab::Provider => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" | "target_agent_model" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" | "target_agent_context_window" => Some((11, 19)),
            _ => None,
        },
        SettingsTab::Tools
        | SettingsTab::Skills
        | SettingsTab::WebSearch
        | SettingsTab::Media
        | SettingsTab::Memory
        | SettingsTab::Heartbeat
        | SettingsTab::Proactivity
        | SettingsTab::Workflows
        | SettingsTab::Chat
        | SettingsTab::Compaction
        | SettingsTab::Execution
        | SettingsTab::Safety
        | SettingsTab::Storage => None,
        SettingsTab::Mlflow => match field {
            "mlflow_tracking_uri" => Some((11, 20)),
            "mlflow_experiment_name" => Some((12, 20)),
            "mlflow_experiment_id" => Some((13, 20)),
            _ => None,
        },
        SettingsTab::Gateway => match field {
            "gateway_default_agent" => Some((5, 19)),
            "gateway_prefix" => Some((6, 19)),
            "slack_token" => Some((9, 19)),
            "slack_channel_filter" => Some((10, 19)),
            "slack_default_agent" => Some((11, 19)),
            "telegram_token" => Some((14, 19)),
            "telegram_allowed_chats" => Some((15, 19)),
            "telegram_default_agent" => Some((16, 19)),
            "discord_token" => Some((19, 19)),
            "discord_channel_filter" => Some((20, 19)),
            "discord_allowed_users" => Some((21, 19)),
            "discord_default_agent" => Some((22, 19)),
            "whatsapp_allowed_contacts" => Some((25, 19)),
            "whatsapp_token" => Some((26, 19)),
            "whatsapp_phone_id" => Some((27, 19)),
            "whatsapp_default_agent" => Some((28, 19)),
            "whatsapp_link_device" => Some((29, 19)),
            "whatsapp_relink_device" => Some((30, 19)),
            _ => None,
        },
        SettingsTab::Auth => None,
        SettingsTab::Agent => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" | "target_agent_model" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" | "target_agent_context_window" => Some((11, 19)),
            "huggingface_provider" => Some((12, 19)),
            "swarog_alias" => Some((
                16 + SettingsState::provider_extra_field_names(&config.provider).len(),
                19,
            )),
            _ => None,
        },
        SettingsTab::SubAgents => None,
        SettingsTab::Concierge => None,
        SettingsTab::Plugins => None,
        SettingsTab::Database => None,
        SettingsTab::About => None,
    }
}

pub(crate) fn textarea_edit_layout(
    settings: &SettingsState,
    config: &ConfigState,
    field: &str,
) -> Option<(usize, usize)> {
    if let Some(page) = crate::state::settings_registry::settings_page_spec(settings.active_tab()) {
        return page
            .field_row(config, settings.editing_textarea(), field)
            .map(|row| {
                (
                    row + 1,
                    crate::state::settings_registry::SETTINGS_PAGE_TEXTAREA_COL,
                )
            });
    }
    match settings.active_tab() {
        SettingsTab::Agent if field == "system_prompt" => Some((
            17 + SettingsState::provider_extra_field_names(&config.provider).len(),
            4,
        )),
        SettingsTab::Gateway if field == "whatsapp_allowed_contacts" => Some((27, 4)),
        _ => None,
    }
}

pub(crate) fn settings_row_hit(
    settings: &SettingsState,
    config: &ConfigState,
    subagents: &SubAgentsState,
    row: usize,
) -> Option<(usize, Option<usize>)> {
    if let Some(page) = crate::state::settings_registry::settings_page_spec(settings.active_tab()) {
        return page
            .field_at_row(config, settings.editing_textarea(), row)
            .map(|index| (index, None));
    }
    match settings.active_tab() {
        SettingsTab::Provider => row
            .checked_sub(4)
            .filter(|idx| *idx < 8)
            .map(|idx| (idx, None)),
        SettingsTab::Tools
        | SettingsTab::Skills
        | SettingsTab::WebSearch
        | SettingsTab::Media
        | SettingsTab::Memory
        | SettingsTab::Heartbeat
        | SettingsTab::Proactivity
        | SettingsTab::Workflows
        | SettingsTab::Chat
        | SettingsTab::Compaction
        | SettingsTab::Execution
        | SettingsTab::Safety
        | SettingsTab::Storage => None,
        SettingsTab::Mlflow => row
            .checked_sub(4)
            .filter(|idx| *idx < 13)
            .map(|idx| (idx, None)),
        SettingsTab::Gateway => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            6 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            14 => Some((6, None)),
            15 => Some((7, None)),
            16 => Some((8, None)),
            19 => Some((9, None)),
            20 => Some((10, None)),
            21 => Some((11, None)),
            22 => Some((12, None)),
            25 => Some((13, None)),
            r if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("whatsapp_allowed_contacts") =>
            {
                let textarea_lines = settings.edit_buffer().lines().count().max(1);
                match r {
                    26..=27 => Some((13, None)),
                    line if line >= 28 && line < 28 + textarea_lines => Some((13, None)),
                    line if line == 28 + textarea_lines => Some((14, None)),
                    line if line == 29 + textarea_lines => Some((15, None)),
                    line if line == 30 + textarea_lines => Some((16, None)),
                    line if line == 31 + textarea_lines => Some((17, None)),
                    line if line == 32 + textarea_lines => Some((18, None)),
                    _ => None,
                }
            }
            26 => Some((14, None)),
            27 => Some((15, None)),
            28 => Some((16, None)),
            29 => Some((17, None)),
            30 => Some((18, None)),
            _ => None,
        },
        SettingsTab::Auth => row
            .checked_sub(4)
            .filter(|idx| *idx < 3)
            .map(|idx| (idx, None)),
        SettingsTab::Agent => {
            let extras = SettingsState::provider_extra_field_names(&config.provider).len();
            if (4..12 + extras).contains(&row) {
                return Some((row - 4, None));
            }
            let row = row.checked_sub(extras)?;
            let hit = if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("system_prompt")
            {
                let prompt_lines = settings.edit_buffer().lines().count().max(1);
                match row {
                    16 => Some(8),
                    r if (17..=20 + prompt_lines).contains(&r) => Some(9),
                    r if r == 21 + prompt_lines => Some(10),
                    r if r == 22 + prompt_lines => Some(11),
                    _ => None,
                }
            } else {
                match row {
                    16 => Some(8),
                    17 => Some(9),
                    18 => Some(10),
                    19 => Some(11),
                    _ => None,
                }
            };
            hit.map(|idx| (idx + extras, None))
        }
        SettingsTab::SubAgents => {
            let list_len = subagents.entries.len();
            if list_len > 0 && (4..4 + list_len).contains(&row) {
                Some((0, Some(row - 4)))
            } else {
                match row {
                    r if r == 5 + list_len => Some((1, None)),
                    r if r == 6 + list_len => Some((2, None)),
                    r if r == 7 + list_len => Some((3, None)),
                    r if r == 8 + list_len => Some((4, None)),
                    _ => None,
                }
            }
        }
        SettingsTab::Concierge => row
            .checked_sub(4)
            .filter(|idx| *idx < 4)
            .map(|idx| (idx, None)),
        SettingsTab::Plugins => None,
        SettingsTab::Database => None,
        SettingsTab::About => None,
    }
}

pub(crate) fn auth_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::auth::ProviderAuthEntry,
) -> (u16, u16, u16) {
    let primary_label = auth_primary_label(entry);
    let test_label = auth_secondary_label(entry);
    let actions_width =
        primary_label.chars().count() as u16 + 1 + test_label.chars().count() as u16;
    let primary_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let primary_end = primary_start.saturating_add(primary_label.chars().count() as u16);
    let test_start = primary_end.saturating_add(1);
    (primary_start, primary_end, test_start)
}

use zorai_shared::providers::{
    PROVIDER_ID_CLAUDE_CODE_CLI, PROVIDER_ID_CURSOR, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
};

pub(crate) fn auth_primary_label(entry: &crate::state::auth::ProviderAuthEntry) -> &'static str {
    match (
        entry.provider_id.as_str(),
        entry.authenticated,
        entry.auth_source.as_str(),
    ) {
        (PROVIDER_ID_GITHUB_COPILOT, false, "github_copilot") => "[Token]",
        (PROVIDER_ID_CLAUDE_CODE_CLI, true, _) => "[Ready]",
        (PROVIDER_ID_CLAUDE_CODE_CLI, false, _) => "[Install]",
        (PROVIDER_ID_CURSOR, true, _) => "[Logout]",
        (PROVIDER_ID_CURSOR, false, _) => "[Login]",
        (_, true, _) => "[Logout]",
        _ => "[API Key]",
    }
}

pub(crate) fn auth_secondary_label(entry: &crate::state::auth::ProviderAuthEntry) -> &'static str {
    match (
        entry.provider_id.as_str(),
        entry.authenticated,
        entry.auth_source.as_str(),
    ) {
        (PROVIDER_ID_OPENAI, false, _) => "[ChatGPT]",
        (PROVIDER_ID_GITHUB_COPILOT, false, "github_copilot") => "[Browser]",
        (PROVIDER_ID_CLAUDE_CODE_CLI, _, _) => "[Recheck]",
        (PROVIDER_ID_CURSOR, _, _) => "[Recheck]",
        _ => "[Test]",
    }
}

pub(crate) fn auth_hit_test(
    content_area: Rect,
    auth: &crate::state::auth::AuthState,
    scroll: usize,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize + scroll;
    let mut current_row = 4usize;
    for (entry_index, entry) in auth.visible_entries().into_iter().enumerate() {
        if row == current_row {
            let (primary_start, primary_end, test_start) =
                auth_row_action_offsets(content_area, entry);
            return if mouse.x >= primary_start && mouse.x < primary_end {
                Some(SettingsHitTarget::AuthAction {
                    index: entry_index,
                    action: AuthTabAction::Primary,
                })
            } else if mouse.x >= test_start {
                Some(SettingsHitTarget::AuthAction {
                    index: entry_index,
                    action: AuthTabAction::Test,
                })
            } else {
                Some(SettingsHitTarget::AuthProviderItem(entry_index))
            };
        }
        current_row += 1;
        if auth.validation_results.contains_key(&entry.provider_id) {
            if row == current_row {
                return Some(SettingsHitTarget::AuthProviderItem(entry_index));
            }
            current_row += 1;
        }
    }
    None
}

pub(crate) fn subagent_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::subagents::SubAgentEntry,
) -> (u16, u16, u16, u16, u16) {
    let edit_label = "[Edit]";
    let delete_label = if entry.delete_allowed {
        "[Delete]"
    } else {
        "[Protected]"
    };
    let toggle_label = if entry.enabled {
        if entry.disable_allowed {
            "[Disable]"
        } else {
            "[Locked]"
        }
    } else {
        "[Enable]"
    };
    let actions_width = edit_label.chars().count() as u16
        + 1
        + delete_label.chars().count() as u16
        + 1
        + toggle_label.chars().count() as u16;
    let edit_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let delete_start = edit_start.saturating_add(edit_label.chars().count() as u16 + 1);
    let toggle_start = delete_start.saturating_add(delete_label.chars().count() as u16 + 1);
    (
        edit_start,
        delete_start,
        toggle_start,
        delete_start.saturating_sub(1),
        toggle_start.saturating_add(toggle_label.chars().count() as u16),
    )
}

pub(crate) const HONCHO_EDITOR_VALUE_COL: usize = 19;

pub(crate) fn honcho_editor_field_offset(
    tab: SettingsTab,
    config: &ConfigState,
    field: &str,
) -> Option<usize> {
    if tab != SettingsTab::Memory || config.honcho_editor.is_none() {
        return None;
    }
    match field {
        "honcho_editor_api_key" => Some(3),
        "honcho_editor_base_url" => Some(4),
        "honcho_editor_workspace_id" => Some(5),
        _ => None,
    }
}
