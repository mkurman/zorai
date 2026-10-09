use super::schema_helpers::{ensure_column, table_has_column};
use std::path::Path;

const OFFLOADED_PAYLOADS_TABLE_SQL: &str = "CREATE TABLE offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL: &str =
    "CREATE TABLE IF NOT EXISTS offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_offloaded_payloads_thread_created ON offloaded_payloads(thread_id, created_at DESC)";

async fn offloaded_payloads_summary_is_required<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
) -> anyhow::Result<bool> {
    let summary_notnull = exec
        .query_opt(
            "SELECT \"notnull\" FROM pragma_table_info('offloaded_payloads') WHERE name = 'summary'",
            super::db::Params::None,
        )
        .await?
        .map(|row| row.get::<i64>(0))
        .transpose()?
        .unwrap_or(0);
    Ok(summary_notnull == 1)
}

fn canonical_offloaded_payload_storage_path(
    offloaded_payloads_dir: &Path,
    thread_id: &str,
    payload_id: &str,
) -> String {
    offloaded_payloads_dir
        .join(thread_id)
        .join(format!("{payload_id}.txt"))
        .to_string_lossy()
        .into_owned()
}

async fn rebuild_offloaded_payloads_table<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
    offloaded_payloads_dir: &Path,
) -> anyhow::Result<()> {
    exec.execute_batch(&format!(
        "ALTER TABLE offloaded_payloads RENAME TO offloaded_payloads_legacy;
         {OFFLOADED_PAYLOADS_TABLE_SQL};"
    ))
    .await?;

    let legacy_rows = exec
        .query(
            "SELECT payload_id, thread_id, tool_name, tool_call_id, content_type, byte_size, summary, created_at
             FROM offloaded_payloads_legacy",
            super::db::Params::None,
        )
        .await?;

    for row in &legacy_rows {
        let payload_id = row.get::<String>(0)?;
        let thread_id = row.get::<String>(1)?;
        let tool_name = row.get::<String>(2)?;
        let tool_call_id = row.get::<Option<String>>(3)?;
        let content_type = row.get::<String>(4)?;
        let byte_size = row.get::<i64>(5)?;
        let summary = row.get::<Option<String>>(6)?;
        let created_at = row.get::<i64>(7)?;
        let storage_path = canonical_offloaded_payload_storage_path(
            offloaded_payloads_dir,
            &thread_id,
            &payload_id,
        );
        exec.execute(
            "INSERT INTO offloaded_payloads (
                 payload_id,
                 thread_id,
                 tool_name,
                 tool_call_id,
                 storage_path,
                 content_type,
                 byte_size,
                 summary,
                 created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            super::db::db_params![
                payload_id,
                thread_id,
                tool_name,
                tool_call_id,
                storage_path,
                content_type,
                byte_size,
                summary.unwrap_or_default(),
                created_at
            ],
        )
        .await?;
    }

    exec.execute_batch(OFFLOADED_PAYLOADS_INDEX_SQL).await?;
    Ok(())
}

async fn ensure_offloaded_payloads_schema<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
    offloaded_payloads_dir: &Path,
) -> anyhow::Result<()> {
    exec.execute_batch(&format!("{OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL};"))
        .await?;
    if table_has_column(&mut *exec, "offloaded_payloads", "summary").await?
        && !offloaded_payloads_summary_is_required(&mut *exec).await?
    {
        rebuild_offloaded_payloads_table(&mut *exec, offloaded_payloads_dir).await?;
    }
    exec.execute_batch(&format!("{OFFLOADED_PAYLOADS_INDEX_SQL};"))
        .await?;
    Ok(())
}

pub(super) async fn ensure_context_archive_fts<E: super::db::DbExecutor + ?Sized>(exec: &mut E) {
    exec
        .execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS context_archive_fts USING fts5(summary, compressed_content, content=context_archive, content_rowid=rowid);",
        )
        .await
        .ok();
}

pub(super) async fn prepare_extended_schema_migrations<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
) -> anyhow::Result<()> {
    if table_has_column(&mut *exec, "external_runtime_profiles", "runtime").await? {
        ensure_column(
            &mut *exec,
            "external_runtime_profiles",
            "session_id",
            "TEXT",
        )
        .await?;
        ensure_column(
            &mut *exec,
            "external_runtime_profiles",
            "source_config_path",
            "TEXT",
        )
        .await?;
        ensure_column(
            &mut *exec,
            "external_runtime_profiles",
            "source_fingerprint",
            "TEXT",
        )
        .await?;
    }

    if table_has_column(&mut *exec, "workspace_settings", "workspace_id").await? {
        ensure_column(
            &mut *exec,
            "workspace_settings",
            "repo_monitor_enabled",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        ensure_column(
            &mut *exec,
            "workspace_settings",
            "repo_monitor_include_dirs_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        ensure_column(
            &mut *exec,
            "workspace_settings",
            "repo_monitor_exclude_dirs_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
    }

    Ok(())
}

pub(super) async fn apply_schema_migrations<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
    offloaded_payloads_dir: &Path,
) -> anyhow::Result<()> {
    ensure_offloaded_payloads_schema(&mut *exec, offloaded_payloads_dir).await?;
    ensure_column(&mut *exec, "approval_inbox", "gateway_surface", "TEXT").await?;
    ensure_column(&mut *exec, "approval_inbox", "gateway_channel", "TEXT").await?;
    ensure_column(&mut *exec, "approval_inbox", "gateway_thread", "TEXT").await?;
    ensure_column(&mut *exec, "approval_inbox", "rendered_prompt", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "external_runtime_profiles",
        "session_id",
        "TEXT",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "external_runtime_profiles",
        "source_config_path",
        "TEXT",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "external_runtime_profiles",
        "source_fingerprint",
        "TEXT",
    )
    .await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_runtime_import_sessions (
            session_id TEXT PRIMARY KEY,
            runtime TEXT NOT NULL,
            source_config_path TEXT NOT NULL,
            source_fingerprint TEXT NOT NULL,
            dry_run INTEGER NOT NULL DEFAULT 0,
            conflict_policy TEXT NOT NULL,
            source_surface TEXT NOT NULL,
            session_json TEXT NOT NULL,
            imported_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_runtime ON external_runtime_import_sessions(runtime, updated_at DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_fingerprint ON external_runtime_import_sessions(runtime, source_config_path, source_fingerprint, dry_run);
        CREATE TABLE IF NOT EXISTS imported_runtime_assets (
            asset_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            runtime TEXT NOT NULL,
            asset_kind TEXT NOT NULL,
            bucket TEXT NOT NULL,
            severity TEXT NOT NULL,
            recommended_action TEXT,
            source_path TEXT,
            source_fingerprint TEXT,
            conflict_policy TEXT NOT NULL,
            asset_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_session ON imported_runtime_assets(session_id, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_runtime_kind ON imported_runtime_assets(runtime, asset_kind, updated_at DESC);
        CREATE TABLE IF NOT EXISTS external_runtime_shadow_runs (
            run_id TEXT PRIMARY KEY,
            runtime TEXT NOT NULL,
            session_id TEXT NOT NULL,
            workflow TEXT NOT NULL,
            readiness_score INTEGER NOT NULL,
            blocker_count INTEGER NOT NULL,
            summary TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_runtime ON external_runtime_shadow_runs(runtime, created_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_session ON external_runtime_shadow_runs(session_id, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS embedding_jobs (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            chunk_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            workspace_id TEXT,
            thread_id TEXT,
            agent_id TEXT,
            source_timestamp INTEGER NOT NULL,
            queued_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            claimed_at INTEGER,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            PRIMARY KEY (source_kind, source_id, chunk_id)
        );
        CREATE TABLE IF NOT EXISTS embedding_job_completions (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            chunk_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            embedding_model TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            completed_at INTEGER NOT NULL,
            PRIMARY KEY (source_kind, source_id, chunk_id, embedding_model, dimensions)
        );
        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_updated ON embedding_jobs(updated_at ASC);
        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_claimed ON embedding_jobs(claimed_at, updated_at);
        CREATE TABLE IF NOT EXISTS embedding_deletions (
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            queued_at INTEGER NOT NULL,
            claimed_at INTEGER,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            PRIMARY KEY (source_kind, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_embedding_deletions_claimed ON embedding_deletions(claimed_at, queued_at);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS semantic_documents (
            source_kind       TEXT NOT NULL,
            root_path         TEXT NOT NULL,
            relative_path     TEXT NOT NULL,
            source_id         TEXT NOT NULL,
            title             TEXT NOT NULL,
            content_hash      TEXT NOT NULL,
            body              TEXT NOT NULL,
            discovered_at     INTEGER NOT NULL,
            updated_at        INTEGER NOT NULL,
            last_seen_at      INTEGER NOT NULL,
            deleted_at        INTEGER,
            PRIMARY KEY (source_kind, root_path, relative_path)
        );
        CREATE INDEX IF NOT EXISTS idx_semantic_documents_source ON semantic_documents(source_kind, source_id);
        CREATE INDEX IF NOT EXISTS idx_semantic_documents_seen ON semantic_documents(source_kind, root_path, last_seen_at);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS thread_structural_memory (
            thread_id TEXT PRIMARY KEY,
            state_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_thread_structural_memory_updated ON thread_structural_memory(updated_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_nodes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            node_type TEXT NOT NULL,
            embedding_blob BLOB,
            created_at_ms INTEGER NOT NULL,
            last_accessed_ms INTEGER NOT NULL,
            access_count INTEGER NOT NULL DEFAULT 0,
            summary_text TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_memory_nodes_type_accessed ON memory_nodes(node_type, last_accessed_ms DESC);
        CREATE TABLE IF NOT EXISTS memory_edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_node_id TEXT NOT NULL,
            target_node_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL NOT NULL DEFAULT 1.0,
            last_updated_ms INTEGER NOT NULL,
            FOREIGN KEY (source_node_id) REFERENCES memory_nodes(id),
            FOREIGN KEY (target_node_id) REFERENCES memory_nodes(id)
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_edges_unique ON memory_edges(source_node_id, target_node_id, relation_type);
        CREATE INDEX IF NOT EXISTS idx_memory_edges_source_updated ON memory_edges(source_node_id, last_updated_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_memory_edges_target_updated ON memory_edges(target_node_id, last_updated_ms DESC);"
    ).await?;
    ensure_column(&mut *exec, "agent_tasks", "session_id", "TEXT").await?;
    ensure_column(&mut *exec, "agent_threads", "metadata_json", "TEXT").await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_variant_history (
            id TEXT PRIMARY KEY,
            variant_id TEXT NOT NULL,
            recorded_at INTEGER NOT NULL,
            outcome TEXT NOT NULL,
            fitness_score REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_skill_variant_history_variant_ts ON skill_variant_history(variant_id, recorded_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_pool (
            parent_a TEXT NOT NULL,
            parent_b TEXT NOT NULL,
            offspring_id TEXT NOT NULL,
            lifecycle_state TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (parent_a, parent_b)
        );
        CREATE INDEX IF NOT EXISTS idx_gene_pool_offspring ON gene_pool(offspring_id, created_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_fitness_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            variant_id TEXT NOT NULL,
            recorded_at_ms INTEGER NOT NULL,
            fitness_score REAL NOT NULL,
            use_count INTEGER NOT NULL,
            success_rate REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_gene_fitness_variant_ts ON gene_fitness_history(variant_id, recorded_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS gene_crossbreeds (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            left_parent_variant_id TEXT NOT NULL,
            right_parent_variant_id TEXT NOT NULL,
            skill_name TEXT NOT NULL,
            co_usage_rate REAL NOT NULL,
            proposed_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_gene_crossbreeds_skill_ts ON gene_crossbreeds(skill_name, proposed_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS morphogenesis_affinities (
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            affinity_score REAL NOT NULL DEFAULT 0.0,
            task_count INTEGER NOT NULL DEFAULT 0,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            last_updated_ms INTEGER NOT NULL,
            PRIMARY KEY (agent_id, domain)
        );
        CREATE INDEX IF NOT EXISTS idx_morphogenesis_domain_updated ON morphogenesis_affinities(domain, last_updated_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS affinity_updates_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            old_affinity REAL NOT NULL,
            new_affinity REAL NOT NULL,
            trigger_type TEXT NOT NULL,
            task_id TEXT,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_affinity_updates_agent_domain_ts ON affinity_updates_log(agent_id, domain, updated_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS soul_adaptations_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            domain TEXT NOT NULL,
            adaptation_type TEXT NOT NULL,
            soul_snippet TEXT NOT NULL,
            old_soul_hash TEXT,
            new_soul_hash TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_soul_adaptations_agent_ts ON soul_adaptations_log(agent_id, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_bid_priors (
            role TEXT PRIMARY KEY,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            prior_score REAL NOT NULL DEFAULT 0.5,
            last_updated_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_bid_priors_updated ON consensus_bid_priors(last_updated_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_bids (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            round_id INTEGER NOT NULL,
            agent_id TEXT NOT NULL,
            confidence REAL NOT NULL,
            reasoning TEXT,
            availability TEXT NOT NULL,
            domain_affinity REAL NOT NULL DEFAULT 0.0,
            submitted_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_bids_task_round ON consensus_bids(task_id, round_id, submitted_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS role_assignments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            round_id INTEGER NOT NULL,
            primary_agent_id TEXT NOT NULL,
            reviewer_agent_id TEXT,
            observers TEXT NOT NULL DEFAULT '[]',
            assigned_at_ms INTEGER NOT NULL,
            outcome TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_role_assignments_task_round ON role_assignments(task_id, round_id, assigned_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS consensus_quality_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            predicted_confidence REAL NOT NULL,
            actual_outcome_score REAL NOT NULL,
            prediction_error REAL NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_consensus_quality_task_ts ON consensus_quality_metrics(task_id, updated_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS cognitive_resonance_samples (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            sampled_at_ms INTEGER NOT NULL,
            revision_velocity_ms INTEGER,
            session_entropy REAL,
            approval_latency_ms INTEGER,
            tool_hesitation_count INTEGER NOT NULL DEFAULT 0,
            cognitive_state TEXT NOT NULL,
            state_confidence REAL NOT NULL,
            resonance_score REAL NOT NULL,
            verbosity_adjustment REAL NOT NULL,
            risk_adjustment REAL NOT NULL,
            proactiveness_adjustment REAL NOT NULL,
            memory_urgency_adjustment REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_cognitive_resonance_samples_sampled ON cognitive_resonance_samples(sampled_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS behavior_adjustments_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            adjusted_at_ms INTEGER NOT NULL,
            parameter TEXT NOT NULL,
            old_value REAL NOT NULL,
            new_value REAL NOT NULL,
            trigger_reason TEXT NOT NULL,
            resonance_score REAL NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_behavior_adjustments_log_adjusted ON behavior_adjustments_log(adjusted_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS intent_models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL UNIQUE,
            model_blob BLOB,
            created_at_ms INTEGER NOT NULL,
            accuracy_score REAL
        );
        CREATE INDEX IF NOT EXISTS idx_intent_models_agent_created ON intent_models(agent_id, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS temporal_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_type TEXT NOT NULL,
            timescale TEXT NOT NULL,
            pattern_description TEXT NOT NULL,
            context_filter TEXT,
            frequency INTEGER NOT NULL DEFAULT 1,
            last_observed_ms INTEGER NOT NULL,
            first_observed_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            decay_rate REAL NOT NULL DEFAULT 0.01,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_temporal_patterns_type_scale ON temporal_patterns(pattern_type, timescale, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS temporal_predictions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_id INTEGER NOT NULL,
            predicted_action TEXT NOT NULL,
            predicted_at_ms INTEGER NOT NULL,
            confidence REAL NOT NULL,
            actual_action TEXT,
            was_accepted INTEGER,
            accuracy_score REAL,
            FOREIGN KEY (pattern_id) REFERENCES temporal_patterns(id)
        );
        CREATE INDEX IF NOT EXISTS idx_temporal_predictions_pattern_predicted ON temporal_predictions(pattern_id, predicted_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS precomputation_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            prediction_id INTEGER NOT NULL,
            precomputation_type TEXT NOT NULL,
            precomputation_details TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            was_used INTEGER,
            FOREIGN KEY (prediction_id) REFERENCES temporal_predictions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_precomputation_log_prediction_started ON precomputation_log(prediction_id, started_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS dream_cycles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            idle_duration_ms INTEGER NOT NULL,
            tasks_analyzed INTEGER NOT NULL,
            counterfactuals_generated INTEGER NOT NULL,
            counterfactuals_successful INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'running'
        );
        CREATE INDEX IF NOT EXISTS idx_dream_cycles_started ON dream_cycles(started_at_ms DESC);",
    )
    .await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS counterfactual_evaluations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dream_cycle_id INTEGER NOT NULL,
            source_task_id TEXT NOT NULL,
            variation_type TEXT NOT NULL,
            counterfactual_description TEXT NOT NULL,
            estimated_token_saving REAL,
            estimated_time_saving_ms INTEGER,
            estimated_revision_reduction INTEGER,
            score REAL NOT NULL,
            threshold_met INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY (dream_cycle_id) REFERENCES dream_cycles(id)
        );
        CREATE INDEX IF NOT EXISTS idx_counterfactual_evaluations_cycle ON counterfactual_evaluations(dream_cycle_id, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS event_triggers (
            id TEXT PRIMARY KEY,
            event_family TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            agent_id TEXT,
            target_state TEXT,
            thread_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            cooldown_secs INTEGER NOT NULL DEFAULT 0,
            risk_label TEXT NOT NULL DEFAULT 'low',
            notification_kind TEXT NOT NULL,
            prompt_template TEXT,
            tool_name TEXT,
            tool_payload_json TEXT,
            title_template TEXT NOT NULL,
            body_template TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_fired_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_event_triggers_family_kind_enabled ON event_triggers(event_family, event_kind, enabled, updated_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_wakeups (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            message TEXT NOT NULL,
            interval_ms INTEGER NOT NULL,
            next_fire_at INTEGER NOT NULL,
            repetitions_remaining INTEGER,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_agent_wakeups_next_fire ON agent_wakeups(next_fire_at ASC);",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "agent_wakeups",
        "wakeup_kind",
        "TEXT NOT NULL DEFAULT 'generic'",
    )
    .await?;
    ensure_column(&mut *exec, "agent_wakeups", "goal_run_id", "TEXT").await?;
    exec.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_agent_wakeups_goal_run ON agent_wakeups(goal_run_id, next_fire_at ASC);",
    )
    .await?;
    let legacy_goal_wakeups = exec
        .query(
            "SELECT id, message FROM agent_wakeups WHERE goal_run_id IS NULL AND repetitions_remaining IS NULL AND message GLOB 'Supervise goal_*'",
            super::db::Params::None,
        )
        .await?;
    for row in &legacy_goal_wakeups {
        let id = row.get::<String>(0)?;
        let message = row.get::<String>(1)?;
        let goal_run_id = message
            .strip_prefix("Supervise ")
            .and_then(|message| message.split_whitespace().next())
            .map(|word| {
                word.trim_end_matches(|character: char| {
                    !character.is_ascii_alphanumeric() && character != '_' && character != '-'
                })
            })
            .filter(|word| word.starts_with("goal_") && word.len() > "goal_".len())
            .map(str::to_string);
        if let Some(goal_run_id) = goal_run_id {
            exec.execute(
                "UPDATE agent_wakeups SET wakeup_kind = 'goal_supervision', goal_run_id = ?1, repetitions_remaining = 1 WHERE id = ?2",
                super::db::db_params![goal_run_id, id],
            )
            .await?;
        }
    }
    ensure_column(&mut *exec, "event_triggers", "agent_id", "TEXT").await?;
    ensure_column(&mut *exec, "event_triggers", "prompt_template", "TEXT").await?;
    ensure_column(&mut *exec, "event_triggers", "tool_name", "TEXT").await?;
    ensure_column(&mut *exec, "event_triggers", "tool_payload_json", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "event_triggers",
        "max_retries",
        "INTEGER NOT NULL DEFAULT 3",
    )
    .await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_definitions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            paused_at INTEGER,
            schedule_expression TEXT NOT NULL,
            target_kind TEXT NOT NULL,
            target_payload_json TEXT NOT NULL,
            schema_version INTEGER NOT NULL DEFAULT 1,
            next_run_at INTEGER,
            last_run_at INTEGER,
            last_result TEXT,
            last_error TEXT,
            last_success_summary TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_routine_definitions_enabled_next_run ON routine_definitions(enabled, next_run_at, updated_at DESC);",
    ).await?;
    ensure_column(
        &mut *exec,
        "routine_definitions",
        "schema_version",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    ensure_column(&mut *exec, "routine_definitions", "last_result", "TEXT").await?;
    ensure_column(&mut *exec, "routine_definitions", "last_error", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "routine_definitions",
        "last_success_summary",
        "TEXT",
    )
    .await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS routine_runs (
            id TEXT PRIMARY KEY,
            routine_id TEXT NOT NULL,
            trigger_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            created_task_id TEXT,
            created_goal_run_id TEXT,
            payload_json TEXT NOT NULL,
            result_summary TEXT,
            error TEXT,
            rerun_of_run_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_routine_runs_routine_started ON routine_runs(routine_id, started_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS event_log (
            id TEXT PRIMARY KEY,
            event_family TEXT NOT NULL,
            event_kind TEXT NOT NULL,
            state TEXT,
            thread_id TEXT,
            payload_json TEXT NOT NULL,
            risk_label TEXT NOT NULL,
            handled_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_event_log_family_kind_ts ON event_log(event_family, event_kind, handled_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_graph_clusters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            summary_text TEXT,
            center_node_id TEXT,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_graph_clusters_center ON memory_graph_clusters(center_node_id, created_at_ms DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_cluster_members (
            cluster_id INTEGER NOT NULL,
            node_id TEXT NOT NULL,
            PRIMARY KEY (cluster_id, node_id)
        );
        CREATE INDEX IF NOT EXISTS idx_memory_cluster_members_node ON memory_cluster_members(node_id, cluster_id);",
    ).await?;
    ensure_column(
        &mut *exec,
        "skill_variants",
        "fitness_score",
        "REAL NOT NULL DEFAULT 0",
    )
    .await?;
    exec.execute(
        "UPDATE skill_variants SET fitness_score = CAST(success_count AS REAL) - CAST(failure_count AS REAL) WHERE fitness_score = 0", super::db::Params::None).await?;
    ensure_column(&mut *exec, "agent_threads", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "agent_messages", "cost_usd", "REAL").await?;
    ensure_column(&mut *exec, "agent_messages", "deleted_at", "INTEGER").await?;
    exec.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_thread_deleted_created ON agent_messages(thread_id, deleted_at, created_at, id)", super::db::Params::None).await?;
    ensure_column(&mut *exec, "agent_tasks", "scheduled_at", "INTEGER").await?;
    ensure_column(&mut *exec, "agent_tasks", "goal_run_id", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "goal_run_title", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "goal_step_id", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "goal_step_title", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "parent_task_id", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "parent_thread_id", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "agent_tasks",
        "runtime",
        "TEXT NOT NULL DEFAULT 'daemon'",
    )
    .await?;
    ensure_column(&mut *exec, "agent_tasks", "override_provider", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "override_model", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "override_api_transport", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "override_system_prompt", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "sub_agent_def_id", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "tool_whitelist_json", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "tool_blacklist_json", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "agent_tasks",
        "context_budget_tokens",
        "INTEGER",
    )
    .await?;
    ensure_column(&mut *exec, "agent_tasks", "context_overflow_action", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "termination_conditions", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "success_criteria", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "max_duration_secs", "INTEGER").await?;
    ensure_column(&mut *exec, "agent_tasks", "supervisor_config_json", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "agent_tasks",
        "completion_contract_json",
        "TEXT",
    )
    .await?;
    ensure_column(&mut *exec, "agent_tasks", "deleted_at", "INTEGER").await?;
    ensure_column(
        &mut *exec,
        "agent_task_dependencies",
        "deleted_at",
        "INTEGER",
    )
    .await?;
    ensure_column(&mut *exec, "agent_task_logs", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "agent_config_items", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "provider_auth_state", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "plugins", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "plugin_settings", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "plugin_credentials", "deleted_at", "INTEGER").await?;
    for table in [
        "command_log",
        "snapshot_index",
        "agent_checkpoints",
        "gateway_threads",
        "gateway_channel_modes",
        "whatsapp_provider_state",
        "operator_profile_sessions",
        "action_audit",
        "memory_tombstones",
        "consolidation_state",
        "offloaded_payloads",
        "thread_structural_memory",
        "routine_definitions",
        "memory_cluster_members",
        "cognitive_biases",
        "workflow_profiles",
        "protocol_steps",
    ] {
        ensure_column(&mut *exec, table, "deleted_at", "INTEGER").await?;
    }
    ensure_column(
        &mut *exec,
        "memory_distillation_log",
        "source_message_span_json",
        "TEXT",
    )
    .await?;
    ensure_column(&mut *exec, "agent_tasks", "policy_fingerprint", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "approval_expires_at", "INTEGER").await?;
    ensure_column(&mut *exec, "agent_tasks", "containment_scope", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "compensation_status", "TEXT").await?;
    ensure_column(&mut *exec, "agent_tasks", "compensation_summary", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "client_request_id", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "failure_cause", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "stopped_reason", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "child_task_count",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "approval_count",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "awaiting_approval_id", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "policy_fingerprint", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "approval_expires_at", "INTEGER").await?;
    ensure_column(&mut *exec, "goal_runs", "containment_scope", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "compensation_status", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "compensation_summary", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "active_task_id", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "duration_ms", "INTEGER").await?;
    ensure_column(&mut *exec, "goal_runs", "dossier_json", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "total_prompt_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "total_completion_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "estimated_cost_usd", "REAL").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "model_usage_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "autonomy_level",
        "TEXT NOT NULL DEFAULT 'aware'",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "authorship_tag", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "planner_owner_profile_json",
        "TEXT",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "current_step_owner_profile_json",
        "TEXT",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "launch_assignment_snapshot_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "runtime_assignment_list_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "root_thread_id", "TEXT").await?;
    ensure_column(&mut *exec, "goal_runs", "active_thread_id", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "execution_thread_ids_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "supervision_thread_id", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "goal_runs",
        "step_failure_history_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(&mut *exec, "goal_runs", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "goal_run_steps", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "goal_run_events", "step_index", "INTEGER").await?;
    ensure_column(&mut *exec, "goal_run_events", "todo_snapshot_json", "TEXT").await?;
    ensure_column(&mut *exec, "goal_run_events", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "action_audit", "user_action", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "causal_traces",
        "trace_family",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    exec.execute(
        "CREATE INDEX IF NOT EXISTS idx_causal_traces_family ON causal_traces(trace_family, created_at DESC)", super::db::Params::None).await?;
    ensure_column(
        &mut *exec,
        "memory_provenance",
        "entry_hash",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(&mut *exec, "memory_provenance", "signature", "TEXT").await?;
    ensure_column(&mut *exec, "memory_provenance", "signature_scheme", "TEXT").await?;
    ensure_column(&mut *exec, "memory_provenance", "confirmed_at", "INTEGER").await?;
    ensure_column(&mut *exec, "memory_provenance", "retracted_at", "INTEGER").await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_provenance_relationships (
            id TEXT PRIMARY KEY,
            source_entry_id TEXT NOT NULL,
            target_entry_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            fact_key TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_provenance_rel_unique ON memory_provenance_relationships(source_entry_id, target_entry_id, relation_type, fact_key);
        CREATE INDEX IF NOT EXISTS idx_memory_provenance_rel_source ON memory_provenance_relationships(source_entry_id, created_at DESC);",
    ).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS collaboration_agent_outcomes (
            parent_task_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            success_count INTEGER NOT NULL DEFAULT 0,
            failure_count INTEGER NOT NULL DEFAULT 0,
            learned_score REAL NOT NULL DEFAULT 0.5,
            last_outcome TEXT,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (parent_task_id, task_id)
        );
        CREATE INDEX IF NOT EXISTS idx_collaboration_agent_outcomes_parent_updated ON collaboration_agent_outcomes(parent_task_id, updated_at_ms DESC);",
    ).await?;
    exec.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run ON agent_tasks(goal_run_id, created_at DESC)", super::db::Params::None).await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_settings (
            workspace_id TEXT PRIMARY KEY,
            workspace_root TEXT,
            operator TEXT NOT NULL,
            repo_monitor_enabled INTEGER NOT NULL DEFAULT 0,
            repo_monitor_include_dirs_json TEXT NOT NULL DEFAULT '[]',
            repo_monitor_exclude_dirs_json TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS workspace_tasks (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            title TEXT NOT NULL,
            task_type TEXT NOT NULL,
            description TEXT NOT NULL,
            definition_of_done TEXT,
            priority TEXT NOT NULL,
            status TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            reporter_json TEXT NOT NULL,
            assignee_json TEXT,
            reviewer_json TEXT,
            thread_id TEXT,
            goal_run_id TEXT,
            runtime_history_json TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            started_at INTEGER,
            completed_at INTEGER,
            deleted_at INTEGER,
            last_notice_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_tasks_visible ON workspace_tasks(workspace_id, deleted_at, status, sort_order);
        CREATE TABLE IF NOT EXISTS workspace_notices (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            notice_type TEXT NOT NULL,
            message TEXT NOT NULL,
            actor_json TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_notices_task ON workspace_notices(workspace_id, task_id, created_at DESC);",
    ).await?;
    ensure_column(
        &mut *exec,
        "workspace_tasks",
        "runtime_history_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "workspace_settings",
        "repo_monitor_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "workspace_settings",
        "repo_monitor_include_dirs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "workspace_settings",
        "repo_monitor_exclude_dirs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    exec.execute(
        "CREATE INDEX IF NOT EXISTS idx_workspace_settings_repo_monitor_enabled
         ON workspace_settings(repo_monitor_enabled, workspace_id)
         WHERE repo_monitor_enabled = 1",
        super::db::Params::None,
    )
    .await?;
    crate::agent::episodic::schema::init_episodic_schema(&mut *exec).await?;
    crate::agent::handoff::schema::init_handoff_schema(&mut *exec).await?;
    ensure_column(&mut *exec, "browser_profiles", "browser_kind", "TEXT").await?;
    ensure_column(&mut *exec, "browser_profiles", "workspace_id", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "browser_profiles",
        "health_state",
        "TEXT NOT NULL DEFAULT 'healthy'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "browser_profiles",
        "last_auth_success_at",
        "INTEGER",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "browser_profiles",
        "last_auth_failure_at",
        "INTEGER",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "browser_profiles",
        "last_auth_failure_reason",
        "TEXT",
    )
    .await?;
    exec.execute_batch(
        "CREATE TABLE IF NOT EXISTS trigger_fire_history (
            id               TEXT PRIMARY KEY,
            trigger_id       TEXT NOT NULL,
            event_family     TEXT NOT NULL,
            event_kind       TEXT NOT NULL,
            status           TEXT NOT NULL DEFAULT 'fired',
            fired_at_ms      INTEGER NOT NULL,
            completed_at_ms  INTEGER,
            retry_count      INTEGER NOT NULL DEFAULT 0,
            error_message    TEXT,
            created_task_id  TEXT,
            notice_id        TEXT,
            payload_json     TEXT NOT NULL DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_trigger_fire_history_trigger_fired ON trigger_fire_history(trigger_id, fired_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_trigger_fire_history_status ON trigger_fire_history(status, fired_at_ms DESC);",
    ).await?;

    exec.execute_batch(
        "-- Per-agent thread picker fetch
        --   matches WHERE lower(trim(t.agent_name)) IN (?, ...) verbatim.
        CREATE INDEX IF NOT EXISTS idx_threads_agent_name_norm
            ON agent_threads(lower(trim(agent_name)), updated_at DESC)
            WHERE deleted_at IS NULL;
        -- Default visible thread lists:
        --   WHERE deleted_at IS NULL ORDER BY updated_at DESC, id ASC.
        CREATE INDEX IF NOT EXISTS idx_threads_visible_updated
            ON agent_threads(updated_at DESC, id)
            WHERE deleted_at IS NULL;

        -- Role-aware latest-message lookups (stalled-turn recovery,
        -- latest_user_message_content, latest_assistant_message).
        CREATE INDEX IF NOT EXISTS idx_messages_thread_role_created
            ON agent_messages(thread_id, role, created_at DESC, id DESC)
            WHERE deleted_at IS NULL;

        -- agent_tasks per-thread/per-goal/per-task lookups.
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_thread
            ON agent_tasks(thread_id, created_at DESC)
            WHERE deleted_at IS NULL AND thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run
            ON agent_tasks(goal_run_id, created_at DESC)
            WHERE deleted_at IS NULL AND goal_run_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run_status_quiet
            ON agent_tasks(goal_run_id, status, id)
            WHERE deleted_at IS NULL AND goal_run_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_parent_task
            ON agent_tasks(parent_task_id)
            WHERE deleted_at IS NULL AND parent_task_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_parent_thread
            ON agent_tasks(parent_thread_id)
            WHERE deleted_at IS NULL AND parent_thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_parent_thread_subagent_status
            ON agent_tasks(parent_thread_id, status, priority, created_at DESC)
            WHERE deleted_at IS NULL AND source = 'subagent' AND parent_thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_awaiting_approval
            ON agent_tasks(awaiting_approval_id)
            WHERE deleted_at IS NULL AND awaiting_approval_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_subagent_def
            ON agent_tasks(sub_agent_def_id, created_at DESC)
            WHERE deleted_at IS NULL AND sub_agent_def_id IS NOT NULL;

        -- goal_runs per-thread/session/client-request lookups.
        CREATE INDEX IF NOT EXISTS idx_goal_runs_thread
            ON goal_runs(thread_id, updated_at DESC)
            WHERE deleted_at IS NULL AND thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_goal_runs_active_thread
            ON goal_runs(active_thread_id, updated_at DESC)
            WHERE deleted_at IS NULL AND active_thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_goal_runs_root_thread
            ON goal_runs(root_thread_id, updated_at DESC)
            WHERE deleted_at IS NULL AND root_thread_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_goal_runs_session
            ON goal_runs(session_id, updated_at DESC)
            WHERE deleted_at IS NULL AND session_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_goal_runs_client_request_id
            ON goal_runs(client_request_id)
            WHERE deleted_at IS NULL AND client_request_id IS NOT NULL;
        -- Powers the concierge welcome's `latest goal_run` lookup
        -- (`SELECT id FROM goal_runs WHERE deleted_at IS NULL
        -- ORDER BY updated_at DESC LIMIT 1`). Without this, SQLite scans
        -- and sorts the whole table — fine for tens of rows, lethal once
        -- the goal-runs log accumulates.
        CREATE INDEX IF NOT EXISTS idx_goal_runs_active_updated
            ON goal_runs(updated_at DESC)
            WHERE deleted_at IS NULL;

        -- goal_run_steps lookups by goal_run_id are filtered by `deleted_at
        -- IS NULL` everywhere; the existing non-partial index forced SQLite
        -- to scan tombstoned rows when the page query joined on goal_run_id.
        CREATE INDEX IF NOT EXISTS idx_goal_run_steps_active_goal
            ON goal_run_steps(goal_run_id, ordinal)
            WHERE deleted_at IS NULL;

        -- goal_run_events lookups (welcome path's latest-step recovery and
        -- list_goal_runs_page's per-goal event fan-in).
        CREATE INDEX IF NOT EXISTS idx_goal_run_events_active_goal_ts
            ON goal_run_events(goal_run_id, timestamp DESC)
            WHERE deleted_at IS NULL;

        -- Powers `goal_run_policy_context`'s aggregate
        -- `SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END)`. Including
        -- `status` in the index column list lets the planner satisfy the
        -- aggregate without fetching the row body for each step. On goal
        -- runs with hundreds of steps this changes the COUNT/SUM from a
        -- table-row read per step into an index-only walk.
        CREATE INDEX IF NOT EXISTS idx_goal_run_steps_active_goal_status
            ON goal_run_steps(goal_run_id, status)
            WHERE deleted_at IS NULL;

        -- `get_emergent_protocol_by_pattern` does
        --   WHERE thread_id = ? AND normalized_pattern = ?
        --   ORDER BY activated_at DESC LIMIT 1
        -- Existing indexes are single-column on either thread_id OR pattern
        -- (with activated_at DESC) — the planner had to scan one and filter
        -- by the other. The composite makes both equality predicates a
        -- direct prefix match and lets the index satisfy the LIMIT 1 via a
        -- single seek.
        CREATE INDEX IF NOT EXISTS idx_emergent_protocols_thread_pattern_activated
            ON emergent_protocols(thread_id, normalized_pattern, activated_at DESC);

        -- `get_latest_workspace_task_for_thread` does
        --   WHERE thread_id = ? AND deleted_at IS NULL
        --   ORDER BY updated_at DESC LIMIT 1
        -- The existing `idx_workspace_tasks_visible` is keyed on
        -- workspace_id, not thread_id, so per-thread latest-task lookups
        -- had to scan the whole workspace_tasks table. Partial index makes
        -- this a direct seek on live tasks for a given thread.
        CREATE INDEX IF NOT EXISTS idx_workspace_tasks_active_thread_updated
            ON workspace_tasks(thread_id, updated_at DESC)
            WHERE deleted_at IS NULL;",
    )
    .await?;
    exec.execute_batch(
        "-- Approval inbox latest-by-session lookup. The existing
        -- `idx_approval_inbox_session(session_id, requested_at DESC)`
        -- helps when ordering by request time, but the active-flow path
        -- needs ordering by `expires_at` to find the imminently-expiring
        -- approval first. Cannot make this a partial index gated on
        -- expires_at > now() (the predicate is non-deterministic), so it
        -- stays a plain composite — the IS NOT NULL filter still trims
        -- archived rows from the index.
        CREATE INDEX IF NOT EXISTS idx_approval_inbox_session_expires
            ON approval_inbox(session_id, expires_at DESC)
            WHERE expires_at IS NOT NULL;

        -- causal_traces by goal_run_id. The settle_goal_plan_causal_traces
        -- and settle_subgoal_causal_traces queries filter `WHERE goal_run_id
        -- = ?` and there was no goal-run index — only task_id, decision_type,
        -- and trace_family. Without this, every settle call scanned the
        -- whole causal_traces table.
        CREATE INDEX IF NOT EXISTS idx_causal_traces_goal_run
            ON causal_traces(goal_run_id, created_at DESC)
            WHERE goal_run_id IS NOT NULL;

        -- Expression index for `list_recent_critique_sessions_for_tool`
        -- which filters `WHERE json_extract(session_json, '$.tool_name')
        -- = ?` then orders by updated_at DESC. Without this every call
        -- scans the full critique_sessions table and re-parses session_json
        -- for each row.
        CREATE INDEX IF NOT EXISTS idx_critique_sessions_tool_updated
            ON critique_sessions(
                json_extract(session_json, '$.tool_name'),
                updated_at DESC
            )
            WHERE session_json IS NOT NULL AND json_valid(session_json);

        -- `mark_missing_semantic_documents_removed` filters by
        --   source_kind = ? AND root_path = ? AND deleted_at IS NULL
        -- The existing `idx_semantic_documents_seen` (source_kind, root_path,
        -- last_seen_at) covers the equality prefix, but tombstoned rows are
        -- still in the index. Partial index trims them. Documents reach
        -- thousands per source for large workspaces.
        CREATE INDEX IF NOT EXISTS idx_semantic_documents_active
            ON semantic_documents(source_kind, root_path)
            WHERE deleted_at IS NULL;

        -- `semantic_index_status` runs `COUNT(*) FROM embedding_jobs WHERE
        -- last_error IS NOT NULL` (and the same on embedding_deletions) as
        -- part of every status poll. Without a matching partial index it
        -- scans every row checking last_error per call. Both partial
        -- indexes are tiny (only failed rows) and turn the COUNT into an
        -- index-only walk.
        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_failed
            ON embedding_jobs(updated_at)
            WHERE last_error IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_embedding_deletions_failed
            ON embedding_deletions(queued_at)
            WHERE last_error IS NOT NULL;

        -- Partial-index variant of `idx_threads_updated`. The base index
        -- (defined in schema_sql.rs) is on `updated_at DESC` over the full
        -- table; with this partial index the `latest_thread_id_by_message_timestamp`
        -- and `list_threads_filtered` queries (both filtering deleted_at IS
        -- NULL and ordering by updated_at DESC) can use an index that has
        -- already excluded tombstoned threads, so LIMIT 1 lookups are a
        -- single seek with zero scan-skip overhead.
        CREATE INDEX IF NOT EXISTS idx_threads_active_updated
            ON agent_threads(updated_at DESC)
            WHERE deleted_at IS NULL;

        -- Powers `get_agent_statistics`'s three aggregate scans (totals,
        -- per-provider, per-model). All three queries filter
        --   role = 'assistant' AND deleted_at IS NULL AND created_at >= ?
        -- The existing `idx_messages_thread_role_created` is keyed on
        -- thread_id first, useless for the global aggregate. This partial
        -- index only contains assistant messages and is keyed on
        -- created_at, so the cutoff filter is a direct seek and the rest
        -- of the scan walks only assistant rows.
        CREATE INDEX IF NOT EXISTS idx_messages_assistant_created
            ON agent_messages(created_at)
            WHERE role = 'assistant' AND deleted_at IS NULL;

        CREATE INDEX IF NOT EXISTS idx_messages_active_created
            ON agent_messages(created_at DESC, id DESC)
            WHERE deleted_at IS NULL;

        CREATE INDEX IF NOT EXISTS idx_embedding_jobs_claim_scan
            ON embedding_jobs(updated_at, chunk_id, source_kind, source_id, content_hash, claimed_at);

        CREATE INDEX IF NOT EXISTS idx_embedding_job_completions_lookup
            ON embedding_job_completions(source_kind, source_id, chunk_id, embedding_model, dimensions, content_hash);",
    )
    .await?;

    ensure_column(
        &mut *exec,
        "agent_threads",
        "pinned",
        "INTEGER GENERATED ALWAYS AS (\
            CASE WHEN metadata_json IS NOT NULL AND json_valid(metadata_json) AND (\
                json_extract(metadata_json, '$.pinned') = 1 \
                OR json_extract(metadata_json, '$.pinnedThread') = 1\
            ) THEN 1 ELSE 0 END\
        ) VIRTUAL",
    )
    .await?;
    exec.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_threads_pinned_active_updated
            ON agent_threads(pinned, updated_at DESC)
            WHERE deleted_at IS NULL;",
    )
    .await?;
    exec.execute_batch(
        "-- agent_tasks visible-status pages, ordered by recent activity.
        -- list_tasks_capped_for_ipc and the supervision loops scan this.
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_active_status_priority
            ON agent_tasks(status, priority, created_at DESC)
            WHERE deleted_at IS NULL;

        -- Fast lookup of the most-recent compaction marker per thread.
        -- The send-message path, the concierge welcome's
        -- `concierge_thread_context_summary`, and the TUI's
        -- `list_active_context_window` all need this — without the partial
        -- index, finding the latest compaction marker scanned every
        -- message in the thread for json_extract + LIKE evaluation.
        CREATE INDEX IF NOT EXISTS idx_messages_compaction_marker
            ON agent_messages(thread_id, created_at DESC, id DESC)
            WHERE deleted_at IS NULL
              AND (
                  (metadata_json IS NOT NULL
                   AND json_valid(metadata_json)
                   AND json_extract(metadata_json, '$.message_kind') = 'compaction_artifact')
                  OR content LIKE '[Compacted earlier context]%'
                  OR content LIKE 'Pre-compaction context:%'
              );

        -- Partial covering index for the per-thread chronological linear scan
        -- in `thread_ids_with_unanswered_tool_calls` (and any future scanner
        -- that needs `WHERE thread_id IN (?,?,...) AND deleted_at IS NULL
        -- ORDER BY thread_id, created_at, id`). The leading `thread_id`
        -- prefix lets SQLite seek directly to each thread's block; the
        -- (created_at, id) suffix provides the index-only ordering. The
        -- WHERE-clause makes it a partial index — we only need rows that
        -- aren't tombstoned. With this in place the scanner is bounded by
        -- the number of live messages in the requested threads, not by the
        -- whole agent_messages table.
        CREATE INDEX IF NOT EXISTS idx_messages_active_thread_chrono
            ON agent_messages(thread_id, created_at, id)
            WHERE deleted_at IS NULL;

        -- Partial indexes for `list_notifications` /
        -- `list_notifications_by_source` /
        -- `archive_notifications_by_source_except_ids`. These three queries
        -- all share the WHERE shape:
        --   category = ? AND json_valid(payload_json)
        --   AND json_extract(payload_json, '$.archived_at') IS NULL
        --   AND json_extract(payload_json, '$.deleted_at')  IS NULL
        -- Without this index the planner had to re-evaluate json_extract on
        -- every row of the (category, ...) prefix. The partial WHERE clause
        -- mirrors the queries exactly, so the planner can use it as a direct
        -- seek; only live notifications end up in the index.
        CREATE INDEX IF NOT EXISTS idx_agent_events_active_by_cat_ts
            ON agent_events(category, timestamp DESC)
            WHERE json_valid(payload_json)
              AND json_extract(payload_json, '$.archived_at') IS NULL
              AND json_extract(payload_json, '$.deleted_at')  IS NULL;

        -- Variant that adds the per-source filter for
        -- `list_notifications_by_source`. The leading (category, source)
        -- shape lets the planner seek directly to a notification source's
        -- live-notification block.
        CREATE INDEX IF NOT EXISTS idx_agent_events_active_by_cat_source_ts
            ON agent_events(
                category,
                json_extract(payload_json, '$.source'),
                timestamp DESC
            )
            WHERE json_valid(payload_json)
              AND json_extract(payload_json, '$.archived_at') IS NULL
              AND json_extract(payload_json, '$.deleted_at')  IS NULL;

        CREATE TABLE IF NOT EXISTS thread_prompt_queue (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            content TEXT NOT NULL,
            content_blocks_json TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_prompt_queue_thread_pos
            ON thread_prompt_queue(thread_id, position, created_at);",
    )
    .await?;

    Ok(())
}
