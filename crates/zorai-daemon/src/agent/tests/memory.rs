use super::super::operator_profile::user_sync::{
    acquire_user_sync_test_guard, set_user_sync_state_for_test,
};
use super::*;
use crate::history::HistoryStore;

fn test_write_context() -> MemoryWriteContext<'static> {
    MemoryWriteContext {
        source_kind: "test",
        thread_id: None,
        task_id: None,
        goal_run_id: None,
    }
}

#[test]
fn validate_memory_size_rejects_over_limit() {
    let err = validate_memory_size(
        MemoryTarget::Soul,
        &"x".repeat(MemoryTarget::Soul.limit_chars() + 1),
    )
    .unwrap_err();
    assert!(err.to_string().contains("SOUL.md would exceed its limit"));
}

#[test]
fn memory_target_limits_match_policy_contract() {
    assert_eq!(MemoryTarget::Soul.limit_chars(), 2_000);
    assert_eq!(MemoryTarget::Memory.limit_chars(), 3_600);
    assert_eq!(MemoryTarget::User.limit_chars(), 1_800);
}

#[test]
fn configured_memory_limits_govern_writes_and_the_prompt_but_never_drop_below_floor() {
    set_memory_limits(&MemoryLimitsConfig {
        soul_chars: 5_000,
        ..MemoryLimitsConfig::default()
    });
    let roomy = validate_memory_size(MemoryTarget::Soul, &"x".repeat(4_000));
    let over = validate_memory_size(MemoryTarget::Soul, &"x".repeat(5_001));
    let prompt = memory_limits_prompt_line();
    set_memory_limits(&MemoryLimitsConfig {
        soul_chars: 10,
        ..MemoryLimitsConfig::default()
    });
    let floored = MemoryTarget::Soul.limit_chars();
    set_memory_limits(&MemoryLimitsConfig::default());

    assert!(roomy.is_ok());
    assert!(over
        .unwrap_err()
        .to_string()
        .contains("(5001 > 5000 chars)"));
    assert!(prompt.contains("SOUL.md 5000 chars"));
    assert_eq!(floored, 200);
}

#[test]
fn append_content_separates_blocks() {
    assert_eq!(append_content("alpha", "beta"), "alpha\n\nbeta");
}

#[test]
fn remove_content_requires_exact_match() {
    let err = remove_content("alpha", "beta").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn extract_fact_candidates_uses_subject_before_colon() {
    let facts = extract_memory_fact_candidates("- Shell: bash\n- editor: helix");
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "editor");
    assert_eq!(facts[1].key, "shell");
}

#[test]
fn extract_fact_candidates_ignore_distilled_prefix_tags() {
    let facts = extract_memory_fact_candidates(
        "- [distilled] Shell: bash\n- [Discord — mariuszkurman] editor: helix",
    );
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "editor");
    assert_eq!(facts[0].display, "editor: helix");
    assert_eq!(facts[1].key, "shell");
    assert_eq!(facts[1].display, "Shell: bash");
}

#[test]
fn contradiction_detection_blocks_conflicting_subject_fact() {
    let err =
        validate_no_memory_contradictions(MemoryTarget::User, "- shell: bash", "- shell: zsh")
            .unwrap_err();
    assert!(err.to_string().contains("Potential contradiction detected"));
}

#[test]
fn contradiction_detection_allows_matching_fact() {
    validate_no_memory_contradictions(
        MemoryTarget::Memory,
        "- package manager: cargo",
        "- package manager: cargo",
    )
    .expect("identical facts should not conflict");
}

#[test]
fn detect_memory_contradictions_returns_pairs_for_conflicting_facts() {
    let contradictions = detect_memory_contradictions("- shell: bash", "- shell: zsh");
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].0.key, "shell");
    assert_eq!(contradictions[0].0.display, "shell: bash");
    assert_eq!(contradictions[0].1.display, "shell: zsh");
}

#[test]
fn extract_fact_candidates_skips_superseded_blocks() {
    let facts = extract_memory_fact_candidates(
        "# Memory\n\
         - shell: bash\n\n\
         ## [SUPERSEDED]\n\
         - editor: vim\n\
         - uncertainty: superseded by editor: helix\n\n\
         - editor: helix\n",
    );

    assert!(facts.iter().any(|fact| fact.display == "shell: bash"));
    assert!(facts.iter().any(|fact| fact.display == "editor: helix"));
    assert!(!facts.iter().any(|fact| fact.display == "editor: vim"));
    assert!(!facts.iter().any(|fact| fact.key == "uncertainty"));
}

#[tokio::test]
async fn conflicting_memory_update_records_conflict_provenance_relationship() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let memory_path = active_memory_dir(&root).join(MemoryTarget::Memory.file_name());
    tokio::fs::write(&memory_path, "# Memory\n- shell: bash\n").await?;
    let baseline_fact_keys = vec!["shell".to_string()];
    history
        .record_memory_provenance(&crate::history::MemoryProvenanceRecord {
            id: "baseline-shell",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "- shell: bash",
            fact_keys: &baseline_fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 1,
            sign: true,
        })
        .await?;

    let result = apply_memory_update(
        &root,
        &history,
        MemoryTarget::Memory,
        MemoryUpdateMode::Append,
        "- shell: zsh",
        test_write_context(),
    )
    .await?;
    assert!(result.contains("Reconciled contradiction"));

    let final_content = tokio::fs::read_to_string(&memory_path).await?;
    assert!(final_content.contains("## [SUPERSEDED]"));
    assert!(final_content.contains("- shell: zsh"));

    let report = history
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let repair_entry = report
        .entries
        .iter()
        .find(|entry| entry.mode == "repaired_conflict")
        .expect("repair provenance entry should exist");
    assert_eq!(repair_entry.status, "active");
    assert_eq!(repair_entry.fact_keys, vec!["shell".to_string()]);
    assert!(repair_entry.hash_valid);
    assert!(repair_entry.signature_valid);

    let remove_entry = report
        .entries
        .iter()
        .find(|entry| entry.mode == "remove")
        .expect("repair should emit remove provenance entry");
    assert_eq!(remove_entry.relationships.len(), 1);
    assert_eq!(remove_entry.relationships[0].relation_type, "retracts");
    assert_eq!(
        remove_entry.relationships[0].related_entry_id,
        "baseline-shell"
    );

    Ok(())
}

#[tokio::test]
async fn append_memory_update_does_not_resupersede_inactive_fact() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let memory_path = active_memory_dir(&root).join(MemoryTarget::Memory.file_name());
    tokio::fs::write(
        &memory_path,
        "# Memory\n\n## [SUPERSEDED]\n- action: old cleanup plan\n- uncertainty: superseded by action: newer cleanup plan\n",
    )
    .await?;

    let result = apply_memory_update(
        &root,
        &history,
        MemoryTarget::Memory,
        MemoryUpdateMode::Append,
        "- action: review stale TODOs from the task inbox",
        test_write_context(),
    )
    .await?;

    assert!(result.contains("append mode"), "{result}");
    let final_content = tokio::fs::read_to_string(&memory_path).await?;
    assert_eq!(final_content.matches("## [SUPERSEDED]").count(), 1);
    assert!(final_content.contains("- action: review stale TODOs from the task inbox"));
    Ok(())
}

#[tokio::test]
async fn replace_memory_update_sizes_replacement_not_old_plus_new() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let memory_path = active_memory_dir(&root).join(MemoryTarget::Memory.file_name());
    let existing = format!(
        "# Memory\n- shell: bash\n{}",
        "existing stable context\n".repeat(160)
    );
    tokio::fs::write(&memory_path, existing).await?;

    let replacement = "# Memory\n- shell: zsh\n- editor: helix\n";
    let result = apply_memory_update(
        &root,
        &history,
        MemoryTarget::Memory,
        MemoryUpdateMode::Replace,
        replacement,
        test_write_context(),
    )
    .await?;

    assert!(result.contains("replace mode"), "{result}");
    let final_content = tokio::fs::read_to_string(&memory_path).await?;
    assert_eq!(final_content, replacement.trim());
    Ok(())
}

#[tokio::test]
async fn user_append_while_reconciling_stages_without_conflicting_file_write() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let user_path = active_memory_dir(&root).join(MemoryTarget::User.file_name());
    tokio::fs::write(&user_path, "# User\n- shell: bash\n").await?;

    set_user_sync_state_for_test(UserProfileSyncState::Reconciling);
    let _ = apply_memory_update(
        &root,
        &history,
        MemoryTarget::User,
        MemoryUpdateMode::Append,
        "- shell: zsh",
        test_write_context(),
    )
    .await?;

    let final_content = tokio::fs::read_to_string(&user_path).await?;
    assert!(final_content.contains("- shell: bash"));
    assert!(!final_content.contains("- shell: zsh"));

    let staged = history
        .get_profile_field("legacy_user_signal")
        .await?
        .expect("legacy append should be staged into profile fields");
    assert_eq!(staged.source, "legacy_append");
    Ok(())
}

#[tokio::test]
async fn user_append_is_staged_then_rerendered_from_db() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let user_path = active_memory_dir(&root).join(MemoryTarget::User.file_name());
    tokio::fs::write(&user_path, "# User\nlegacy note\n").await?;

    set_user_sync_state_for_test(UserProfileSyncState::Clean);
    let _ = apply_memory_update(
        &root,
        &history,
        MemoryTarget::User,
        MemoryUpdateMode::Append,
        "- prefers concise replies",
        test_write_context(),
    )
    .await?;

    let final_content = tokio::fs::read_to_string(&user_path).await?;
    assert!(
        final_content.contains("Profile summary is generated from SQLite-backed operator profile.")
    );
    assert!(
        final_content.contains("legacy note"),
        "imported legacy USER.md content must survive the DB round-trip: {final_content}"
    );
    assert!(
        final_content.contains("- prefers concise replies"),
        "appended signal must survive the DB round-trip: {final_content}"
    );
    assert!(
        !final_content.contains("\\n"),
        "rendered profile must be plain markdown, not JSON-escaped blobs: {final_content}"
    );

    let import_done = history
        .get_profile_field("__legacy_user_import_done")
        .await?;
    assert!(
        import_done.is_some(),
        "legacy bootstrap import sentinel should be written"
    );
    Ok(())
}

#[tokio::test]
async fn persona_scope_loads_local_memory_and_shared_user() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files_for_scope(&root, MAIN_AGENT_ID).await?;
    ensure_memory_files_for_scope(&root, RADOGOST_AGENT_ID).await?;

    let main_paths = memory_paths_for_scope(&root, MAIN_AGENT_ID);
    let persona_paths = memory_paths_for_scope(&root, RADOGOST_AGENT_ID);

    tokio::fs::write(&main_paths.user_path, "# User\n- prefers detailed audits\n").await?;
    tokio::fs::write(&persona_paths.soul_path, "# Identity\nRadogost persona\n").await?;
    tokio::fs::write(
        &persona_paths.memory_path,
        "# Memory\n- prefers tradeoff tables\n",
    )
    .await?;

    let loaded = load_memory_for_scope(&root, RADOGOST_AGENT_ID).await?;
    assert!(loaded.soul.contains("Radogost persona"));
    assert!(loaded.memory.contains("tradeoff tables"));
    assert!(loaded.user_profile.contains("prefers detailed audits"));

    let _ = history;
    Ok(())
}

#[tokio::test]
async fn custom_agent_scope_uses_legacy_agent_mission_persona_dir() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-memory-test-{}", Uuid::new_v4()));
    let agent_data_dir = root.join("agent");
    let legacy_memory_dir = root.join("agent-mission");
    tokio::fs::create_dir_all(&legacy_memory_dir).await?;
    tokio::fs::write(legacy_memory_dir.join("SOUL.md"), "# Identity\nSvarog\n").await?;
    tokio::fs::write(legacy_memory_dir.join("MEMORY.md"), "# Memory\nmain\n").await?;
    tokio::fs::write(legacy_memory_dir.join("USER.md"), "# User\nshared\n").await?;

    let custom_paths = memory_paths_for_scope(&agent_data_dir, "dola");
    assert_eq!(
        custom_paths.memory_dir,
        legacy_memory_dir.join("personas/dola")
    );
    assert_eq!(custom_paths.user_path, legacy_memory_dir.join("USER.md"));

    ensure_memory_files_for_scope(&agent_data_dir, "dola").await?;
    let custom_soul = tokio::fs::read_to_string(&custom_paths.soul_path).await?;
    assert!(custom_soul.contains("I'm dola"), "{custom_soul}");
    assert!(!custom_soul.contains("I'm Svarog"), "{custom_soul}");
    assert!(tokio::fs::try_exists(custom_paths.memory_path).await?);
    assert!(tokio::fs::try_exists(custom_paths.soul_path).await?);

    Ok(())
}
