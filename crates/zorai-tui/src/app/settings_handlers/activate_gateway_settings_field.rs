use super::*;
impl TuiModel {
    pub(super) fn activate_gateway_settings_field(&mut self, field: &str) -> bool {
        match field {
            "gateway_default_agent"
            | "slack_default_agent"
            | "discord_default_agent"
            | "telegram_default_agent"
            | "whatsapp_default_agent" => {
                self.open_gateway_default_agent_picker(field);
            }
            "gateway_prefix" => {
                self.settings
                    .start_editing("gateway_prefix", &self.config.gateway_prefix.clone());
            }
            "slack_token" => {
                self.settings
                    .start_editing("slack_token", &self.config.slack_token.clone());
            }
            "slack_channel_filter" => {
                self.settings.start_editing(
                    "slack_channel_filter",
                    &self.config.slack_channel_filter.clone(),
                );
            }
            "telegram_token" => {
                self.settings
                    .start_editing("telegram_token", &self.config.telegram_token.clone());
            }
            "telegram_allowed_chats" => {
                self.settings.start_editing(
                    "telegram_allowed_chats",
                    &self.config.telegram_allowed_chats.clone(),
                );
            }
            "discord_token" => {
                self.settings
                    .start_editing("discord_token", &self.config.discord_token.clone());
            }
            "discord_channel_filter" => {
                self.settings.start_editing(
                    "discord_channel_filter",
                    &self.config.discord_channel_filter.clone(),
                );
            }
            "discord_allowed_users" => {
                self.settings.start_editing(
                    "discord_allowed_users",
                    &self.config.discord_allowed_users.clone(),
                );
            }
            "whatsapp_allowed_contacts" => {
                self.settings.start_editing(
                    "whatsapp_allowed_contacts",
                    &self.config.whatsapp_allowed_contacts.clone(),
                );
            }
            "whatsapp_token" => {
                self.settings
                    .start_editing("whatsapp_token", &self.config.whatsapp_token.clone());
            }
            "whatsapp_phone_id" => {
                self.settings
                    .start_editing("whatsapp_phone_id", &self.config.whatsapp_phone_id.clone());
            }
            "whatsapp_link_device" => {
                if !self.whatsapp_linking_allowed() {
                    self.status_line =
                        "Set at least one allowed WhatsApp phone number before linking".to_string();
                    return true;
                }
                self.send_daemon_command(DaemonCommand::WhatsAppLinkSubscribe);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                if self.modal.whatsapp_link().phase() == modal::WhatsAppLinkPhase::Connected {
                    self.status_line = "Showing WhatsApp link status".to_string();
                } else {
                    self.modal.set_whatsapp_link_starting();
                    self.send_daemon_command(DaemonCommand::WhatsAppLinkStart);
                    self.status_line = "Starting WhatsApp link workflow".to_string();
                }
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
            }
            "whatsapp_relink_device" => {
                if !self.whatsapp_linking_allowed() {
                    self.status_line =
                        "Set at least one allowed WhatsApp phone number before linking".to_string();
                    return true;
                }
                if self.modal.whatsapp_link().phase() != modal::WhatsAppLinkPhase::Connected {
                    self.status_line =
                        "WhatsApp is not linked yet — use Link Device first".to_string();
                    return true;
                }
                self.modal.set_whatsapp_link_starting();
                self.send_daemon_command(DaemonCommand::WhatsAppLinkSubscribe);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkReset);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStart);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
                self.status_line = "Restarting WhatsApp link workflow".to_string();
            }
            _ => return false,
        }
        true
    }
}
