use super::*;

fn leszy_config_key(field: &str) -> Option<&'static str> {
    match field {
        "leszy_provider" => Some("provider"),
        "leszy_model" => Some("model"),
        "leszy_reasoning_effort" => Some("reasoning_effort"),
        "leszy_base_url" => Some("base_url"),
        "leszy_system_prompt" => Some("system_prompt"),
        _ => None,
    }
}

impl TuiModel {
    pub(super) fn activate_leszy_weles_settings_field(&mut self, field: &str) -> bool {
        if field == "weles_agent_editor" {
            self.open_weles_agent_editor();
            return true;
        }
        let Some(key) = leszy_config_key(field) else {
            return false;
        };
        let current = self.leszy_config_value(key);
        if field == "leszy_reasoning_effort" {
            let efforts = ["", "minimal", "low", "medium", "high", "xhigh", "max"];
            let current_idx = efforts
                .iter()
                .position(|effort| *effort == current)
                .unwrap_or(0);
            let next = efforts[(current_idx + 1) % efforts.len()];
            self.set_leszy_config_item(field, next);
        } else {
            self.settings.start_editing(field, &current);
        }
        true
    }

    pub(crate) fn set_leszy_config_item(&mut self, field: &str, value: &str) {
        let Some(key) = leszy_config_key(field) else {
            return;
        };
        let value = if value.trim().is_empty() {
            serde_json::Value::Null
        } else if key == "system_prompt" {
            serde_json::Value::String(value.to_string())
        } else {
            serde_json::Value::String(value.trim().to_string())
        };
        self.set_raw_config_item(&["coder", key], value);
    }

    fn leszy_config_value(&self, key: &str) -> String {
        self.config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("coder"))
            .and_then(|coder| coder.get(key))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    }
}
