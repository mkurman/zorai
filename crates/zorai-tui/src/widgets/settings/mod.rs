use ratatui::prelude::*;

#[path = "advanced_single_line_edit_layout_to_subagent_row_action_offsets.rs"]
mod advanced_single_line_edit_layout_to_subagent_row_action_offsets;
#[path = "render_about_tab.rs"]
mod render_about_tab;
#[path = "render_auth_tab_to_render_agent_tab.rs"]
mod render_auth_tab_to_render_agent_tab;
#[path = "render_chat_tab_to_render_honcho_editor_actions.rs"]
mod render_chat_tab_to_render_honcho_editor_actions;
#[path = "render_concierge_tab_to_render_feature_toggle_line.rs"]
mod render_concierge_tab_to_render_feature_toggle_line;
#[path = "render_database_tab.rs"]
mod render_database_tab;
#[path = "render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content.rs"]
mod render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content;
#[path = "render_gateway_text_field.rs"]
mod render_gateway_text_field;
#[path = "render_mlflow_tab.rs"]
mod render_mlflow_tab;
#[path = "render_plugins_tab_to_connector_readiness_style.rs"]
mod render_plugins_tab_to_connector_readiness_style;
#[path = "render_provider_tab_to_render_tools_tab.rs"]
mod render_provider_tab_to_render_tools_tab;
#[path = "render_settings_page.rs"]
mod render_settings_page;
#[path = "wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render.rs"]
mod wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::ConfigState;

    #[path = "agent_tab.rs"]
    mod agent_tab;
    #[path = "concierge_tab.rs"]
    mod concierge_tab;
    #[path = "gateway_tab.rs"]
    mod gateway_tab;
    #[path = "heartbeat_page.rs"]
    mod heartbeat_page;
    #[path = "mlflow_tab.rs"]
    mod mlflow_tab;
    #[path = "settings_handles_empty_state_to_auth_tab_shows_chatgpt_logout.rs"]
    mod settings_handles_empty_state_to_auth_tab_shows_chatgpt_logout;
    #[path = "settings_pages.rs"]
    mod settings_pages;
}

pub(crate) use advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
pub(crate) use render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;

#[cfg(test)]
pub(crate) use render_auth_tab_to_render_agent_tab::*;
#[cfg(test)]
pub(crate) use render_concierge_tab_to_render_feature_toggle_line::*;
#[cfg(test)]
pub(crate) use render_gateway_text_field::*;
pub(crate) use render_mlflow_tab::*;
#[cfg(test)]
pub(crate) use render_plugins_tab_to_connector_readiness_style::*;
#[cfg(test)]
pub(crate) use render_provider_tab_to_render_tools_tab::*;
#[cfg(test)]
pub(crate) use render_settings_page::*;
