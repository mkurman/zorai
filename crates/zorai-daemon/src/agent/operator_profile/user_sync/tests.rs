use super::*;

fn test_guard() -> &'static std::sync::Mutex<()> {
    static GUARD: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    GUARD.get_or_init(|| std::sync::Mutex::new(()))
}

fn acquire_user_sync_test_guard() -> std::sync::MutexGuard<'static, ()> {
    match test_guard().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn current_user_sync_state() -> UserProfileSyncState {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned")
}

fn set_user_sync_state_for_test(state: UserProfileSyncState) {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned") = state;
}

fn test_agent_data_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join("agent")
}

#[tokio::test]
async fn stage_legacy_append_marks_dirty() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    set_user_sync_state_for_test(UserProfileSyncState::Clean);

    stage_legacy_user_memory_write(&history, "- prefers concise output").await?;

    assert_eq!(current_user_sync_state(), UserProfileSyncState::Dirty);
    let field = history
        .get_profile_field("legacy_user_signal")
        .await?
        .expect("legacy signal field should exist");
    assert_eq!(field.source, "legacy_append");
    Ok(())
}

#[tokio::test]
async fn reconcile_flattens_nested_generated_legacy_wrappers() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;

    let original = "## Operator\r\n\r\n- Background: ML scientist\r\n- Interests: agentic AI";
    let inner_generated = format!(
        "# User\n{}\n\n- legacy_user_md: {}\n- legacy_user_signal: {}",
        GENERATED_PROFILE_HEADER,
        serde_json::to_string(original)?,
        serde_json::to_string("- Operator preference: concise deltas only")?,
    );
    history
        .upsert_profile_field(
            "legacy_user_md",
            &serde_json::to_string(&inner_generated)?,
            0.30,
            "legacy_import",
        )
        .await?;
    history
        .upsert_profile_field("__legacy_user_import_done", "true", 1.0, "system")
        .await?;
    set_user_sync_state_for_test(UserProfileSyncState::Dirty);

    reconcile_user_profile_from_db(&agent_data_dir, &history).await?;
    let rendered = tokio::fs::read_to_string(memory_dir.join("USER.md")).await?;

    assert!(rendered.contains("## Operator"));
    assert!(rendered.contains("- Background: ML scientist"));
    assert!(rendered.contains("- Operator preference: concise deltas only"));
    assert!(
        !rendered.contains("legacy_user_md"),
        "nested generated wrappers must be flattened, not re-escaped: {rendered}"
    );
    assert!(
        !rendered.contains("\\r\\n"),
        "no escaped newlines may survive flattening: {rendered}"
    );
    Ok(())
}

#[tokio::test]
async fn bootstrap_skips_reimporting_generated_user_md() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(
        memory_dir.join("USER.md"),
        format!(
            "# User\n{}\n\n- some_field: \"value\"\n",
            GENERATED_PROFILE_HEADER
        ),
    )
    .await?;
    set_user_sync_state_for_test(UserProfileSyncState::Dirty);

    reconcile_user_profile_from_db(&agent_data_dir, &history).await?;

    assert!(
        history.get_profile_field("legacy_user_md").await?.is_none(),
        "a generated USER.md must never be re-imported as legacy content"
    );
    Ok(())
}

#[tokio::test]
async fn reconcile_renders_deterministic_user_md() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(memory_dir.join("USER.md"), "# User\nlegacy").await?;
    history
        .upsert_profile_field("preferred_name", "\"Milan\"", 1.0, "onboarding")
        .await?;
    history
        .upsert_profile_field("__legacy_user_import_done", "true", 1.0, "system")
        .await?;
    history
        .upsert_profile_field("communication_style", "\"direct\"", 1.0, "onboarding")
        .await?;
    set_user_sync_state_for_test(UserProfileSyncState::Dirty);

    reconcile_user_profile_from_db(&agent_data_dir, &history).await?;
    let rendered = tokio::fs::read_to_string(memory_dir.join("USER.md")).await?;
    let communication_index = rendered
        .find("- communication_style: \"direct\"")
        .expect("communication style should render");
    let preferred_name_index = rendered
        .find("- preferred_name: \"Milan\"")
        .expect("preferred name should render");
    assert!(rendered.contains("- preferred_name: \"Milan\""));
    assert!(
        !rendered.contains("__legacy_user_import_done"),
        "legacy import sentinel should be filtered by SQL before rendering"
    );
    assert!(
        communication_index < preferred_name_index,
        "profile fields should be SQL ordered by key"
    );
    assert_eq!(current_user_sync_state(), UserProfileSyncState::Clean);
    Ok(())
}

#[tokio::test]
async fn reconcile_sets_dirty_on_write_error() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::create_dir_all(memory_dir.join("USER.md")).await?;
    set_user_sync_state_for_test(UserProfileSyncState::Clean);

    let result = reconcile_user_profile_from_db(&agent_data_dir, &history).await;
    assert!(
        result.is_err(),
        "expected error because USER.md is a directory"
    );
    assert_eq!(
        current_user_sync_state(),
        UserProfileSyncState::Dirty,
        "state must be Dirty after reconcile error, not stuck in Reconciling"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_user_appends_both_stage_and_no_stuck_reconciling() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(memory_dir.join("USER.md"), "").await?;
    set_user_sync_state_for_test(UserProfileSyncState::Clean);

    let h1 = history.clone();
    let h2 = history.clone();
    let r1 = agent_data_dir.clone();
    let r2 = agent_data_dir.clone();

    let (res1, res2) = tokio::join!(
        handle_user_memory_append_with_reconcile(&r1, &h1, "prefers dark mode"),
        handle_user_memory_append_with_reconcile(&r2, &h2, "uses vim keybindings"),
    );
    res1?;
    res2?;

    let events = history.list_profile_events(20).await?;
    let append_count = events
        .iter()
        .filter(|e| e.event_type == "legacy_user_memory_append")
        .count();
    assert!(
        append_count >= 2,
        "both concurrent appends should be staged; got {append_count}"
    );
    assert_ne!(
        current_user_sync_state(),
        UserProfileSyncState::Reconciling,
        "state must not be stuck in Reconciling after concurrent appends"
    );
    Ok(())
}

#[tokio::test]
async fn staging_failure_after_acquire_resets_state_to_dirty() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(memory_dir.join("USER.md"), "").await?;
    set_user_sync_state_for_test(UserProfileSyncState::Clean);

    history
        .conn
        .call(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS operator_profile_fields")?;
            Ok(())
        })
        .await?;

    let result =
        handle_user_memory_append_with_reconcile(&agent_data_dir, &history, "should fail").await;
    assert!(
        result.is_err(),
        "expected staging to fail with no such table"
    );
    assert_eq!(
        current_user_sync_state(),
        UserProfileSyncState::Dirty,
        "state must be Dirty after staging failure, not stuck in Reconciling"
    );
    Ok(())
}

#[tokio::test]
async fn direct_reconcile_is_noop_when_already_reconciling() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(memory_dir.join("USER.md"), "SENTINEL").await?;
    set_user_sync_state_for_test(UserProfileSyncState::Reconciling);

    reconcile_user_profile_from_db(&agent_data_dir, &history).await?;

    let contents = tokio::fs::read_to_string(memory_dir.join("USER.md")).await?;
    assert_eq!(contents, "SENTINEL");
    assert_eq!(
        current_user_sync_state(),
        UserProfileSyncState::Reconciling,
        "state must remain Reconciling when direct reconcile was a no-op"
    );
    Ok(())
}

#[tokio::test]
async fn append_reconcile_write_error_keeps_db_updates_and_marks_dirty() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    tokio::fs::create_dir_all(&agent_data_dir).await?;
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::create_dir_all(memory_dir.join("USER.md")).await?;

    set_user_sync_state_for_test(UserProfileSyncState::Clean);
    let result =
        handle_user_memory_append_with_reconcile(&agent_data_dir, &history, "uses neovim").await;
    assert!(
        result.is_err(),
        "expected reconcile to fail when USER.md is a directory"
    );
    assert_eq!(
        current_user_sync_state(),
        UserProfileSyncState::Dirty,
        "sync state must be Dirty when USER.md sync fails"
    );

    let staged_field = history
        .get_profile_field("legacy_user_signal")
        .await?
        .expect("legacy_user_signal should be persisted even when USER.md write fails");
    assert_eq!(staged_field.source, "legacy_append");

    let events = history.list_profile_events(20).await?;
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "legacy_user_memory_append"),
        "legacy append event should be present even when USER.md write fails"
    );
    Ok(())
}

#[tokio::test]
async fn operator_user_md_edit_survives_the_next_profile_reconcile() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-user-sync-test-{}", uuid::Uuid::new_v4()));
    let history = crate::history::HistoryStore::new_test_store(&root).await?;
    let agent_data_dir = test_agent_data_dir(&root);
    let memory_dir = super::active_memory_dir(&agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;
    set_user_sync_state_for_test(UserProfileSyncState::Clean);

    history
        .upsert_profile_field(
            "legacy_user_signal",
            "\"- prefers tabs\"",
            0.55,
            "legacy_append",
        )
        .await?;
    history
        .upsert_profile_field("timezone", "\"Europe/Warsaw\"", 0.9, "operator_model")
        .await?;

    let edited = format!(
        "# User\n{GENERATED_PROFILE_HEADER}\n\n- prefers spaces\n- timezone: \"Europe/Warsaw\""
    );
    replace_user_profile_from_operator_edit(&agent_data_dir, &history, &edited).await?;
    reconcile_user_profile_from_db(&agent_data_dir, &history).await?;

    let rendered = tokio::fs::read_to_string(memory_dir.join("USER.md")).await?;
    assert!(rendered.contains("- prefers spaces"));
    assert!(
        !rendered.contains("prefers tabs"),
        "the replaced append must not come back: {rendered}"
    );
    assert_eq!(
        rendered.matches("timezone").count(),
        1,
        "structured fields must not be duplicated: {rendered}"
    );
    Ok(())
}
