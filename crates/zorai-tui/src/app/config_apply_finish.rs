use super::*;

impl TuiModel {
    pub(super) fn apply_snapshot_gateway_tier_config_json(&mut self, json: &serde_json::Value) {
        self.config.snapshot_max_count = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_snapshots"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        self.config.snapshot_max_size_mb = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_total_size_mb"))
            .and_then(|value| value.as_u64())
            .unwrap_or(10_240) as u32;
        self.config.snapshot_auto_cleanup = json
            .get("snapshot_retention")
            .and_then(|value| value.get("auto_cleanup"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        if let Some(gateway) = json.get("gateway") {
            self.config.gateway_enabled = gateway
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            self.config.gateway_prefix = gateway
                .get("command_prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("!zorai")
                .to_string();
            self.config.gateway_default_agent = gateway
                .get("default_agent")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .unwrap_or("rarog")
                .to_string();
            self.config.slack_default_agent = gateway
                .get("slack_default_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_default_agent = gateway
                .get("discord_default_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.telegram_default_agent = gateway
                .get("telegram_default_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_default_agent = gateway
                .get("whatsapp_default_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.slack_token = gateway
                .get("slack_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.slack_channel_filter = gateway
                .get("slack_channel_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.telegram_token = gateway
                .get("telegram_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.telegram_allowed_chats = gateway
                .get("telegram_allowed_chats")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_token = gateway
                .get("discord_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_channel_filter = gateway
                .get("discord_channel_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_allowed_users = gateway
                .get("discord_allowed_users")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_allowed_contacts = gateway
                .get("whatsapp_allowed_contacts")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_token = gateway
                .get("whatsapp_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_phone_id = gateway
                .get("whatsapp_phone_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }

        if let Some(tier_config) = json.get("tier") {
            let tier_str = tier_config
                .get("user_override")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    tier_config
                        .get("user_self_assessment")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("newcomer");
            self.tier.on_tier_changed(tier_str);
        }

        crate::agent_aliases::set_from_raw(&json);
        self.config.agent_config_raw = Some(json.clone());
        self.config
            .reduce(config::ConfigAction::ConfigRawReceived(json.clone()));
        self.refresh_openai_auth_status();
        self.refresh_snapshot_stats();
    }
}
