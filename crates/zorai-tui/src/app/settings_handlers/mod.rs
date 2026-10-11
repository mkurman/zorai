use super::*;
use zorai_shared::providers::*;

#[path = "activate_feature_settings_field_to_settings_field_click_uses_toggle.rs"]
mod activate_feature_settings_field_to_settings_field_click_uses_toggle;
#[path = "commit_subagent_editor_to_run_subagent_action.rs"]
mod commit_subagent_editor_to_run_subagent_action;
#[path = "embedding_dimensions.rs"]
mod embedding_dimensions;
#[path = "handle_honcho_settings_key_to_handle_subagent_settings_key.rs"]
mod handle_honcho_settings_key_to_handle_subagent_settings_key;
#[path = "image_remote_model_fetch_output_modalities_to_fetched_model_supports.rs"]
mod image_remote_model_fetch_output_modalities_to_fetched_model_supports;
#[path = "openrouter_endpoint_url_for_to_activate_settings_field.rs"]
mod openrouter_endpoint_url_for_to_activate_settings_field;
#[path = "toggle_settings_field_to_handle_plugins_settings_key.rs"]
mod toggle_settings_field_to_handle_plugins_settings_key;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use std::ffi::OsString;
    use std::path::PathBuf;
    use tokio::sync::mpsc::unbounded_channel;

    pub(crate) fn make_model() -> (
        TuiModel,
        tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    ) {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, daemon_rx) = unbounded_channel();
        (TuiModel::new(event_rx, daemon_tx), daemon_rx)
    }

    pub(crate) fn auth_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::auth::auth_test_env_lock().lock().unwrap()
    }

    pub(crate) struct EnvGuard {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        pub(crate) fn new(keys: &[&'static str]) -> Self {
            Self {
                saved: keys
                    .iter()
                    .map(|key| (*key, std::env::var_os(key)))
                    .collect(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }

    fn init_provider_auth_db(path: &std::path::Path) {
        let conn = Connection::open(path).expect("open auth db");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS provider_auth_state (
                provider_id TEXT NOT NULL,
                auth_mode   TEXT NOT NULL,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (provider_id, auth_mode)
            );
            ",
        )
        .expect("create auth schema");
    }

    pub(crate) fn write_provider_auth_row(
        path: &std::path::Path,
        provider_id: &str,
        auth_mode: &str,
    ) {
        init_provider_auth_db(path);
        let conn = Connection::open(path).expect("open auth db");
        conn.execute(
            "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                provider_id,
                auth_mode,
                "{\"token\":\"test\"}",
                1_i64
            ],
        )
        .expect("insert auth row");
    }

    pub(crate) fn has_provider_auth_row(
        path: &std::path::Path,
        provider_id: &str,
        auth_mode: &str,
    ) -> bool {
        init_provider_auth_db(path);
        let conn = Connection::open(path).expect("open auth db");
        conn.query_row(
            "SELECT 1 FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
            params![provider_id, auth_mode],
            |_row| Ok(()),
        )
        .is_ok()
    }

    pub(crate) fn unique_test_db_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("zorai-{name}-{nanos}.sqlite"))
    }

    #[path = "agent_alias_settings_fields.rs"]
    mod agent_alias_settings_fields;
    #[path = "collaboration_sessions_inspect_field_requests_collaboration_snapshot.rs"]
    mod collaboration_sessions_inspect_field_requests_collaboration_snapshot;
    #[path = "operator_model_inspect_field_requests_operator_model_snapshot_to_chat.rs"]
    mod operator_model_inspect_field_requests_operator_model_snapshot_to_chat;
    #[path = "tests_audio.rs"]
    mod tests_audio;
    #[path = "whatsapp_link_device_probes_status_before_starting_link_flow.rs"]
    mod whatsapp_link_device_probes_status_before_starting_link_flow;
}
