//! Per-goal cost and token accounting with budget alerts.
//!
//! The cost module tracks prompt and completion token usage for each goal run,
//! converts tokens to estimated USD via provider rate cards, and fires a
//! one-shot budget alert when cumulative cost exceeds a configured threshold.
//!
//! Cost accumulation happens at exactly ONE point -- `agent_loop.rs` after
//! `CompletionChunk::Done` -- to prevent double-counting.

pub mod rate_cards;

pub use rate_cards::{lookup_rate, RateCard};

use crate::agent::types::GoalRunModelUsage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Operator-configurable cost tracking settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    /// Whether cost tracking is enabled.
    #[serde(default = "default_cost_enabled")]
    pub enabled: bool,
    /// Fire a BudgetAlert event when cumulative cost exceeds this USD amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_alert_threshold_usd: Option<f64>,
    /// Optional operator-supplied per-model pricing. Empty by default because
    /// prices are volatile and provider/account specific.
    #[serde(default)]
    pub rate_cards: HashMap<String, RateCard>,
}

fn default_cost_enabled() -> bool {
    true
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            budget_alert_threshold_usd: None,
            rate_cards: HashMap::new(),
        }
    }
}

/// Accumulated cost snapshot for a goal run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostSummary {
    #[serde(default)]
    pub total_prompt_tokens: u64,
    #[serde(default)]
    pub total_completion_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
}

/// Tracks per-goal-run token usage and estimated cost.
///
/// Each goal run gets its own `CostTracker` instance, keyed by goal_run_id
/// in `AgentEngine.cost_trackers`.
#[derive(Debug, Clone)]
pub struct CostTracker {
    summary: CostSummary,
    model_usage: BTreeMap<(String, String), GoalRunModelUsage>,
    budget_alerted: bool,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            summary: CostSummary::default(),
            model_usage: BTreeMap::new(),
            budget_alerted: false,
        }
    }

    /// Accumulate tokens from a single LLM call and compute incremental cost.
    ///
    /// Returns the incremental cost for this call if a rate card was found,
    /// `None` otherwise (tokens are still accumulated).
    pub fn accumulate(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        provider: &str,
        model: &str,
        rate_cards: &HashMap<String, RateCard>,
        duration_ms: Option<u64>,
    ) -> Option<f64> {
        self.summary.total_prompt_tokens += input_tokens;
        self.summary.total_completion_tokens += output_tokens;

        let cost = lookup_rate(rate_cards, provider, model).map(|rate| {
            let incremental = compute_cost_from_tokens(input_tokens, output_tokens, rate);
            let current = self.summary.estimated_cost_usd.unwrap_or(0.0);
            self.summary.estimated_cost_usd = Some(current + incremental);
            incremental
        });

        let entry = self
            .model_usage
            .entry((provider.to_string(), model.to_string()))
            .or_insert_with(|| GoalRunModelUsage {
                provider: provider.to_string(),
                model: model.to_string(),
                ..Default::default()
            });
        entry.request_count = entry.request_count.saturating_add(1);
        entry.prompt_tokens = entry.prompt_tokens.saturating_add(input_tokens);
        entry.completion_tokens = entry.completion_tokens.saturating_add(output_tokens);
        if let Some(incremental) = cost {
            let current = entry.estimated_cost_usd.unwrap_or(0.0);
            entry.estimated_cost_usd = Some(current + incremental);
        }
        if let Some(duration_ms) = duration_ms {
            let current = entry.duration_ms.unwrap_or(0);
            entry.duration_ms = Some(current.saturating_add(duration_ms));
        }

        cost
    }

    /// Returns a reference to the current cost summary.
    pub fn summary(&self) -> &CostSummary {
        &self.summary
    }

    pub fn model_usage(&self) -> Vec<GoalRunModelUsage> {
        self.model_usage.values().cloned().collect()
    }

    /// Returns `true` once when cumulative cost crosses the threshold.
    ///
    /// After firing once, subsequent calls return `false` even if cost
    /// continues to climb.
    pub fn budget_alert_needed(&mut self, threshold: Option<f64>) -> bool {
        if self.budget_alerted {
            return false;
        }
        if let (Some(threshold), Some(cost)) = (threshold, self.summary.estimated_cost_usd) {
            if cost >= threshold {
                self.budget_alerted = true;
                return true;
            }
        }
        false
    }
}

/// Pure function: compute USD cost from token counts and a rate card.
pub fn compute_cost_from_tokens(input: u64, output: u64, rate: &RateCard) -> f64 {
    (input as f64 * rate.input_per_million / 1_000_000.0)
        + (output as f64 * rate.output_per_million / 1_000_000.0)
}

/// Thread cost is the USD figure the provider returned. Missing cost stays missing.
pub fn reported_turn_cost(enabled: bool, reported_cost_usd: Option<f64>) -> Option<f64> {
    if !enabled {
        return None;
    }
    reported_cost_usd
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorai_shared::providers::PROVIDER_ID_OPENAI;

    fn explicitly_configured_test_rates() -> HashMap<String, RateCard> {
        HashMap::from([(
            "test-model".to_string(),
            RateCard {
                input_per_million: 2.50,
                output_per_million: 10.00,
            },
        )])
    }

    #[test]
    fn cost_tracker_new_has_zero_totals() {
        let tracker = CostTracker::new();
        let s = tracker.summary();
        assert_eq!(s.total_prompt_tokens, 0);
        assert_eq!(s.total_completion_tokens, 0);
        assert!(s.estimated_cost_usd.is_none());
    }

    #[test]
    fn cost_config_ships_without_any_rate_cards() {
        assert!(CostConfig::default().rate_cards.is_empty());

        let config: CostConfig = serde_json::from_str(r#"{"enabled":true}"#)
            .expect("cost config without explicit pricing should deserialize");
        assert!(config.rate_cards.is_empty());
    }

    #[test]
    fn cost_tracker_accumulate_adds_tokens_and_computes_usd() {
        let mut tracker = CostTracker::new();
        let cards = explicitly_configured_test_rates();

        let cost = tracker.accumulate(1000, 500, PROVIDER_ID_OPENAI, "test-model", &cards, None);
        assert!(cost.is_some());
        let c = cost.unwrap();
        let expected = (1000.0 * 2.50 / 1_000_000.0) + (500.0 * 10.00 / 1_000_000.0);
        assert!((c - expected).abs() < 1e-10, "expected {expected}, got {c}");

        let s = tracker.summary();
        assert_eq!(s.total_prompt_tokens, 1000);
        assert_eq!(s.total_completion_tokens, 500);
        assert!((s.estimated_cost_usd.unwrap() - expected).abs() < 1e-10);
    }

    #[test]
    fn reported_turn_cost_keeps_only_provider_usd() {
        assert_eq!(reported_turn_cost(true, Some(0.42)), Some(0.42));
        assert_eq!(reported_turn_cost(true, None), None);
        assert_eq!(reported_turn_cost(false, Some(0.42)), None);
    }

    #[test]
    fn cost_tracker_accumulate_unknown_model_returns_none() {
        let mut tracker = CostTracker::new();
        let cards = explicitly_configured_test_rates();

        let cost = tracker.accumulate(500, 200, "unknown", "fake-model-9000", &cards, None);
        assert!(cost.is_none());
        assert_eq!(tracker.summary().total_prompt_tokens, 500);
        assert_eq!(tracker.summary().total_completion_tokens, 200);
        assert!(tracker.summary().estimated_cost_usd.is_none());
    }

    #[test]
    fn cost_tracker_budget_alert_fires_once() {
        let mut tracker = CostTracker::new();
        let cards = explicitly_configured_test_rates();
        let threshold = Some(0.001);

        tracker.accumulate(100, 50, PROVIDER_ID_OPENAI, "test-model", &cards, None);
        assert!(!tracker.budget_alert_needed(threshold));

        tracker.accumulate(
            100_000,
            50_000,
            PROVIDER_ID_OPENAI,
            "test-model",
            &cards,
            None,
        );
        assert!(tracker.budget_alert_needed(threshold));

        assert!(!tracker.budget_alert_needed(threshold));
    }

    #[test]
    fn cost_tracker_budget_alert_none_threshold() {
        let mut tracker = CostTracker::new();
        let cards = explicitly_configured_test_rates();
        tracker.accumulate(
            1_000_000,
            500_000,
            PROVIDER_ID_OPENAI,
            "test-model",
            &cards,
            None,
        );
        assert!(!tracker.budget_alert_needed(None));
    }

    #[test]
    fn cost_tracker_keeps_per_model_usage_and_duration() {
        let mut tracker = CostTracker::new();
        let cards = explicitly_configured_test_rates();

        tracker.accumulate(
            1000,
            500,
            PROVIDER_ID_OPENAI,
            "test-model",
            &cards,
            Some(1200),
        );
        tracker.accumulate(
            2000,
            700,
            PROVIDER_ID_OPENAI,
            "test-model",
            &cards,
            Some(800),
        );
        tracker.accumulate(
            300,
            100,
            zorai_shared::providers::PROVIDER_ID_OPENROUTER,
            "unpriced-model",
            &cards,
            None,
        );

        let usage = tracker.model_usage();
        assert_eq!(usage.len(), 2);
        let gpt = usage
            .iter()
            .find(|entry| entry.provider == PROVIDER_ID_OPENAI && entry.model == "test-model")
            .expect("explicit test-model usage should be grouped");
        assert_eq!(gpt.request_count, 2);
        assert_eq!(gpt.prompt_tokens, 3000);
        assert_eq!(gpt.completion_tokens, 1200);
        assert_eq!(gpt.duration_ms, Some(2000));

        let unpriced = usage
            .iter()
            .find(|entry| {
                entry.provider == zorai_shared::providers::PROVIDER_ID_OPENROUTER
                    && entry.model == "unpriced-model"
            })
            .expect("unpriced provider usage should still be tracked");
        assert_eq!(unpriced.request_count, 1);
        assert_eq!(unpriced.prompt_tokens, 300);
        assert_eq!(unpriced.completion_tokens, 100);
        assert_eq!(unpriced.duration_ms, None);
        assert_eq!(unpriced.estimated_cost_usd, None);
    }

    #[test]
    fn cost_summary_serde_roundtrip() {
        let summary = CostSummary {
            total_prompt_tokens: 12345,
            total_completion_tokens: 6789,
            estimated_cost_usd: Some(1.23),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: CostSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_prompt_tokens, 12345);
        assert_eq!(decoded.total_completion_tokens, 6789);
        assert!((decoded.estimated_cost_usd.unwrap() - 1.23).abs() < 1e-10);
    }

    #[test]
    fn cost_summary_default_deserialize() {
        let decoded: CostSummary = serde_json::from_str("{}").unwrap();
        assert_eq!(decoded.total_prompt_tokens, 0);
        assert_eq!(decoded.total_completion_tokens, 0);
        assert!(decoded.estimated_cost_usd.is_none());
    }

    #[test]
    fn cost_goal_run_serde_backward_compat() {
        let json = r#"{
            "id": "goal_1",
            "title": "Test",
            "goal": "Do it",
            "status": "completed",
            "priority": "normal",
            "created_at": 1,
            "updated_at": 2,
            "current_step_index": 0,
            "replan_count": 0,
            "max_replans": 2,
            "steps": [],
            "events": []
        }"#;
        let goal_run: crate::agent::types::GoalRun = serde_json::from_str(json).unwrap();
        assert_eq!(goal_run.total_prompt_tokens, 0);
        assert_eq!(goal_run.total_completion_tokens, 0);
        assert!(goal_run.estimated_cost_usd.is_none());

        let mut gr = goal_run;
        gr.total_prompt_tokens = 1000;
        gr.total_completion_tokens = 500;
        gr.estimated_cost_usd = Some(0.05);
        let serialized = serde_json::to_string(&gr).unwrap();
        let roundtripped: crate::agent::types::GoalRun = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtripped.total_prompt_tokens, 1000);
        assert_eq!(roundtripped.total_completion_tokens, 500);
        assert!((roundtripped.estimated_cost_usd.unwrap() - 0.05).abs() < 1e-10);
    }

    #[test]
    fn cost_compute_cost_from_tokens_pure() {
        let rate = RateCard {
            input_per_million: 3.00,
            output_per_million: 15.00,
        };
        let cost = compute_cost_from_tokens(1_000_000, 1_000_000, &rate);
        assert!((cost - 18.00).abs() < 1e-10);
    }
}
