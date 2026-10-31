pub(super) fn extended_schema_sql() -> &'static str {
    r#"
            CREATE TABLE IF NOT EXISTS causal_traces (
                id                    TEXT PRIMARY KEY,
                thread_id             TEXT,
                goal_run_id           TEXT,
                task_id               TEXT,
                decision_type         TEXT NOT NULL,
                trace_family          TEXT NOT NULL DEFAULT '',
                selected_json         TEXT NOT NULL,
                rejected_options_json TEXT,
                context_hash          TEXT,
                causal_factors_json   TEXT,
                outcome_json          TEXT NOT NULL,
                model_used            TEXT,
                created_at            INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_causal_traces_decision_type ON causal_traces(decision_type, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_causal_traces_task_id ON causal_traces(task_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS harness_state_records (
                entry_id       TEXT PRIMARY KEY,
                entity_id      TEXT NOT NULL,
                thread_id      TEXT,
                goal_run_id    TEXT,
                task_id        TEXT,
                record_kind    TEXT NOT NULL,
                status         TEXT,
                summary        TEXT NOT NULL,
                payload_json   TEXT NOT NULL,
                created_at_ms  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_harness_state_scope_created ON harness_state_records(thread_id, goal_run_id, task_id, created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_harness_state_kind_created ON harness_state_records(record_kind, created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_harness_state_entity_created ON harness_state_records(entity_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS memory_provenance (
                id            TEXT PRIMARY KEY,
                target        TEXT NOT NULL,
                mode          TEXT NOT NULL,
                source_kind   TEXT NOT NULL,
                content       TEXT NOT NULL,
                fact_keys_json TEXT NOT NULL DEFAULT '[]',
                thread_id     TEXT,
                task_id       TEXT,
                goal_run_id   TEXT,
                created_at    INTEGER NOT NULL,
                entry_hash    TEXT NOT NULL DEFAULT '',
                signature     TEXT,
                signature_scheme TEXT,
                confirmed_at  INTEGER,
                retracted_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_target_ts ON memory_provenance(target, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_goal_run ON memory_provenance(goal_run_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_provenance_relationships (
                id              TEXT PRIMARY KEY,
                source_entry_id TEXT NOT NULL,
                target_entry_id TEXT NOT NULL,
                relation_type   TEXT NOT NULL,
                fact_key        TEXT,
                created_at      INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_provenance_rel_unique ON memory_provenance_relationships(source_entry_id, target_entry_id, relation_type, fact_key);
            CREATE INDEX IF NOT EXISTS idx_memory_provenance_rel_source ON memory_provenance_relationships(source_entry_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS skill_variants (
                variant_id         TEXT PRIMARY KEY,
                skill_name         TEXT NOT NULL,
                variant_name       TEXT NOT NULL,
                relative_path      TEXT NOT NULL UNIQUE,
                parent_variant_id  TEXT,
                version            TEXT NOT NULL,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                use_count          INTEGER NOT NULL DEFAULT 0,
                success_count      INTEGER NOT NULL DEFAULT 0,
                failure_count      INTEGER NOT NULL DEFAULT 0,
                fitness_score      REAL NOT NULL DEFAULT 0,
                status             TEXT NOT NULL DEFAULT 'active',
                last_used_at       INTEGER,
                created_at         INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variants_name ON skill_variants(skill_name, status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variants_path ON skill_variants(relative_path);

            CREATE TABLE IF NOT EXISTS semantic_documents (
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
            CREATE INDEX IF NOT EXISTS idx_semantic_documents_seen ON semantic_documents(source_kind, root_path, last_seen_at);

            CREATE TABLE IF NOT EXISTS skill_variant_usage (
                usage_id           TEXT PRIMARY KEY,
                variant_id         TEXT NOT NULL,
                thread_id          TEXT,
                task_id            TEXT,
                goal_run_id        TEXT,
                context_tags_json  TEXT NOT NULL DEFAULT '[]',
                consulted_at       INTEGER NOT NULL,
                resolved_at        INTEGER,
                outcome            TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_variant ON skill_variant_usage(variant_id, consulted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_skill_variant_usage_resolution ON skill_variant_usage(task_id, goal_run_id, thread_id, resolved_at);

            CREATE TABLE IF NOT EXISTS skill_variant_history (
                id                 TEXT PRIMARY KEY,
                variant_id         TEXT NOT NULL,
                recorded_at        INTEGER NOT NULL,
                outcome            TEXT NOT NULL,
                fitness_score      REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_skill_variant_history_variant_ts ON skill_variant_history(variant_id, recorded_at DESC);

            CREATE TABLE IF NOT EXISTS gene_pool (
                parent_a        TEXT NOT NULL,
                parent_b        TEXT NOT NULL,
                offspring_id    TEXT NOT NULL,
                lifecycle_state TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                PRIMARY KEY (parent_a, parent_b)
            );
            CREATE INDEX IF NOT EXISTS idx_gene_pool_offspring ON gene_pool(offspring_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS gene_fitness_history (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                variant_id      TEXT NOT NULL,
                recorded_at_ms  INTEGER NOT NULL,
                fitness_score   REAL NOT NULL,
                use_count       INTEGER NOT NULL,
                success_rate    REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gene_fitness_variant_ts ON gene_fitness_history(variant_id, recorded_at_ms DESC);

            CREATE TABLE IF NOT EXISTS gene_crossbreeds (
                id                       INTEGER PRIMARY KEY AUTOINCREMENT,
                left_parent_variant_id   TEXT NOT NULL,
                right_parent_variant_id  TEXT NOT NULL,
                skill_name               TEXT NOT NULL,
                co_usage_rate            REAL NOT NULL,
                proposed_at_ms           INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gene_crossbreeds_skill_ts ON gene_crossbreeds(skill_name, proposed_at_ms DESC);

            CREATE TABLE IF NOT EXISTS morphogenesis_affinities (
                agent_id         TEXT NOT NULL,
                domain           TEXT NOT NULL,
                affinity_score   REAL NOT NULL DEFAULT 0.0,
                task_count       INTEGER NOT NULL DEFAULT 0,
                success_count    INTEGER NOT NULL DEFAULT 0,
                failure_count    INTEGER NOT NULL DEFAULT 0,
                last_updated_ms  INTEGER NOT NULL,
                PRIMARY KEY (agent_id, domain)
            );
            CREATE INDEX IF NOT EXISTS idx_morphogenesis_domain_updated ON morphogenesis_affinities(domain, last_updated_ms DESC);

            CREATE TABLE IF NOT EXISTS affinity_updates_log (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id        TEXT NOT NULL,
                domain          TEXT NOT NULL,
                old_affinity    REAL NOT NULL,
                new_affinity    REAL NOT NULL,
                trigger_type    TEXT NOT NULL,
                task_id         TEXT,
                updated_at_ms   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_affinity_updates_agent_domain_ts ON affinity_updates_log(agent_id, domain, updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS soul_adaptations_log (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id         TEXT NOT NULL,
                domain           TEXT NOT NULL,
                adaptation_type  TEXT NOT NULL,
                soul_snippet     TEXT NOT NULL,
                old_soul_hash    TEXT,
                new_soul_hash    TEXT,
                created_at_ms    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_soul_adaptations_agent_ts ON soul_adaptations_log(agent_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS consensus_bid_priors (
                role             TEXT PRIMARY KEY,
                success_count    INTEGER NOT NULL DEFAULT 0,
                failure_count    INTEGER NOT NULL DEFAULT 0,
                prior_score      REAL NOT NULL DEFAULT 0.5,
                last_updated_ms  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_consensus_bid_priors_updated ON consensus_bid_priors(last_updated_ms DESC);

            CREATE TABLE IF NOT EXISTS consensus_bids (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id          TEXT NOT NULL,
                round_id         INTEGER NOT NULL,
                agent_id         TEXT NOT NULL,
                confidence       REAL NOT NULL,
                reasoning        TEXT,
                availability     TEXT NOT NULL,
                domain_affinity  REAL NOT NULL DEFAULT 0.0,
                submitted_at_ms  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_consensus_bids_task_round ON consensus_bids(task_id, round_id, submitted_at_ms DESC);

            CREATE TABLE IF NOT EXISTS role_assignments (
                id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id            TEXT NOT NULL,
                round_id           INTEGER NOT NULL,
                primary_agent_id   TEXT NOT NULL,
                reviewer_agent_id  TEXT,
                observers          TEXT NOT NULL DEFAULT '[]',
                assigned_at_ms     INTEGER NOT NULL,
                outcome            TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_role_assignments_task_round ON role_assignments(task_id, round_id, assigned_at_ms DESC);

            CREATE TABLE IF NOT EXISTS consensus_quality_metrics (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id               TEXT NOT NULL,
                predicted_confidence  REAL NOT NULL,
                actual_outcome_score  REAL NOT NULL,
                prediction_error      REAL NOT NULL,
                updated_at_ms         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_consensus_quality_task_ts ON consensus_quality_metrics(task_id, updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS memory_graph_clusters (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                name           TEXT NOT NULL UNIQUE,
                summary_text   TEXT,
                center_node_id TEXT,
                created_at_ms  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_graph_clusters_center ON memory_graph_clusters(center_node_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS memory_cluster_members (
                cluster_id INTEGER NOT NULL,
                node_id    TEXT NOT NULL,
                deleted_at INTEGER,
                PRIMARY KEY (cluster_id, node_id)
            );
            CREATE INDEX IF NOT EXISTS idx_memory_cluster_members_node ON memory_cluster_members(node_id, cluster_id);

            CREATE TABLE IF NOT EXISTS collaboration_sessions (
                parent_task_id TEXT PRIMARY KEY,
                session_json   TEXT NOT NULL,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_collaboration_sessions_updated ON collaboration_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS collaboration_agent_outcomes (
                parent_task_id TEXT NOT NULL,
                task_id        TEXT NOT NULL,
                success_count  INTEGER NOT NULL DEFAULT 0,
                failure_count  INTEGER NOT NULL DEFAULT 0,
                learned_score  REAL NOT NULL DEFAULT 0.5,
                last_outcome   TEXT,
                updated_at_ms  INTEGER NOT NULL,
                PRIMARY KEY (parent_task_id, task_id)
            );
            CREATE INDEX IF NOT EXISTS idx_collaboration_agent_outcomes_parent_updated ON collaboration_agent_outcomes(parent_task_id, updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS debate_sessions (
                session_id   TEXT PRIMARY KEY,
                session_json TEXT NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_debate_sessions_updated ON debate_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS debate_arguments (
                session_id     TEXT NOT NULL,
                argument_json  TEXT NOT NULL,
                created_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_debate_arguments_session_created ON debate_arguments(session_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS debate_verdicts (
                session_id    TEXT PRIMARY KEY,
                verdict_json  TEXT NOT NULL,
                updated_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_debate_verdicts_updated ON debate_verdicts(updated_at DESC);

            CREATE TABLE IF NOT EXISTS critique_sessions (
                session_id   TEXT PRIMARY KEY,
                session_json TEXT NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_critique_sessions_updated ON critique_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS critique_arguments (
                session_id     TEXT NOT NULL,
                role           TEXT NOT NULL,
                claim          TEXT NOT NULL,
                weight         REAL NOT NULL,
                evidence_json  TEXT NOT NULL,
                created_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_critique_arguments_session_created ON critique_arguments(session_id, created_at ASC);

            CREATE TABLE IF NOT EXISTS critique_resolutions (
                session_id       TEXT PRIMARY KEY,
                decision         TEXT NOT NULL,
                resolution_json  TEXT NOT NULL,
                risk_score       REAL NOT NULL,
                confidence       REAL NOT NULL,
                resolved_at      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_critique_resolutions_resolved ON critique_resolutions(resolved_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_threads (
                channel_key TEXT PRIMARY KEY,
                thread_id   TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_threads_updated ON gateway_threads(updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_channel_modes (
                channel_key TEXT PRIMARY KEY,
                route_mode  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_channel_modes_updated ON gateway_channel_modes(updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_replay_cursors (
                platform    TEXT NOT NULL,
                channel_id  TEXT NOT NULL,
                cursor_value TEXT NOT NULL,
                cursor_type TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (platform, channel_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_replay_cursors_platform ON gateway_replay_cursors(platform, updated_at DESC);

            CREATE TABLE IF NOT EXISTS gateway_health_snapshots (
                platform    TEXT PRIMARY KEY,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_gateway_health_snapshots_updated ON gateway_health_snapshots(updated_at DESC);

            CREATE TABLE IF NOT EXISTS whatsapp_provider_state (
                provider_id    TEXT PRIMARY KEY,
                linked_phone   TEXT,
                auth_json      TEXT,
                metadata_json  TEXT,
                last_reset_at  INTEGER,
                last_linked_at INTEGER,
                updated_at     INTEGER NOT NULL,
                deleted_at     INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_whatsapp_provider_state_updated ON whatsapp_provider_state(updated_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_sessions (
                session_id   TEXT PRIMARY KEY,
                kind         TEXT NOT NULL,
                session_json TEXT NOT NULL,
                updated_at   INTEGER NOT NULL,
                deleted_at   INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_operator_profile_sessions_updated ON operator_profile_sessions(updated_at DESC);

            CREATE TABLE IF NOT EXISTS external_runtime_profiles (
                runtime             TEXT PRIMARY KEY,
                profile_json        TEXT NOT NULL,
                session_id          TEXT,
                source_config_path  TEXT,
                source_fingerprint  TEXT,
                updated_at          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_external_runtime_profiles_updated ON external_runtime_profiles(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_external_runtime_profiles_session ON external_runtime_profiles(session_id, updated_at DESC);

            CREATE TABLE IF NOT EXISTS external_runtime_import_sessions (
                session_id          TEXT PRIMARY KEY,
                runtime             TEXT NOT NULL,
                source_config_path  TEXT NOT NULL,
                source_fingerprint  TEXT NOT NULL,
                dry_run             INTEGER NOT NULL DEFAULT 0,
                conflict_policy     TEXT NOT NULL,
                source_surface      TEXT NOT NULL,
                session_json        TEXT NOT NULL,
                imported_at_ms      INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_runtime ON external_runtime_import_sessions(runtime, updated_at DESC);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_external_runtime_import_sessions_fingerprint ON external_runtime_import_sessions(runtime, source_config_path, source_fingerprint, dry_run);

            CREATE TABLE IF NOT EXISTS imported_runtime_assets (
                asset_id            TEXT PRIMARY KEY,
                session_id          TEXT NOT NULL,
                runtime             TEXT NOT NULL,
                asset_kind          TEXT NOT NULL,
                bucket              TEXT NOT NULL,
                severity            TEXT NOT NULL,
                recommended_action  TEXT,
                source_path         TEXT,
                source_fingerprint  TEXT,
                conflict_policy     TEXT NOT NULL,
                asset_json          TEXT NOT NULL,
                created_at_ms       INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_session ON imported_runtime_assets(session_id, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_imported_runtime_assets_runtime_kind ON imported_runtime_assets(runtime, asset_kind, updated_at DESC);

            CREATE TABLE IF NOT EXISTS external_runtime_shadow_runs (
                run_id             TEXT PRIMARY KEY,
                runtime            TEXT NOT NULL,
                session_id         TEXT NOT NULL,
                workflow           TEXT NOT NULL,
                readiness_score    INTEGER NOT NULL,
                blocker_count      INTEGER NOT NULL,
                summary            TEXT NOT NULL,
                payload_json       TEXT NOT NULL,
                created_at_ms      INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_runtime ON external_runtime_shadow_runs(runtime, created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_external_runtime_shadow_runs_session ON external_runtime_shadow_runs(session_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS browser_profiles (
                profile_id              TEXT PRIMARY KEY,
                label                   TEXT NOT NULL,
                profile_dir             TEXT NOT NULL,
                browser_kind            TEXT,
                workspace_id            TEXT,
                health_state            TEXT NOT NULL DEFAULT 'healthy',
                created_at              INTEGER NOT NULL,
                updated_at              INTEGER NOT NULL,
                last_used_at            INTEGER,
                last_auth_success_at    INTEGER,
                last_auth_failure_at    INTEGER,
                last_auth_failure_reason TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_browser_profiles_updated ON browser_profiles(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_items (
                key_path   TEXT PRIMARY KEY,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                deleted_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_items_updated ON agent_config_items(updated_at DESC);

            CREATE TABLE IF NOT EXISTS agent_config_updates (
                id         TEXT PRIMARY KEY,
                key_path   TEXT NOT NULL,
                value_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_agent_config_updates_key_ts ON agent_config_updates(key_path, updated_at DESC);

            CREATE TABLE IF NOT EXISTS provider_auth_state (
                provider_id TEXT NOT NULL,
                auth_mode   TEXT NOT NULL,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (provider_id, auth_mode)
            );
            CREATE INDEX IF NOT EXISTS idx_provider_auth_state_updated ON provider_auth_state(updated_at DESC);

            CREATE TABLE IF NOT EXISTS heartbeat_history (
                id              TEXT PRIMARY KEY,
                cycle_timestamp INTEGER NOT NULL,
                checks_json     TEXT NOT NULL,
                synthesis_json  TEXT,
                actionable      INTEGER NOT NULL DEFAULT 0,
                digest_text     TEXT,
                llm_tokens_used INTEGER NOT NULL DEFAULT 0,
                duration_ms     INTEGER NOT NULL DEFAULT 0,
                status          TEXT NOT NULL DEFAULT 'completed'
            );
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_ts ON heartbeat_history(cycle_timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_heartbeat_history_actionable ON heartbeat_history(actionable, cycle_timestamp DESC);

            CREATE TABLE IF NOT EXISTS meta_cognition_model (
                id                  INTEGER PRIMARY KEY CHECK (id = 1),
                agent_id            TEXT NOT NULL,
                calibration_offset  REAL NOT NULL DEFAULT 0.0,
                last_updated_at     INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cognitive_biases (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                model_id              INTEGER NOT NULL,
                name                  TEXT NOT NULL,
                trigger_pattern_json  TEXT NOT NULL,
                mitigation_prompt     TEXT NOT NULL,
                severity              REAL NOT NULL,
                occurrence_count      INTEGER NOT NULL DEFAULT 0,
                deleted_at            INTEGER,
                FOREIGN KEY (model_id) REFERENCES meta_cognition_model(id)
            );
            CREATE INDEX IF NOT EXISTS idx_cognitive_biases_model ON cognitive_biases(model_id, severity DESC);

            CREATE TABLE IF NOT EXISTS workflow_profiles (
                id                 INTEGER PRIMARY KEY AUTOINCREMENT,
                model_id           INTEGER NOT NULL,
                name               TEXT NOT NULL,
                avg_success_rate   REAL NOT NULL,
                avg_steps          INTEGER NOT NULL,
                typical_tools_json TEXT NOT NULL,
                deleted_at         INTEGER,
                FOREIGN KEY (model_id) REFERENCES meta_cognition_model(id)
            );
            CREATE INDEX IF NOT EXISTS idx_workflow_profiles_model ON workflow_profiles(model_id, avg_success_rate DESC);

            CREATE TABLE IF NOT EXISTS action_audit (
                id                TEXT PRIMARY KEY,
                timestamp         INTEGER NOT NULL,
                action_type       TEXT NOT NULL,
                summary           TEXT NOT NULL,
                explanation       TEXT,
                confidence        REAL,
                confidence_band   TEXT,
                causal_trace_id   TEXT,
                thread_id         TEXT,
                goal_run_id       TEXT,
                task_id           TEXT,
                raw_data_json     TEXT,
                deleted_at        INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_action_audit_ts ON action_audit(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_type_ts ON action_audit(action_type, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_action_audit_thread ON action_audit(thread_id, timestamp DESC);

            CREATE TABLE IF NOT EXISTS memory_tombstones (
                id TEXT PRIMARY KEY,
                target TEXT NOT NULL,
                original_content TEXT NOT NULL,
                fact_key TEXT,
                replaced_by TEXT,
                replaced_at INTEGER NOT NULL,
                source_kind TEXT NOT NULL,
                provenance_id TEXT,
                created_at INTEGER NOT NULL,
                deleted_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_tombstones_created ON memory_tombstones(created_at);
            CREATE INDEX IF NOT EXISTS idx_tombstones_target ON memory_tombstones(target, created_at DESC);

            CREATE TABLE IF NOT EXISTS consolidation_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                deleted_at INTEGER
            );

            CREATE TABLE IF NOT EXISTS offloaded_payloads (
                payload_id    TEXT PRIMARY KEY,
                thread_id     TEXT NOT NULL,
                tool_name     TEXT NOT NULL,
                tool_call_id  TEXT,
                storage_path  TEXT NOT NULL,
                content_type  TEXT NOT NULL,
                byte_size     INTEGER NOT NULL,
                summary       TEXT NOT NULL,
                created_at    INTEGER NOT NULL,
                deleted_at    INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_offloaded_payloads_thread_created ON offloaded_payloads(thread_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS thread_structural_memory (
                thread_id   TEXT PRIMARY KEY,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_thread_structural_memory_updated ON thread_structural_memory(updated_at DESC);

            CREATE TABLE IF NOT EXISTS implicit_signals (
                id                    TEXT PRIMARY KEY,
                session_id            TEXT NOT NULL,
                signal_type           TEXT NOT NULL,
                weight                REAL NOT NULL,
                timestamp_ms          INTEGER NOT NULL,
                context_snapshot_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_implicit_signals_session_ts ON implicit_signals(session_id, timestamp_ms DESC);

            CREATE TABLE IF NOT EXISTS satisfaction_scores (
                id             TEXT PRIMARY KEY,
                session_id     TEXT NOT NULL,
                score          REAL NOT NULL,
                computed_at_ms INTEGER NOT NULL,
                label          TEXT NOT NULL,
                signal_count   INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_satisfaction_scores_session_ts ON satisfaction_scores(session_id, computed_at_ms DESC);

            CREATE TABLE IF NOT EXISTS cognitive_resonance_samples (
                id                         INTEGER PRIMARY KEY AUTOINCREMENT,
                sampled_at_ms              INTEGER NOT NULL,
                revision_velocity_ms       INTEGER,
                session_entropy            REAL,
                approval_latency_ms        INTEGER,
                tool_hesitation_count      INTEGER NOT NULL DEFAULT 0,
                cognitive_state            TEXT NOT NULL,
                state_confidence           REAL NOT NULL,
                resonance_score            REAL NOT NULL,
                verbosity_adjustment       REAL NOT NULL,
                risk_adjustment            REAL NOT NULL,
                proactiveness_adjustment   REAL NOT NULL,
                memory_urgency_adjustment  REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cognitive_resonance_samples_sampled ON cognitive_resonance_samples(sampled_at_ms DESC);

            CREATE TABLE IF NOT EXISTS behavior_adjustments_log (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                adjusted_at_ms    INTEGER NOT NULL,
                parameter         TEXT NOT NULL,
                old_value         REAL NOT NULL,
                new_value         REAL NOT NULL,
                trigger_reason    TEXT NOT NULL,
                resonance_score   REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_behavior_adjustments_log_adjusted ON behavior_adjustments_log(adjusted_at_ms DESC);

            CREATE TABLE IF NOT EXISTS intent_predictions (
                id                 TEXT PRIMARY KEY,
                session_id         TEXT NOT NULL,
                context_state_hash TEXT NOT NULL,
                predicted_action   TEXT NOT NULL,
                confidence         REAL NOT NULL,
                actual_action      TEXT,
                was_correct        INTEGER,
                created_at_ms      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_intent_predictions_session_ts ON intent_predictions(session_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS intent_models (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id        TEXT NOT NULL UNIQUE,
                model_blob      BLOB,
                created_at_ms   INTEGER NOT NULL,
                accuracy_score  REAL
            );
            CREATE INDEX IF NOT EXISTS idx_intent_models_agent_created ON intent_models(agent_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS system_outcome_predictions (
                id                TEXT PRIMARY KEY,
                session_id        TEXT NOT NULL,
                prediction_type   TEXT NOT NULL,
                predicted_outcome TEXT NOT NULL,
                confidence        REAL NOT NULL,
                actual_outcome    TEXT,
                was_correct       INTEGER,
                created_at_ms     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_system_outcome_predictions_session_ts ON system_outcome_predictions(session_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS temporal_patterns (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern_type        TEXT NOT NULL,
                timescale           TEXT NOT NULL,
                pattern_description TEXT NOT NULL,
                context_filter      TEXT,
                frequency           INTEGER NOT NULL DEFAULT 1,
                last_observed_ms    INTEGER NOT NULL,
                first_observed_ms   INTEGER NOT NULL,
                confidence          REAL NOT NULL,
                decay_rate          REAL NOT NULL DEFAULT 0.01,
                created_at_ms       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_temporal_patterns_type_scale ON temporal_patterns(pattern_type, timescale, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS temporal_predictions (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern_id       INTEGER NOT NULL,
                predicted_action TEXT NOT NULL,
                predicted_at_ms  INTEGER NOT NULL,
                confidence       REAL NOT NULL,
                actual_action    TEXT,
                was_accepted     INTEGER,
                accuracy_score   REAL,
                FOREIGN KEY (pattern_id) REFERENCES temporal_patterns(id)
            );
            CREATE INDEX IF NOT EXISTS idx_temporal_predictions_pattern_predicted ON temporal_predictions(pattern_id, predicted_at_ms DESC);

            CREATE TABLE IF NOT EXISTS precomputation_log (
                id                      INTEGER PRIMARY KEY AUTOINCREMENT,
                prediction_id           INTEGER NOT NULL,
                precomputation_type     TEXT NOT NULL,
                precomputation_details  TEXT NOT NULL,
                started_at_ms           INTEGER NOT NULL,
                completed_at_ms         INTEGER,
                was_used                INTEGER,
                FOREIGN KEY (prediction_id) REFERENCES temporal_predictions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_precomputation_log_prediction_started ON precomputation_log(prediction_id, started_at_ms DESC);

            CREATE TABLE IF NOT EXISTS dream_cycles (
                id                           INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at_ms                INTEGER NOT NULL,
                completed_at_ms              INTEGER,
                idle_duration_ms             INTEGER NOT NULL,
                tasks_analyzed               INTEGER NOT NULL,
                counterfactuals_generated    INTEGER NOT NULL,
                counterfactuals_successful   INTEGER NOT NULL,
                status                       TEXT NOT NULL DEFAULT 'running'
            );
            CREATE INDEX IF NOT EXISTS idx_dream_cycles_started ON dream_cycles(started_at_ms DESC);

            CREATE TABLE IF NOT EXISTS counterfactual_evaluations (
                id                            INTEGER PRIMARY KEY AUTOINCREMENT,
                dream_cycle_id                INTEGER NOT NULL,
                source_task_id                TEXT NOT NULL,
                variation_type                TEXT NOT NULL,
                counterfactual_description    TEXT NOT NULL,
                estimated_token_saving        REAL,
                estimated_time_saving_ms      INTEGER,
                estimated_revision_reduction  INTEGER,
                score                         REAL NOT NULL,
                threshold_met                 INTEGER NOT NULL,
                created_at_ms                 INTEGER NOT NULL,
                FOREIGN KEY (dream_cycle_id) REFERENCES dream_cycles(id)
            );
            CREATE INDEX IF NOT EXISTS idx_counterfactual_evaluations_cycle ON counterfactual_evaluations(dream_cycle_id, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS event_log (
                id             TEXT PRIMARY KEY,
                event_family   TEXT NOT NULL,
                event_kind     TEXT NOT NULL,
                state          TEXT,
                thread_id      TEXT,
                payload_json   TEXT NOT NULL,
                risk_label     TEXT NOT NULL,
                handled_at_ms  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_event_log_family_kind_ts ON event_log(event_family, event_kind, handled_at_ms DESC);

            CREATE TABLE IF NOT EXISTS thread_protocol_candidates (
                thread_id   TEXT PRIMARY KEY,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_thread_protocol_candidates_updated ON thread_protocol_candidates(updated_at DESC);

            CREATE TABLE IF NOT EXISTS emergent_protocols (
                protocol_id              TEXT PRIMARY KEY,
                token                    TEXT NOT NULL UNIQUE,
                description              TEXT NOT NULL,
                agent_a                  TEXT NOT NULL,
                agent_b                  TEXT NOT NULL,
                thread_id                TEXT NOT NULL,
                normalized_pattern       TEXT NOT NULL,
                signal_kind              TEXT NOT NULL,
                context_signature_json   TEXT NOT NULL,
                created_at               INTEGER NOT NULL,
                activated_at             INTEGER NOT NULL,
                last_used_at             INTEGER,
                usage_count              INTEGER NOT NULL DEFAULT 0,
                success_rate             REAL NOT NULL DEFAULT 1.0,
                source_candidate_id      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_emergent_protocols_thread_activated ON emergent_protocols(thread_id, activated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_emergent_protocols_pattern ON emergent_protocols(normalized_pattern, activated_at DESC);

            CREATE TABLE IF NOT EXISTS protocol_steps (
                protocol_id          TEXT NOT NULL,
                step_index           INTEGER NOT NULL,
                intent               TEXT NOT NULL,
                tool_name            TEXT,
                args_template_json   TEXT NOT NULL,
                deleted_at           INTEGER,
                PRIMARY KEY (protocol_id, step_index),
                FOREIGN KEY (protocol_id) REFERENCES emergent_protocols(protocol_id)
            );
            CREATE INDEX IF NOT EXISTS idx_protocol_steps_protocol ON protocol_steps(protocol_id, step_index ASC);

            CREATE TABLE IF NOT EXISTS protocol_usage_log (
                id                 TEXT PRIMARY KEY,
                protocol_id        TEXT NOT NULL,
                used_at            INTEGER NOT NULL,
                execution_time_ms  INTEGER,
                success            INTEGER NOT NULL,
                fallback_reason    TEXT,
                FOREIGN KEY (protocol_id) REFERENCES emergent_protocols(protocol_id)
            );
            CREATE INDEX IF NOT EXISTS idx_protocol_usage_log_protocol_used ON protocol_usage_log(protocol_id, used_at DESC);

            CREATE TABLE IF NOT EXISTS plugins (
                name            TEXT PRIMARY KEY,
                version         TEXT NOT NULL,
                description     TEXT,
                author          TEXT,
                manifest_json   TEXT NOT NULL,
                install_source  TEXT NOT NULL DEFAULT 'local',
                enabled         INTEGER NOT NULL DEFAULT 1,
                installed_at    TEXT NOT NULL,
                updated_at      TEXT NOT NULL,
                deleted_at      INTEGER
            );

            CREATE TABLE IF NOT EXISTS plugin_settings (
                plugin_name     TEXT NOT NULL,
                key             TEXT NOT NULL,
                value           TEXT NOT NULL,
                is_secret       INTEGER NOT NULL DEFAULT 0,
                deleted_at      INTEGER,
                PRIMARY KEY (plugin_name, key),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name)
            );

            CREATE TABLE IF NOT EXISTS plugin_credentials (
                plugin_name      TEXT NOT NULL,
                credential_type  TEXT NOT NULL,
                encrypted_value  BLOB,
                expires_at       TEXT,
                created_at       TEXT NOT NULL,
                updated_at       TEXT NOT NULL,
                deleted_at       INTEGER,
                PRIMARY KEY (plugin_name, credential_type),
                FOREIGN KEY (plugin_name) REFERENCES plugins(name)
            );

            CREATE TABLE IF NOT EXISTS operator_profile_fields (
                field_key        TEXT PRIMARY KEY,
                field_value_json TEXT NOT NULL,
                confidence       REAL NOT NULL DEFAULT 0.0,
                source           TEXT NOT NULL DEFAULT 'unknown',
                updated_at       INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_fields_updated ON operator_profile_fields(updated_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_consents (
                consent_key TEXT PRIMARY KEY,
                granted     INTEGER NOT NULL DEFAULT 0,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS operator_profile_events (
                id            TEXT PRIMARY KEY,
                event_type    TEXT NOT NULL,
                field_key     TEXT,
                value_json    TEXT,
                source        TEXT NOT NULL DEFAULT 'unknown',
                metadata_json TEXT,
                created_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_events_created ON operator_profile_events(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_op_profile_events_field ON operator_profile_events(field_key, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_distillation_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_thread_id TEXT NOT NULL,
                source_message_range TEXT,
                source_message_span_json TEXT,
                distilled_fact TEXT NOT NULL,
                target_file TEXT NOT NULL,
                category TEXT NOT NULL,
                confidence REAL NOT NULL,
                created_at_ms INTEGER NOT NULL,
                applied_to_memory INTEGER DEFAULT 0,
                agent_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_distillation_log_thread ON memory_distillation_log(source_thread_id, created_at_ms DESC);
            CREATE INDEX IF NOT EXISTS idx_distillation_log_applied ON memory_distillation_log(applied_to_memory, created_at_ms DESC);

            CREATE TABLE IF NOT EXISTS memory_distillation_progress (
                source_thread_id TEXT PRIMARY KEY,
                last_processed_created_at_ms INTEGER NOT NULL,
                last_processed_message_id TEXT NOT NULL,
                last_processed_span_json TEXT,
                last_run_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                agent_id TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_distillation_progress_updated ON memory_distillation_progress(updated_at_ms DESC);

            CREATE TABLE IF NOT EXISTS forge_pass_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT NOT NULL,
                period_start_ms INTEGER NOT NULL,
                period_end_ms INTEGER NOT NULL,
                traces_analyzed INTEGER NOT NULL,
                patterns_found INTEGER NOT NULL,
                hints_applied INTEGER NOT NULL,
                hints_logged INTEGER NOT NULL,
                completed_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_forge_pass_log_agent ON forge_pass_log(agent_id, completed_at_ms DESC);

            CREATE TABLE IF NOT EXISTS event_triggers (
                id                 TEXT PRIMARY KEY,
                event_family       TEXT NOT NULL,
                event_kind         TEXT NOT NULL,
                agent_id           TEXT,
                target_state       TEXT,
                thread_id          TEXT,
                enabled            INTEGER NOT NULL DEFAULT 1,
                cooldown_secs      INTEGER NOT NULL DEFAULT 0,
                risk_label         TEXT NOT NULL DEFAULT 'low',
                notification_kind  TEXT NOT NULL,
                prompt_template    TEXT,
                tool_name          TEXT,
                tool_payload_json  TEXT,
                title_template     TEXT NOT NULL,
                body_template      TEXT NOT NULL,
                created_at         INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL,
                last_fired_at      INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_event_triggers_family_kind_enabled ON event_triggers(event_family, event_kind, enabled, updated_at DESC);

            CREATE TABLE IF NOT EXISTS trigger_fire_history (
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
            CREATE INDEX IF NOT EXISTS idx_trigger_fire_history_status ON trigger_fire_history(status, fired_at_ms DESC);

            CREATE TABLE IF NOT EXISTS routine_definitions (
                id                  TEXT PRIMARY KEY,
                title               TEXT NOT NULL,
                description         TEXT NOT NULL,
                enabled             INTEGER NOT NULL DEFAULT 1,
                paused_at           INTEGER,
                schedule_expression TEXT NOT NULL,
                target_kind         TEXT NOT NULL,
                target_payload_json TEXT NOT NULL,
                schema_version      INTEGER NOT NULL DEFAULT 1,
                next_run_at         INTEGER,
                last_run_at         INTEGER,
                last_result         TEXT,
                last_error          TEXT,
                last_success_summary TEXT,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                deleted_at          INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_routine_definitions_enabled_next_run ON routine_definitions(enabled, next_run_at, updated_at DESC);

            CREATE TABLE IF NOT EXISTS routine_runs (
                id                   TEXT PRIMARY KEY,
                routine_id           TEXT NOT NULL,
                trigger_kind         TEXT NOT NULL,
                status               TEXT NOT NULL,
                started_at           INTEGER NOT NULL,
                finished_at          INTEGER,
                created_task_id      TEXT,
                created_goal_run_id  TEXT,
                payload_json         TEXT NOT NULL,
                result_summary       TEXT,
                error                TEXT,
                rerun_of_run_id      TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_routine_runs_routine_started ON routine_runs(routine_id, started_at DESC);

            CREATE TABLE IF NOT EXISTS operator_profile_checkins (
                id            TEXT PRIMARY KEY,
                kind          TEXT NOT NULL,
                scheduled_at  INTEGER NOT NULL,
                shown_at      INTEGER,
                status        TEXT NOT NULL DEFAULT 'pending',
                response_json TEXT,
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_op_profile_checkins_scheduled ON operator_profile_checkins(scheduled_at ASC);
            CREATE INDEX IF NOT EXISTS idx_op_profile_checkins_status ON operator_profile_checkins(status, scheduled_at ASC);

            CREATE TABLE IF NOT EXISTS workspace_settings (
                workspace_id   TEXT PRIMARY KEY,
                workspace_root TEXT,
                operator       TEXT NOT NULL,
                repo_monitor_enabled INTEGER NOT NULL DEFAULT 0,
                repo_monitor_include_dirs_json TEXT NOT NULL DEFAULT '[]',
                repo_monitor_exclude_dirs_json TEXT NOT NULL DEFAULT '[]',
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_workspace_settings_repo_monitor_enabled
                ON workspace_settings(repo_monitor_enabled, workspace_id)
                WHERE repo_monitor_enabled = 1;

            CREATE TABLE IF NOT EXISTS workspace_tasks (
                id                 TEXT PRIMARY KEY,
                workspace_id       TEXT NOT NULL,
                title              TEXT NOT NULL,
                task_type          TEXT NOT NULL,
                description        TEXT NOT NULL,
                definition_of_done TEXT,
                priority           TEXT NOT NULL,
                status             TEXT NOT NULL,
                sort_order         INTEGER NOT NULL,
                reporter_json      TEXT NOT NULL,
                assignee_json      TEXT,
                reviewer_json      TEXT,
                thread_id          TEXT,
                goal_run_id        TEXT,
                runtime_history_json TEXT NOT NULL DEFAULT '[]',
                created_at         INTEGER NOT NULL,
                updated_at         INTEGER NOT NULL,
                started_at         INTEGER,
                completed_at       INTEGER,
                deleted_at         INTEGER,
                last_notice_id     TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_workspace_tasks_visible ON workspace_tasks(workspace_id, deleted_at, status, sort_order);

            CREATE TABLE IF NOT EXISTS workspace_notices (
                id           TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                task_id      TEXT NOT NULL,
                notice_type  TEXT NOT NULL,
                message      TEXT NOT NULL,
                actor_json   TEXT,
                created_at   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_workspace_notices_task ON workspace_notices(workspace_id, task_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS thread_skill_reads (
                thread_id      TEXT NOT NULL,
                kind           TEXT NOT NULL,
                name           TEXT NOT NULL,
                read_count     INTEGER NOT NULL DEFAULT 1,
                last_read_at   INTEGER NOT NULL,
                last_content   TEXT NOT NULL,
                PRIMARY KEY (thread_id, kind, name)
            );
            CREATE INDEX IF NOT EXISTS idx_thread_skill_reads_lookup
                ON thread_skill_reads(thread_id, kind, read_count DESC, last_read_at DESC);

            CREATE TABLE IF NOT EXISTS discoverable_guideline_documents (
                relative_path  TEXT PRIMARY KEY,
                excerpt        TEXT NOT NULL,
                last_seen_at   INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_discoverable_guideline_documents_seen
                ON discoverable_guideline_documents(last_seen_at DESC);

            CREATE TABLE IF NOT EXISTS thread_prompt_queue (
                id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                content TEXT NOT NULL,
                content_blocks_json TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_prompt_queue_thread_pos
                ON thread_prompt_queue(thread_id, position, created_at);

            CREATE TABLE IF NOT EXISTS pending_operator_questions (
                question_id TEXT PRIMARY KEY,
                thread_id TEXT NOT NULL,
                tool_call_id TEXT NOT NULL,
                session_id TEXT,
                content TEXT NOT NULL,
                options_json TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
    "#
}
