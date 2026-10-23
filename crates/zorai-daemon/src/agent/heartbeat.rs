#![allow(dead_code)]

//! Heartbeat system — periodic health checks and notifications.
//!
//! Contains the structured heartbeat orchestration (`run_structured_heartbeat`) that:
//! - Gathers all built-in check results (stale todos, stuck goals, unreplied messages, repo changes)
//! - Collects custom HeartbeatItem prompts that are due
//! - Feeds everything into a single LLM synthesis call (BEAT-08/D-09)
//! - Broadcasts HeartbeatDigest only when actionable (BEAT-03/D-14)
//! - Persists every cycle to SQLite regardless of LLM outcome (D-12/Pitfall 4)

use super::*;
use chrono::Timelike;
use futures::StreamExt;
use std::collections::HashSet;

const HEARTBEAT_SYNTHESIS_SYSTEM_PROMPT: &str =
    "You summarize scheduled heartbeat check data for the operator. \
You cannot run tools or take actions; reply only with the requested ACTIONABLE / DIGEST / ITEMS text.";

mod helpers;
mod legacy;
mod postprocess;
use helpers::{
    check_quiet_window, check_type_to_action_type, format_anticipatory_items_for_heartbeat,
    format_speculative_summary_for_heartbeat, heartbeat_persistence_status, is_custom_item_due,
    parse_digest_items, should_broadcast, should_run_check,
};
#[cfg(test)]
pub(crate) use helpers::{compute_check_priority, enabled_checks};
pub(in crate::agent) use helpers::{is_peak_activity_hour, resolve_cron_from_config};

impl AgentEngine {
    /// Check if current time falls within quiet hours or DND is active. Per D-07.
    pub(super) async fn is_quiet_hours(&self) -> bool {
        let config = self.config.read().await;
        let hour = chrono::Local::now().hour();
        check_quiet_window(
            hour,
            config.quiet_hours_start,
            config.quiet_hours_end,
            config.dnd_enabled,
        )
    }

    /// Resolve the effective cron expression from config. Per D-08.
    /// Prefers heartbeat_cron if set, otherwise converts heartbeat_interval_mins.
    pub(super) async fn resolve_heartbeat_cron(&self) -> String {
        let config = self.config.read().await;
        resolve_cron_from_config(&config)
    }

    pub(super) async fn running_goal_trajectory_targets(&self) -> Vec<(String, String)> {
        let mut running_goal_runs = match self
            .history
            .list_goal_run_goal_refs_for_statuses(&[GoalRunStatus::Running])
            .await
        {
            Ok(goal_runs) => goal_runs,
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted running goal runs for heartbeat trajectories: {error}"
                );
                Vec::new()
            }
        };
        let mut seen_goal_ids = running_goal_runs
            .iter()
            .map(|(goal_run_id, _)| goal_run_id.clone())
            .collect::<HashSet<_>>();
        {
            let live_goal_runs = self.goal_runs.lock().await;
            for goal_run in live_goal_runs
                .iter()
                .filter(|goal_run| goal_run.status == GoalRunStatus::Running)
            {
                if seen_goal_ids.insert(goal_run.id.clone()) {
                    running_goal_runs.push((goal_run.id.clone(), goal_run.goal.clone()));
                }
            }
        }

        running_goal_runs
    }

    /// Run the structured heartbeat (backward-compatible wrapper).
    /// Delegates to `run_structured_heartbeat_adaptive(0)` so existing callers
    /// (tests, legacy code paths) continue to work without changes.
    pub(super) async fn run_structured_heartbeat(&self) -> Result<()> {
        self.run_structured_heartbeat_adaptive(0).await
    }

    /// Run the structured heartbeat with activity-aware priority gating and
    /// learned weight updates. `cycle_count` drives `should_run_check` modular
    /// gating: weight 1.0 = every cycle, 0.5 = every 2nd, etc.
    ///
    /// Per D-01, D-04, D-06, D-09, D-10, D-11, D-12, D-14, BEAT-02 through BEAT-08.
    pub(super) async fn run_structured_heartbeat_adaptive(&self, cycle_count: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let cycle_id = uuid::Uuid::new_v4().to_string();
        let now = now_millis();

        let config = self.config.read().await.clone();
        let checks_config = &config.heartbeat_checks;

        if checks_config.reset_learned_priorities {
            tracing::info!("resetting all learned priority weights to defaults");
            let mut weights = self.learned_check_weights.write().await;
            weights.clear();
            drop(weights);
        }

        let mut check_results: Vec<HeartbeatCheckResult> = Vec::new();
        {
            let learned_weights = self.learned_check_weights.read().await;

            let effective_stale_weight = checks_config
                .stale_todos_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::StaleTodos)
                        .copied()
                        .unwrap_or(checks_config.stale_todos_priority_weight)
                });

            let effective_stuck_weight = checks_config
                .stuck_goals_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::StuckGoalRuns)
                        .copied()
                        .unwrap_or(checks_config.stuck_goals_priority_weight)
                });

            let effective_unreplied_weight = checks_config
                .unreplied_messages_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::UnrepliedGatewayMessages)
                        .copied()
                        .unwrap_or(checks_config.unreplied_messages_priority_weight)
                });

            let effective_repo_weight = checks_config
                .repo_changes_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::RepoChanges)
                        .copied()
                        .unwrap_or(checks_config.repo_changes_priority_weight)
                });
            let effective_plugin_auth_weight = checks_config
                .plugin_auth_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::PluginAuth)
                        .copied()
                        .unwrap_or(checks_config.plugin_auth_priority_weight)
                });

            drop(learned_weights);

            if checks_config.stale_todos_enabled
                && should_run_check(effective_stale_weight, cycle_count)
            {
                check_results.push(
                    self.check_stale_todos(checks_config.stale_todo_threshold_hours)
                        .await,
                );
            }
            if checks_config.stuck_goals_enabled
                && should_run_check(effective_stuck_weight, cycle_count)
            {
                check_results.push(
                    self.check_stuck_goal_runs(checks_config.stuck_goal_threshold_hours)
                        .await,
                );
            }
            if checks_config.unreplied_messages_enabled
                && should_run_check(effective_unreplied_weight, cycle_count)
            {
                check_results.push(
                    self.check_unreplied_messages(checks_config.unreplied_message_threshold_hours)
                        .await,
                );
            }
            if checks_config.repo_changes_enabled
                && should_run_check(effective_repo_weight, cycle_count)
            {
                check_results.push(self.check_repo_changes().await);
            }
            if checks_config.plugin_auth_enabled
                && should_run_check(effective_plugin_auth_weight, cycle_count)
            {
                check_results.push(self.check_plugin_auth().await);
            }
        }

        // L2-response-timeout watcher → external (L3) escalation. Sweep every
        // tick: the operation is a single SQLite SELECT with an ORDER BY +
        // bounded predicate, and the L3 dispatcher is idempotent via
        // deterministic notification IDs, so re-running on each tick costs
        // little and guarantees no stuck-approval slips past the deadline by
        // more than one heartbeat interval. Errors are logged and swallowed —
        // missing one tick's escalation notice must not kill the heartbeat.
        if let Err(error) = self.dispatch_stale_approval_escalations_to_external().await {
            tracing::warn!(
                %error,
                "failed to dispatch stale-approval external escalations during heartbeat",
            );
        }

        // Per-goal-run health aggregation layer (the doc-described "health
        // monitor periodically evaluates the runtime"). Rolls up the
        // per-task SubagentRuntimeStats into a per-goal-run HealthIndicators
        // and runs each goal run's persistent HealthMonitor — its
        // hysteresis state machine prevents a single noisy tick from
        // flipping the run's state. Persisted rows complement the per-task
        // health log already written by gateway_loop/run_loop.rs:432;
        // entity_type discriminates ("goal_run" vs "task"). Errors are
        // logged and swallowed so a health-log write failure can't kill
        // the heartbeat.
        if let Err(error) = self.dispatch_goal_run_health_aggregation().await {
            tracing::warn!(
                %error,
                "failed to dispatch per-goal-run health aggregation during heartbeat",
            );
        }

        {
            let running = self.running_goal_trajectory_targets().await;

            for (gr_id, gr_goal) in &running {
                if let Some(traj) = self.get_awareness_trajectory(gr_id).await {
                    let _ = self.event_tx.send(AgentEvent::TrajectoryUpdate {
                        goal_run_id: gr_id.clone(),
                        direction: traj.label.to_string(),
                        progress_ratio: traj.progress_ratio,
                        message: format!(
                            "Goal '{}' trajectory: {} ({:.0}% progress ratio)",
                            gr_goal.chars().take(50).collect::<String>(),
                            traj.label,
                            traj.progress_ratio * 100.0,
                        ),
                    });
                }
            }
        }

        let custom_items = self.heartbeat_items.read().await.clone();
        let mut custom_summaries: Vec<String> = Vec::new();
        for item in &custom_items {
            if !item.enabled {
                continue;
            }
            if !is_custom_item_due(
                now,
                item.last_run_at,
                item.interval_minutes,
                config.heartbeat_interval_mins,
            ) {
                continue;
            }
            custom_summaries.push(format!("- Custom check '{}': {}", item.label, item.prompt));
        }

        let (anticipatory_items, is_first_heartbeat) = self.get_anticipatory_for_heartbeat().await;

        let speculative_summary = {
            let runtime = self.anticipatory.read().await;
            let queued = runtime
                .opportunity_queue
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let cached = runtime
                .speculative_results_by_thread
                .values()
                .flat_map(|results| results.iter().cloned())
                .collect::<Vec<_>>();
            format_speculative_summary_for_heartbeat(&queued, &cached, now)
        };
        let anticipatory_summary = {
            let mut parts = Vec::new();
            if !anticipatory_items.is_empty() {
                parts.push(format_anticipatory_items_for_heartbeat(&anticipatory_items));
            }
            if let Some(speculative_summary) = speculative_summary {
                parts.push(speculative_summary);
            }
            parts.join("\n")
        };

        let anticipatory_section = if anticipatory_summary.is_empty() {
            String::new()
        } else {
            format!("\n\n== Anticipatory Items ==\n{}", anticipatory_summary)
        };

        let morning_brief_note = if is_first_heartbeat {
            "\n\nNOTE: This is the FIRST heartbeat of a new session. Include a morning brief \
             summary of overnight changes, stuck items, and context for today's work. \
             Mark morning brief items clearly in your DIGEST."
        } else {
            ""
        };

        let mut learning_observations: Vec<String> = Vec::new();

        {
            let model = self.operator_model.read().await;
            let config_snap = self.config.read().await;
            let threshold = config_snap.ema_activity_threshold;
            drop(config_snap);

            let mut current_peaks: Vec<u8> = model
                .session_rhythm
                .smoothed_activity_histogram
                .iter()
                .filter(|(_, &v)| v >= threshold)
                .map(|(&h, _)| h)
                .collect();
            current_peaks.sort();

            let previous_peaks = &model.session_rhythm.peak_activity_hours_utc;

            let added: Vec<u8> = current_peaks
                .iter()
                .filter(|h| !previous_peaks.contains(h))
                .copied()
                .collect();
            let removed: Vec<u8> = previous_peaks
                .iter()
                .filter(|h| !current_peaks.contains(h))
                .copied()
                .collect();

            if added.len() + removed.len() > 2 && model.session_rhythm.session_count >= 5 {
                let peak_str = current_peaks
                    .iter()
                    .map(|h| format!("{}:00", h))
                    .collect::<Vec<_>>()
                    .join(", ");
                let data = serde_json::json!({ "peak_hours": peak_str });
                if let ExplanationResult::Template(explanation) =
                    generate_explanation("schedule_learned", &data)
                {
                    learning_observations.push(explanation);
                }
            }
        }

        {
            let weights = self.learned_check_weights.read().await;
            let check_names = [
                (HeartbeatCheckType::StaleTodos, "stale todos"),
                (HeartbeatCheckType::StuckGoalRuns, "stuck goals"),
                (
                    HeartbeatCheckType::UnrepliedGatewayMessages,
                    "unreplied messages",
                ),
                (HeartbeatCheckType::RepoChanges, "repo changes"),
                (HeartbeatCheckType::PluginAuth, "plugin auth"),
            ];
            for (check_type, check_name) in &check_names {
                if let Some(&weight) = weights.get(check_type) {
                    if weight < 0.5 {
                        let data =
                            serde_json::json!({ "check_name": check_name, "weight": weight });
                        if let ExplanationResult::Template(explanation) =
                            generate_explanation("check_deprioritized", &data)
                        {
                            learning_observations.push(explanation);
                        }
                    }
                }
            }
        }

        let learning_section = if learning_observations.is_empty() {
            String::new()
        } else {
            let observations = learning_observations
                .iter()
                .map(|obs| format!("- [learning] {}", obs))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n\n== Learning Observations ==\n{}", observations)
        };

        let built_in_summary = check_results
            .iter()
            .map(|r| {
                let mut s = format!(
                    "### {:?} ({})\n{}",
                    r.check_type,
                    if r.items_found > 0 {
                        format!("{} item(s)", r.items_found)
                    } else {
                        "clear".into()
                    },
                    r.summary
                );
                for detail in &r.details {
                    s.push_str(&format!(
                        "\n  - [{:?}] {} ({:.1}h): {}",
                        detail.severity, detail.label, detail.age_hours, detail.context
                    ));
                }
                s
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let custom_summary = if custom_summaries.is_empty() {
            "No custom checks configured.".to_string()
        } else {
            custom_summaries.join("\n")
        };

        let synthesis_prompt = format!(
            "HEARTBEAT SYNTHESIS\n\
             You are performing a scheduled heartbeat check for the operator. \
             Analyze the following data and respond in this exact format:\n\n\
             ACTIONABLE: true|false\n\
             DIGEST: <one-line natural language summary>\n\
             ITEMS:\n\
             - PRIORITY:<1-5> TYPE:<check_type> TITLE:<brief title> SUGGESTION:<suggested action>\n\
             ...\n\n\
             Rules:\n\
             - Set ACTIONABLE to true only if there are items genuinely needing operator attention\n\
             - Keep DIGEST concise (under 200 chars)\n\
             - Priority 1 is highest urgency\n\
             - If nothing is actionable, you may still note meta-observations in DIGEST \
               (e.g., 'All systems quiet for 3 days - is the project stalled?')\n\
             - Do NOT include items that are purely informational with no suggested action\n\
             - Anticipatory items marked LOW-PRIORITY INFORMATIONAL are context only, not actionable\n\
             - Learning observations should be mentioned in DIGEST when present\n\n\
             == Built-in Check Results ==\n{}\n\n\
             == Custom Check Results ==\n{}{}{}{}\n",
            built_in_summary,
            custom_summary,
            anticipatory_section,
            morning_brief_note,
            learning_section,
        );

        let checks_json = serde_json::to_string(&check_results).unwrap_or_default();
        let (synthesis_json, actionable, digest_text, digest_items, llm_tokens) =
            match self.request_heartbeat_synthesis(&synthesis_prompt).await {
                Ok((response, llm_tokens)) => {
                    let actionable = response.contains("ACTIONABLE: true");
                    let digest = response
                        .lines()
                        .find(|l| l.starts_with("DIGEST:"))
                        .map(|l| l.trim_start_matches("DIGEST:").trim().to_string())
                        .unwrap_or_else(|| {
                            if actionable {
                                "Items need attention.".into()
                            } else {
                                "All systems normal.".into()
                            }
                        });

                    let items = parse_digest_items(&response);

                    (Some(response), actionable, digest, items, llm_tokens)
                }
                Err(e) => {
                    tracing::error!("heartbeat LLM synthesis failed: {e}");
                    (None, false, format!("Synthesis failed: {e}"), vec![], 0u64)
                }
            };

        let duration_ms = start.elapsed().as_millis() as i64;
        let status = heartbeat_persistence_status(synthesis_json.as_deref());
        if let Err(e) = self
            .history
            .insert_heartbeat_history(
                &cycle_id,
                now as i64,
                &checks_json,
                synthesis_json.as_deref(),
                actionable,
                Some(&digest_text),
                llm_tokens as i64,
                duration_ms,
                status,
            )
            .await
        {
            tracing::warn!("failed to persist heartbeat history: {e}");
        }

        self.refresh_weles_health_from_heartbeat(now).await;

        let digest_explanation = if digest_items.is_empty() {
            None
        } else if digest_items.len() == 1 {
            let item = &digest_items[0];
            let action_type = check_type_to_action_type(&item.check_type);
            let item_data = serde_json::json!({
                "title": item.title,
                "hours": 0,
                "count": 1,
                "source": "heartbeat",
                "repo": "unknown",
            });
            match generate_explanation(action_type, &item_data) {
                ExplanationResult::Template(s) => Some(s),
                ExplanationResult::NeedsLlm => Some(item.title.clone()),
            }
        } else {
            let mut parts = Vec::new();
            for item in &digest_items {
                let action_type = check_type_to_action_type(&item.check_type);
                let item_data = serde_json::json!({
                    "title": item.title,
                    "hours": 0,
                    "count": 1,
                    "source": "heartbeat",
                    "repo": "unknown",
                });
                match generate_explanation(action_type, &item_data) {
                    ExplanationResult::Template(s) => parts.push(s),
                    ExplanationResult::NeedsLlm => parts.push(item.title.clone()),
                }
            }
            Some(format!(
                "Found {} items: {}",
                digest_items.len(),
                parts.join("; ")
            ))
        };

        if should_broadcast(actionable, &digest_items) {
            let _ = self.event_tx.send(AgentEvent::HeartbeatDigest {
                cycle_id: cycle_id.clone(),
                actionable,
                digest: digest_text.clone(),
                items: digest_items.clone(),
                checked_at: now,
                explanation: digest_explanation,
                confidence: None,
            });
        } else {
            tracing::debug!(cycle_id = %cycle_id, "heartbeat quiet tick — no broadcast");
        }

        if is_first_heartbeat && synthesis_json.is_some() {
            self.anticipatory.write().await.session_start_pending_at = None;
            tracing::info!("morning brief consumed in heartbeat digest");
        }

        self.finalize_heartbeat_postprocess(
            &cycle_id,
            now,
            &config,
            &digest_items,
            start,
            actionable,
            check_results.len(),
        )
        .await;

        Ok(())
    }

    pub async fn get_heartbeat_items(&self) -> Vec<HeartbeatItem> {
        self.heartbeat_items.read().await.clone()
    }

    pub async fn set_heartbeat_items(&self, items: Vec<HeartbeatItem>) {
        *self.heartbeat_items.write().await = items;
        self.persist_heartbeat().await;
    }
}

#[cfg(test)]
#[path = "tests/heartbeat.rs"]
mod tests;

impl AgentEngine {
    async fn request_heartbeat_synthesis(&self, prompt: &str) -> Result<(String, u64)> {
        let config = self.config.read().await.clone();
        let weles = super::config::build_effective_weles_definition(&config);
        let mut provider_config =
            resolve_provider_config_for(&config, &weles.provider, Some(&weles.model))?;
        crate::agent::provider_resolution::apply_role_transport_override(
            &weles.provider,
            &mut provider_config,
            weles.api_transport,
        );
        drop(config);

        self.check_circuit_breaker(&weles.provider).await?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            reasoning: None,
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let stream = send_completion_request(
            &self.http_client,
            &weles.provider,
            &provider_config,
            HEARTBEAT_SYNTHESIS_SYSTEM_PROMPT,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );
        let mut content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(&weles.provider, false).await;
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta { content: delta, .. } => content.push_str(&delta),
                CompletionChunk::Done {
                    content: done_content,
                    input_tokens,
                    output_tokens,
                    ..
                } => {
                    self.record_llm_outcome(&weles.provider, true).await;
                    if !done_content.trim().is_empty() {
                        content = done_content;
                    }
                    if content.trim().is_empty() {
                        anyhow::bail!("empty heartbeat synthesis response");
                    }
                    return Ok((content, input_tokens + output_tokens));
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&weles.provider, false).await;
                    anyhow::bail!("LLM error: {message}");
                }
                _ => {}
            }
        }
        anyhow::bail!("heartbeat synthesis stream ended without a final response")
    }
}
