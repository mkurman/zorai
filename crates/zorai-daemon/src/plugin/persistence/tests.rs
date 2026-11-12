use super::*;

async fn make_test_history() -> Arc<crate::history::HistoryStore> {
    let root = std::env::temp_dir().join(format!(
        "zorai-plugin-persist-test-{}",
        uuid::Uuid::new_v4()
    ));
    let store = crate::history::HistoryStore::new_test_store(&root)
        .await
        .unwrap();
    Arc::new(store)
}

fn sample_record(name: &str) -> PluginRecord {
    PluginRecord {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: Some("Test plugin".to_string()),
        author: Some("Test Author".to_string()),
        manifest_json: r#"{"name":"test","version":"1.0.0","schema_version":1}"#.to_string(),
        install_source: "local".to_string(),
        enabled: true,
        installed_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn list_plugins_returns_empty_on_fresh_db() {
    let persistence = PluginPersistence::new(make_test_history().await);
    let plugins = persistence.list_plugins().await.unwrap();
    assert!(plugins.is_empty());
}

#[tokio::test]
async fn upsert_then_list_returns_record() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("test-plugin"))
        .await
        .unwrap();

    let plugins = persistence.list_plugins().await.unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "test-plugin");
    assert_eq!(plugins[0].version, "1.0.0");
    assert!(plugins[0].enabled);
}

#[tokio::test]
async fn set_enabled_toggles_flag() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("test-plugin"))
        .await
        .unwrap();

    persistence.set_enabled("test-plugin", false).await.unwrap();
    let plugin = persistence
        .get_plugin("test-plugin")
        .await
        .unwrap()
        .unwrap();
    assert!(!plugin.enabled);

    persistence.set_enabled("test-plugin", true).await.unwrap();
    let plugin = persistence
        .get_plugin("test-plugin")
        .await
        .unwrap()
        .unwrap();
    assert!(plugin.enabled);
}

#[tokio::test]
async fn remove_stale_plugins_removes_absent_names() {
    let persistence = PluginPersistence::new(make_test_history().await);

    persistence
        .upsert_plugin(&sample_record("keep-me"))
        .await
        .unwrap();
    persistence
        .upsert_plugin(&sample_record("remove-me"))
        .await
        .unwrap();
    persistence
        .upsert_plugin(&sample_record("also-remove"))
        .await
        .unwrap();

    let deleted = persistence
        .remove_stale_plugins(&["keep-me".to_string()])
        .await
        .unwrap();
    assert_eq!(deleted, 2);

    let plugins = persistence.list_plugins().await.unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].name, "keep-me");
}

#[tokio::test]
async fn get_plugin_returns_none_for_missing() {
    let persistence = PluginPersistence::new(make_test_history().await);
    let result = persistence.get_plugin("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn remove_plugin_deletes_record() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("doomed-plugin"))
        .await
        .unwrap();

    assert!(persistence
        .get_plugin("doomed-plugin")
        .await
        .unwrap()
        .is_some());

    let existed = persistence.remove_plugin("doomed-plugin").await.unwrap();
    assert!(existed);
    assert!(persistence
        .get_plugin("doomed-plugin")
        .await
        .unwrap()
        .is_none());

    let existed_again = persistence.remove_plugin("doomed-plugin").await.unwrap();
    assert!(!existed_again);
}

#[tokio::test]
async fn plugin_persist_get_settings_empty() {
    let persistence = PluginPersistence::new(make_test_history().await);
    let settings = persistence.get_settings("nonexistent").await.unwrap();
    assert!(settings.is_empty());
}

#[tokio::test]
async fn plugin_persist_upsert_then_get_settings() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("test-plugin"))
        .await
        .unwrap();

    persistence
        .upsert_setting("test-plugin", "api_key", "my-secret", true)
        .await
        .unwrap();
    persistence
        .upsert_setting("test-plugin", "base_url", "https://api.example.com", false)
        .await
        .unwrap();

    let settings = persistence.get_settings("test-plugin").await.unwrap();
    assert_eq!(settings.len(), 2);
    assert_eq!(settings[0].0, "api_key");
    assert_eq!(settings[0].1, "my-secret");
    assert!(settings[0].2);
    assert_eq!(settings[1].0, "base_url");
    assert_eq!(settings[1].1, "https://api.example.com");
    assert!(!settings[1].2);
}

#[tokio::test]
async fn plugin_persist_upsert_updates_existing() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("test-plugin"))
        .await
        .unwrap();

    persistence
        .upsert_setting("test-plugin", "api_key", "old-value", false)
        .await
        .unwrap();
    persistence
        .upsert_setting("test-plugin", "api_key", "new-value", false)
        .await
        .unwrap();

    let settings = persistence.get_settings("test-plugin").await.unwrap();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].1, "new-value");
}

#[tokio::test]
async fn plugin_persist_secret_values_are_base64_encoded() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("test-plugin"))
        .await
        .unwrap();

    persistence
        .upsert_setting("test-plugin", "secret_key", "super-secret-123", true)
        .await
        .unwrap();

    let raw = persistence
        .history
        .conn
        .call(|conn| {
            let value: String = conn.query_row(
                "SELECT value FROM plugin_settings WHERE plugin_name = ?1 AND key = ?2",
                rusqlite::params!["test-plugin", "secret_key"],
                |row| row.get(0),
            )?;
            Ok(value)
        })
        .await
        .unwrap();

    use base64::Engine;
    let expected = base64::engine::general_purpose::STANDARD.encode("super-secret-123".as_bytes());
    assert_eq!(raw, expected);

    let settings = persistence.get_settings("test-plugin").await.unwrap();
    assert_eq!(settings[0].1, "super-secret-123");
}

#[tokio::test]
async fn upsert_updates_existing_record() {
    let persistence = PluginPersistence::new(make_test_history().await);

    let mut record = sample_record("test-plugin");
    persistence.upsert_plugin(&record).await.unwrap();

    record.version = "2.0.0".to_string();
    record.updated_at = "2026-06-01T00:00:00Z".to_string();
    persistence.upsert_plugin(&record).await.unwrap();

    let plugins = persistence.list_plugins().await.unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].version, "2.0.0");
    assert_eq!(plugins[0].updated_at, "2026-06-01T00:00:00Z");
}

#[tokio::test]
async fn auth_status_is_refreshable_when_access_token_expired_but_refresh_token_exists() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("oauth-plugin"))
        .await
        .unwrap();

    let expired_at = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    persistence
        .upsert_credential(
            "oauth-plugin",
            "access_token",
            b"encrypted-access",
            Some(&expired_at),
        )
        .await
        .unwrap();
    persistence
        .upsert_credential("oauth-plugin", "refresh_token", b"encrypted-refresh", None)
        .await
        .unwrap();

    let status = persistence.get_auth_status("oauth-plugin").await.unwrap();
    assert_eq!(status, "refreshable");
}

#[tokio::test]
async fn auth_status_needs_reconnect_when_access_token_expired_without_refresh_token() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("oauth-plugin"))
        .await
        .unwrap();

    let expired_at = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    persistence
        .upsert_credential(
            "oauth-plugin",
            "access_token",
            b"encrypted-access",
            Some(&expired_at),
        )
        .await
        .unwrap();

    let status = persistence.get_auth_status("oauth-plugin").await.unwrap();
    assert_eq!(status, "needs_reconnect");
}

#[tokio::test]
async fn auth_status_is_expiring_soon_when_access_token_nearly_expires_without_refresh_token() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("oauth-plugin"))
        .await
        .unwrap();

    let expiring_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    persistence
        .upsert_credential(
            "oauth-plugin",
            "access_token",
            b"encrypted-access",
            Some(&expiring_at),
        )
        .await
        .unwrap();

    let status = persistence.get_auth_status("oauth-plugin").await.unwrap();
    assert_eq!(status, "expiring_soon");
}

#[tokio::test]
async fn reinstall_records_the_new_source_but_startup_discovery_never_erases_a_known_one() {
    let persistence = PluginPersistence::new(make_test_history().await);
    persistence
        .upsert_plugin(&sample_record("calendar"))
        .await
        .unwrap();

    let mut reinstalled = sample_record("calendar");
    reinstalled.install_source = "local:/repo/plugins/zorai-plugin-gmail-calendar".to_string();
    reinstalled.version = "1.2.0".to_string();
    persistence.upsert_plugin(&reinstalled).await.unwrap();
    let plugin = persistence.get_plugin("calendar").await.unwrap().unwrap();
    assert_eq!(
        plugin.install_source,
        "local:/repo/plugins/zorai-plugin-gmail-calendar"
    );
    assert_eq!(plugin.version, "1.2.0");

    persistence
        .upsert_plugin(&sample_record("calendar"))
        .await
        .unwrap();
    let plugin = persistence.get_plugin("calendar").await.unwrap().unwrap();
    assert_eq!(
        plugin.install_source,
        "local:/repo/plugins/zorai-plugin-gmail-calendar"
    );
}
