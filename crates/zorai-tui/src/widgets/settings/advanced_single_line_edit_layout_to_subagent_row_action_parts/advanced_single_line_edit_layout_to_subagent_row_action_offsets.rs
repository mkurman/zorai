use super::super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
use crate::state::config::ConfigState;
use crate::state::settings::{SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use ratatui::prelude::*;
pub(crate) fn advanced_single_line_edit_layout(
    config: &ConfigState,
    field: &str,
) -> Option<(usize, usize)> {
    let row = match config.compaction_strategy.as_str() {
        "weles" => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "compact_threshold_pct" => 19,
            "keep_recent_on_compact" => 20,
            "bash_timeout_secs" => 21,
            "weles_max_concurrent_reviews" => 22,
            "compaction_weles_model" => 26,
            "snapshot_max_count" => 31,
            "snapshot_max_size_mb" => 32,
            "auto_refresh_interval_secs" => 34,
            _ => return None,
        },
        "custom_model" => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "compact_threshold_pct" => 19,
            "keep_recent_on_compact" => 20,
            "bash_timeout_secs" => 21,
            "weles_max_concurrent_reviews" => 22,
            "compaction_custom_base_url" => 26,
            "compaction_custom_model" => 28,
            "compaction_custom_api_key" => 30,
            "compaction_custom_assistant_id" => 31,
            "compaction_custom_context_window_tokens" => 33,
            "snapshot_max_count" => 37,
            "snapshot_max_size_mb" => 38,
            "auto_refresh_interval_secs" => 40,
            _ => return None,
        },
        _ => match field {
            "max_context_messages" => 10,
            "max_tool_loops" => 11,
            "max_retries" => 12,
            "retry_delay_ms" => 13,
            "message_loop_delay_ms" => 14,
            "tool_call_delay_ms" => 15,
            "llm_stream_chunk_timeout_secs" => 16,
            "compact_threshold_pct" => 19,
            "keep_recent_on_compact" => 20,
            "bash_timeout_secs" => 21,
            "weles_max_concurrent_reviews" => 22,
            "snapshot_max_count" => 29,
            "snapshot_max_size_mb" => 30,
            "auto_refresh_interval_secs" => 32,
            _ => return None,
        },
    };

    Some((row, 20))
}

pub(crate) fn single_line_edit_layout(
    settings: &SettingsState,
    config: &ConfigState,
    field: &str,
) -> Option<(usize, usize)> {
    match settings.active_tab() {
        SettingsTab::Provider => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" | "target_agent_model" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" | "target_agent_context_window" => Some((11, 19)),
            _ => None,
        },
        SettingsTab::WebSearch => match field {
            "duckduckgo_region" => Some((6, 19)),
            "firecrawl_api_key" => Some((8, 19)),
            "exa_api_key" => Some((9, 19)),
            "tavily_api_key" => Some((10, 19)),
            "search_max_results" => Some((11, 19)),
            "search_timeout" => Some((12, 19)),
            _ => None,
        },
        SettingsTab::Chat => match field {
            "tui_chat_history_page_size" => Some((23, 19)),
            "participant_observer_restore_window_hours" => Some((24, 19)),
            "honcho_editor_api_key" => Some((9, 19)),
            "honcho_editor_base_url" => Some((10, 19)),
            "honcho_editor_workspace_id" => Some((11, 19)),
            _ => None,
        },
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
            "telegram_token" => Some((13, 19)),
            "telegram_allowed_chats" => Some((14, 19)),
            "discord_token" => Some((17, 19)),
            "discord_channel_filter" => Some((18, 19)),
            "discord_allowed_users" => Some((19, 19)),
            "whatsapp_allowed_contacts" => Some((22, 19)),
            "whatsapp_token" => Some((23, 19)),
            "whatsapp_phone_id" => Some((24, 19)),
            "whatsapp_link_device" => Some((25, 19)),
            "whatsapp_relink_device" => Some((26, 19)),
            _ => None,
        },
        SettingsTab::Auth => None,
        SettingsTab::Agent => match field {
            "base_url" => Some((5, 19)),
            "custom_model_entry" | "target_agent_model" => Some((7, 19)),
            "assistant_id" => Some((9, 19)),
            "context_window_tokens" | "target_agent_context_window" => Some((11, 19)),
            _ => None,
        },
        SettingsTab::SubAgents => None,
        SettingsTab::Concierge => None,
        SettingsTab::Advanced => advanced_single_line_edit_layout(config, field),
        SettingsTab::Tools => None,
        SettingsTab::Features => match field {
            "feat_heartbeat_cron" => Some((8, 20)),
            "feat_heartbeat_quiet_start" => Some((9, 20)),
            "feat_heartbeat_quiet_end" => Some((10, 20)),
            "feat_decay_half_life_hours" => Some((19, 20)),
            "feat_heuristic_promotion_threshold" => Some((20, 20)),
            "feat_skill_promotion_threshold" => Some((25, 20)),
            _ => None,
        },
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
    match settings.active_tab() {
        SettingsTab::Agent if field == "system_prompt" => Some((17, 4)),
        SettingsTab::Gateway if field == "whatsapp_allowed_contacts" => Some((24, 4)),
        SettingsTab::Advanced => match config.compaction_strategy.as_str() {
            "weles" => match field {
                "workspace_repo_monitor_include_dirs" => Some((38, 4)),
                "workspace_repo_monitor_exclude_dirs" => Some((39, 4)),
                _ => None,
            },
            "custom_model" => match field {
                "workspace_repo_monitor_include_dirs" => Some((44, 4)),
                "workspace_repo_monitor_exclude_dirs" => Some((45, 4)),
                _ => None,
            },
            _ => match field {
                "workspace_repo_monitor_include_dirs" => Some((36, 4)),
                "workspace_repo_monitor_exclude_dirs" => Some((37, 4)),
                _ => None,
            },
        },
        _ => None,
    }
}

pub(crate) fn advanced_settings_row_hit(
    config: &ConfigState,
    row: usize,
) -> Option<(usize, Option<usize>)> {
    match config.compaction_strategy.as_str() {
        "weles" => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            21 => Some((15, None)),
            22 => Some((16, None)),
            23 => Some((17, None)),
            24 => Some((18, None)),
            25 => Some((19, None)),
            26 => Some((20, None)),
            27 => Some((21, None)),
            28 => Some((22, None)),
            29 => Some((23, None)),
            30 => Some((24, None)),
            37 => Some((25, None)),
            38 => Some((26, None)),
            39 => Some((27, None)),
            _ => None,
        },
        "custom_model" => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            21 => Some((15, None)),
            22 => Some((16, None)),
            23 => Some((17, None)),
            24 => Some((18, None)),
            25 => Some((19, None)),
            26 => Some((20, None)),
            27 => Some((21, None)),
            28 => Some((22, None)),
            29 => Some((23, None)),
            30 => Some((24, None)),
            31 => Some((25, None)),
            32 => Some((26, None)),
            33 => Some((27, None)),
            34 => Some((28, None)),
            35 => Some((29, None)),
            39 => Some((30, None)),
            40 => Some((31, None)),
            43 => Some((32, None)),
            44 => Some((33, None)),
            45 => Some((34, None)),
            _ => None,
        },
        _ => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            15 => Some((9, None)),
            16 => Some((10, None)),
            17 => Some((11, None)),
            18 => Some((12, None)),
            19 => Some((13, None)),
            20 => Some((14, None)),
            21 => Some((15, None)),
            22 => Some((16, None)),
            23 => Some((17, None)),
            29 => Some((18, None)),
            30 => Some((19, None)),
            31 => Some((20, None)),
            32 => Some((21, None)),
            35 => Some((22, None)),
            36 => Some((23, None)),
            37 => Some((24, None)),
            _ => None,
        },
    }
}

pub(crate) fn settings_row_hit(
    settings: &SettingsState,
    config: &ConfigState,
    subagents: &SubAgentsState,
    row: usize,
) -> Option<(usize, Option<usize>)> {
    match settings.active_tab() {
        SettingsTab::Provider => row
            .checked_sub(4)
            .filter(|idx| *idx < 8)
            .map(|idx| (idx, None)),
        SettingsTab::Tools => row
            .checked_sub(4)
            .filter(|idx| *idx < 7)
            .map(|idx| (idx, None)),
        SettingsTab::WebSearch => match row {
            4..=12 => Some((row - 4, None)),
            15 => Some((9, None)),
            _ => None,
        },
        SettingsTab::Chat => row
            .checked_sub(4)
            .filter(|idx| *idx < 23)
            .map(|idx| (idx, None)),
        SettingsTab::Mlflow => row
            .checked_sub(4)
            .filter(|idx| *idx < 13)
            .map(|idx| (idx, None)),
        SettingsTab::Advanced => advanced_settings_row_hit(config, row),
        SettingsTab::Gateway => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            6 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            13 => Some((5, None)),
            14 => Some((6, None)),
            17 => Some((7, None)),
            18 => Some((8, None)),
            19 => Some((9, None)),
            22 => Some((10, None)),
            r if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("whatsapp_allowed_contacts") =>
            {
                let textarea_lines = settings.edit_buffer().lines().count().max(1);
                match r {
                    23..=24 => Some((10, None)),
                    line if line >= 25 && line < 25 + textarea_lines => Some((10, None)),
                    line if line == 25 + textarea_lines => Some((11, None)),
                    line if line == 26 + textarea_lines => Some((12, None)),
                    line if line == 27 + textarea_lines => Some((13, None)),
                    line if line == 28 + textarea_lines => Some((14, None)),
                    _ => None,
                }
            }
            23 => Some((11, None)),
            24 => Some((12, None)),
            25 => Some((13, None)),
            26 => Some((14, None)),
            _ => None,
        },
        SettingsTab::Auth => row
            .checked_sub(4)
            .filter(|idx| *idx < 3)
            .map(|idx| (idx, None)),
        SettingsTab::Agent => {
            if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("system_prompt")
            {
                let prompt_lines = settings.edit_buffer().lines().count().max(1);
                match row {
                    4..=11 => Some((row - 4, None)),
                    r if (17..=20 + prompt_lines).contains(&r) => Some((8, None)),
                    r if r == 21 + prompt_lines => Some((9, None)),
                    _ => None,
                }
            } else {
                match row {
                    4..=11 => Some((row - 4, None)),
                    17 => Some((8, None)),
                    18 => Some((9, None)),
                    _ => None,
                }
            }
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
        SettingsTab::Features => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            10 => Some((4, None)),
            11 => Some((5, None)),
            12 => Some((6, None)),
            13 => Some((7, None)),
            14 => Some((8, None)),
            17 => Some((9, None)),
            18 => Some((10, None)),
            19 => Some((11, None)),
            23 => Some((12, None)),
            24 => Some((13, None)),
            _ => None,
        },
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
    for (entry_index, entry) in auth.entries.iter().enumerate() {
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
