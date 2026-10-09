use super::*;
use zorai_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI};

#[path = "config_apply_compaction.rs"]
mod apply_compaction;
#[path = "config_apply_finish.rs"]
mod apply_finish;
#[path = "config_apply_general.rs"]
mod apply_general;
#[path = "config_apply_provider.rs"]
mod apply_provider;
#[path = "config_io_helpers.rs"]
mod helpers;

use helpers::{
    flatten_config_value, normalize_compliance_mode, normalize_provider_auth_source,
    normalize_provider_transport, openrouter_provider_list_value,
};

impl TuiModel {
    fn normalized_workspace_repo_monitor_dirs(raw: &str) -> Vec<String> {
        raw.lines()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    pub(in crate::app) fn sync_workspace_repo_monitor_to_daemon(
        &mut self,
        requested_enabled: bool,
    ) {
        let workspace_id = self.workspace.workspace_id().to_string();
        let include_dirs = Self::normalized_workspace_repo_monitor_dirs(
            &self.config.workspace_repo_monitor_include_dirs,
        );
        let exclude_dirs = Self::normalized_workspace_repo_monitor_dirs(
            &self.config.workspace_repo_monitor_exclude_dirs,
        );
        let enabled = requested_enabled && !include_dirs.is_empty();

        self.config.workspace_repo_monitor_enabled = enabled;

        if !self.connected {
            self.status_line =
                "Workspace repo monitor change not saved: daemon is disconnected".to_string();
            return;
        }

        self.send_daemon_command(DaemonCommand::SetWorkspaceRepoMonitor {
            workspace_id,
            repo_monitor_enabled: enabled,
            repo_monitor_include_dirs: include_dirs,
            repo_monitor_exclude_dirs: exclude_dirs,
        });

        self.status_line = if requested_enabled && !enabled {
            "Repo monitor stays disabled until at least one include directory is set".to_string()
        } else {
            "Workspace repo monitor updated".to_string()
        };
    }

    pub(in crate::app) fn apply_svarog_reasoning_effort_override(&mut self, effort: &str) {
        let reasoning_effort = effort.to_string();
        self.config.reduce(config::ConfigAction::SetReasoningEffort(
            reasoning_effort.clone(),
        ));

        if let Some(raw) = self.config.agent_config_raw.as_mut() {
            raw["reasoning_effort"] = serde_json::Value::String(reasoning_effort.clone());
            let provider_id = self.config.provider.clone();
            if let Some(provider) = raw
                .get_mut("providers")
                .and_then(|providers| providers.get_mut(&provider_id))
            {
                provider["reasoning_effort"] = serde_json::Value::String(reasoning_effort.clone());
            }
            if let Some(provider) = raw.get_mut(&provider_id) {
                provider["reasoning_effort"] = serde_json::Value::String(reasoning_effort.clone());
            }
        }
    }

    pub(in crate::app) fn set_pending_svarog_reasoning_effort(&mut self, effort: String) {
        self.pending_svarog_reasoning_effort = Some(effort.clone());
        self.apply_svarog_reasoning_effort_override(&effort);
    }

    pub(in crate::app) fn reapply_pending_svarog_reasoning_effort(&mut self) {
        let Some(effort) = self.pending_svarog_reasoning_effort.clone() else {
            return;
        };
        self.apply_svarog_reasoning_effort_override(&effort);
    }

    pub(in crate::app) fn reconcile_pending_svarog_reasoning_effort_after_raw_config(&mut self) {
        let Some(effort) = self.pending_svarog_reasoning_effort.clone() else {
            return;
        };
        if self.raw_config_svarog_effort_matches(&effort) {
            self.pending_svarog_reasoning_effort = None;
        } else {
            self.apply_svarog_reasoning_effort_override(&effort);
        }
    }

    fn raw_config_svarog_effort_matches(&self, effort: &str) -> bool {
        let Some(raw) = self.config.agent_config_raw.as_ref() else {
            return false;
        };
        if raw.get("reasoning_effort").and_then(|value| value.as_str()) != Some(effort) {
            return false;
        }
        let provider_id = self.config.provider.as_str();
        raw.get("providers")
            .and_then(|providers| providers.get(provider_id))
            .and_then(|provider| provider.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .map_or(true, |value| value == effort)
    }

    pub(super) fn sync_config_to_daemon(&mut self) {
        self.chat
            .set_history_page_size(self.config.tui_chat_history_page_size as usize);
        if !self.connected {
            self.status_line = "Config change not saved: daemon is disconnected".to_string();
            return;
        }
        if !self.agent_config_loaded {
            self.status_line = "Config change not saved yet: waiting for daemon config".to_string();
            return;
        }
        let before = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut after = self.build_config_patch_value();

        let mut before_items = Vec::new();
        flatten_config_value(&before, "", &mut before_items);
        let before_map = before_items
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();

        let mut after_items = Vec::new();
        flatten_config_value(&after, "", &mut after_items);
        let mut changed = 0usize;

        for (key_path, value) in after_items {
            if before_map.get(&key_path) == Some(&value) {
                continue;
            }
            if let Ok(value_json) = serde_json::to_string(&value) {
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path,
                    value_json,
                });
                changed += 1;
            }
        }

        if let (Some(before_providers), Some(after_providers)) = (
            before.get("providers").and_then(|value| value.as_object()),
            after
                .get_mut("providers")
                .and_then(|value| value.as_object_mut()),
        ) {
            for (provider_id, before_provider) in before_providers {
                if let Some(api_key) = before_provider.get("api_key").cloned() {
                    if let Some(after_provider) = after_providers
                        .get_mut(provider_id)
                        .and_then(|value| value.as_object_mut())
                    {
                        after_provider.insert("api_key".to_string(), api_key);
                    }
                }
            }
        }

        for provider in providers::PROVIDERS {
            if let Some(api_key) = before
                .get(provider.id)
                .and_then(|value| value.get("api_key"))
                .cloned()
            {
                if let Some(after_provider) = after
                    .get_mut(provider.id)
                    .and_then(|value| value.as_object_mut())
                {
                    after_provider.insert("api_key".to_string(), api_key);
                }
            }
        }

        self.config.agent_config_raw = Some(after);

        if changed == 0 {
            self.status_line = "No config changes to save".to_string();
        }
    }

    pub fn load_saved_settings(&mut self) {
        self.refresh_openai_auth_status();
        self.refresh_snapshot_stats();
    }

    pub(super) fn apply_config_json(&mut self, json: &serde_json::Value) {
        self.apply_provider_config_json(json);
        self.apply_general_config_json(json);
        self.apply_compaction_config_json(json);
        self.apply_snapshot_gateway_tier_config_json(json);
    }

    pub(super) fn save_settings(&self) {}
}

fn normalize_managed_security_level(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "highest" => "highest".to_string(),
        "moderate" => "moderate".to_string(),
        "yolo" => "yolo".to_string(),
        _ => "lowest".to_string(),
    }
}

#[cfg(test)]
#[path = "tests/config_io.rs"]
mod tests;
