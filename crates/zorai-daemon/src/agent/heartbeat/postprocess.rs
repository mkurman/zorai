use std::collections::HashMap;

use super::*;
use crate::history::AuditEntryRow;

const SUPPRESSED_TRIGGER_FIRE_RETENTION_DAYS: u32 = 7;

impl AgentEngine {
    pub(super) async fn finalize_heartbeat_postprocess(
        &self,
        cycle_id: &str,
        now: u64,
        config: &AgentConfig,
        digest_items: &[HeartbeatDigestItem],
        start: std::time::Instant,
        actionable: bool,
        check_count: usize,
    ) {
        self.persist_heartbeat_audit(cycle_id, now, config, digest_items)
            .await;
        self.update_heartbeat_item_state(now).await;
        self.refresh_activity_smoothing().await;
        self.refresh_learned_check_weights().await;
        self.run_heartbeat_consolidation(start, cycle_id, actionable, check_count, config)
            .await;
    }

    async fn persist_heartbeat_audit(
        &self,
        cycle_id: &str,
        now: u64,
        config: &AgentConfig,
        digest_items: &[HeartbeatDigestItem],
    ) {
        if !config.audit.scope.heartbeat {
            return;
        }

        for (idx, item) in digest_items.iter().enumerate() {
            let action_type = super::helpers::check_type_to_action_type(&item.check_type);
            let item_data = serde_json::json!({
                "title": item.title,
                "suggestion": item.suggestion,
                "priority": item.priority,
                "check_type": format!("{:?}", item.check_type),
            });
            let summary = match generate_explanation(action_type, &item_data) {
                ExplanationResult::Template(s) => s,
                ExplanationResult::NeedsLlm => item.title.clone(),
            };
            let entry = AuditEntryRow {
                id: format!("audit-hb-{}-{}", cycle_id, idx),
                timestamp: now as i64,
                action_type: action_type.to_string(),
                summary: summary.clone(),
                explanation: Some(summary.clone()),
                confidence: None,
                confidence_band: None,
                causal_trace_id: None,
                thread_id: None,
                goal_run_id: None,
                task_id: None,
                raw_data_json: serde_json::to_string(&item).ok(),
            };
            if let Err(e) = self.history.insert_action_audit(&entry).await {
                tracing::warn!(cycle_id = %cycle_id, idx = idx, "failed to insert heartbeat audit entry: {e}");
            }
            let _ = self.event_tx.send(AgentEvent::AuditAction {
                id: entry.id.clone(),
                timestamp: now,
                action_type: entry.action_type.clone(),
                summary: entry.summary.clone(),
                explanation: entry.explanation.clone(),
                confidence: None,
                confidence_band: None,
                causal_trace_id: None,
                thread_id: None,
            });
        }

        match self
            .history
            .cleanup_action_audit(config.audit.max_entries, config.audit.max_age_days)
            .await
        {
            Ok(deleted) if deleted > 0 => {
                tracing::info!(deleted = deleted, "audit trail cleanup removed old entries");
            }
            Err(e) => {
                tracing::warn!("audit trail cleanup failed: {e}");
            }
            _ => {}
        }

        match self
            .history
            .prune_heartbeat_history_to_latest()
            .await
        {
            Ok(deleted) if deleted > 0 => {
                tracing::info!(deleted, "heartbeat history retention removed superseded cycles");
            }
            Err(e) => tracing::warn!("heartbeat history retention failed: {e}"),
            _ => {}
        }

        match self
            .history
            .prune_suppressed_trigger_fire_history(SUPPRESSED_TRIGGER_FIRE_RETENTION_DAYS)
            .await
        {
            Ok(deleted) if deleted > 0 => {
                tracing::info!(deleted, "trigger fire retention removed old suppressed fires");
            }
            Err(e) => tracing::warn!("trigger fire retention failed: {e}"),
            _ => {}
        }
    }

    async fn update_heartbeat_item_state(&self, now: u64) {
        {
            let mut items = self.heartbeat_items.write().await;
            for item in items.iter_mut() {
                if item.enabled {
                    item.last_run_at = Some(now);
                }
            }
        }
        self.persist_heartbeat().await;
    }

    async fn refresh_activity_smoothing(&self) {
        let config_snap = self.config.read().await;
        let alpha = config_snap.ema_alpha;
        drop(config_snap);

        let mut model = self.operator_model.write().await;
        let new_smoothed = smooth_activity_histogram(
            &model.session_rhythm.smoothed_activity_histogram,
            &model.session_rhythm.activity_hour_histogram,
            alpha,
        );
        model.session_rhythm.smoothed_activity_histogram = new_smoothed;
    }

    async fn refresh_learned_check_weights(&self) {
        let seven_days_ago_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64)
            - (7 * 24 * 3600 * 1000);

        let history = &self.history;
        let dismissals = history
            .count_dismissals_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();
        let shown = history
            .count_shown_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();
        let acted_on = history
            .count_acted_on_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();

        let check_types = [
            (HeartbeatCheckType::StaleTodos, "stale_todo"),
            (HeartbeatCheckType::StuckGoalRuns, "stuck_goal"),
            (
                HeartbeatCheckType::UnrepliedGatewayMessages,
                "unreplied_message",
            ),
            (HeartbeatCheckType::RepoChanges, "repo_change"),
            (HeartbeatCheckType::PluginAuth, "plugin_auth"),
        ];

        let decay_rate = 0.05;
        let recovery_rate = 0.1;

        let mut new_weights = HashMap::new();
        for (check_type, action_type_key) in &check_types {
            let dismiss_count = dismissals.get(*action_type_key).copied().unwrap_or(0);
            let total_shown = shown.get(*action_type_key).copied().unwrap_or(0);
            let recovery_count = acted_on.get(*action_type_key).copied().unwrap_or(0);
            let inaction_count = total_shown
                .saturating_sub(dismiss_count)
                .saturating_sub(recovery_count);

            let weight = super::helpers::compute_check_priority(
                dismiss_count,
                inaction_count,
                total_shown,
                recovery_count,
                decay_rate,
                recovery_rate,
            );
            new_weights.insert(*check_type, weight);
        }

        {
            let mut weights = self.learned_check_weights.write().await;
            *weights = new_weights;
        }

        let weights_snapshot = self.learned_check_weights.read().await.clone();
        tracing::debug!(
            weights = ?weights_snapshot,
            "updated learned check priority weights from feedback signals"
        );
    }

    async fn run_heartbeat_consolidation(
        &self,
        start: std::time::Instant,
        cycle_id: &str,
        actionable: bool,
        check_count: usize,
        config: &AgentConfig,
    ) {
        let consolidation_budget = std::time::Duration::from_secs(config.consolidation.budget_secs);
        let consolidation_result = self
            .maybe_run_consolidation_if_idle(consolidation_budget)
            .await;
        if let Some(ref result) = consolidation_result {
            if let Some(summary) = super::helpers::format_consolidation_forge_summary(result) {
                tracing::debug!(
                    details = %summary,
                    "consolidation carryover summary available via show_dreams; skipping workflow notice"
                );
            }
            if let Some(summary) = super::helpers::format_consolidation_dream_summary(result) {
                tracing::debug!(
                    details = %summary,
                    "dream-state carryover summary available via show_dreams; skipping workflow notice"
                );
            }
            tracing::info!(
                traces = result.traces_reviewed,
                distillation_ran = result.distillation_ran,
                distillation_threads = result.distillation_threads_analyzed,
                distillation_applied = result.distillation_auto_applied,
                forge_ran = result.forge_ran,
                forge_traces = result.forge_traces_analyzed,
                forge_patterns = result.forge_patterns_detected,
                forge_applied = result.forge_hints_auto_applied,
                decayed = result.facts_decayed,
                tombstones = result.tombstones_purged,
                refined = result.facts_refined,
                "consolidation tick completed"
            );
        }

        let duration_ms = start.elapsed().as_millis() as i64;
        let consolidated = consolidation_result.is_some();
        tracing::info!(
            cycle_id = %cycle_id,
            actionable = actionable,
            checks = check_count,
            consolidated = consolidated,
            duration_ms = duration_ms,
            "heartbeat cycle complete"
        );
    }
}
