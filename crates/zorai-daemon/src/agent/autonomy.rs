//! Per-goal autonomy dial — controls how much the agent reports during goal runs.
//!
//! Three levels:
//! - **Autonomous**: suppresses intermediate events; operator sees only final report.
//! - **Aware** (default): reports on milestones — current behavior before this feature.
//! - **Supervised**: reports every significant step and waits for operator acknowledgment.

use serde::{Deserialize, Serialize};

/// Controls event-emission verbosity and acknowledgment gates for a goal run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    Autonomous,
    Aware,
    Supervised,
}

impl Default for AutonomyLevel {
    fn default() -> Self {
        Self::Aware
    }
}

impl AutonomyLevel {
    /// Parse a string into an `AutonomyLevel`, defaulting to `Aware` for unknown values.
    pub fn from_str_or_default(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "autonomous" => Self::Autonomous,
            "aware" => Self::Aware,
            "supervised" => Self::Supervised,
            _ => Self::Aware,
        }
    }
}

/// Determine whether an event should be emitted at the given autonomy level.
///
/// Event kinds:
/// - `"completed"`, `"failed"`, `"budget_alert"`, `"paused"` — always emitted.
/// - `"step_started"`, `"planning"`, `"step_completed"` — emitted at Aware and Supervised.
/// - `"step_detail"` — only emitted at Supervised.
#[cfg(test)]
pub fn should_emit_event(level: AutonomyLevel, event_kind: &str) -> bool {
    match level {
        AutonomyLevel::Autonomous => {
            matches!(
                event_kind,
                "completed" | "failed" | "budget_alert" | "paused"
            )
        }
        AutonomyLevel::Aware => event_kind != "step_detail",
        AutonomyLevel::Supervised => true,
    }
}

/// Returns `true` if the given autonomy level requires operator acknowledgment at step
/// boundaries.
#[cfg(test)]
pub fn requires_acknowledgment(level: AutonomyLevel) -> bool {
    matches!(level, AutonomyLevel::Supervised)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_aware() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::Aware);
    }

    #[test]
    fn should_emit_completed_in_autonomous() {
        assert!(should_emit_event(AutonomyLevel::Autonomous, "completed"));
    }

    #[test]
    fn should_suppress_step_started_in_autonomous() {
        assert!(!should_emit_event(
            AutonomyLevel::Autonomous,
            "step_started"
        ));
    }

    #[test]
    fn should_emit_step_started_in_aware() {
        assert!(should_emit_event(AutonomyLevel::Aware, "step_started"));
    }

    #[test]
    fn should_suppress_step_detail_in_aware() {
        assert!(!should_emit_event(AutonomyLevel::Aware, "step_detail"));
    }

    #[test]
    fn should_emit_step_detail_in_supervised() {
        assert!(should_emit_event(AutonomyLevel::Supervised, "step_detail"));
    }

    #[test]
    fn requires_acknowledgment_supervised_true() {
        assert!(requires_acknowledgment(AutonomyLevel::Supervised));
    }

    #[test]
    fn requires_acknowledgment_aware_false() {
        assert!(!requires_acknowledgment(AutonomyLevel::Aware));
    }

    #[test]
    fn requires_acknowledgment_autonomous_false() {
        assert!(!requires_acknowledgment(AutonomyLevel::Autonomous));
    }

    #[test]
    fn serde_round_trip() {
        for (level, expected) in [
            (AutonomyLevel::Autonomous, "\"autonomous\""),
            (AutonomyLevel::Aware, "\"aware\""),
            (AutonomyLevel::Supervised, "\"supervised\""),
        ] {
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, expected, "serialization mismatch for {level:?}");
            let back: AutonomyLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(back, level, "deserialization mismatch for {level:?}");
        }
    }

    #[test]
    fn from_str_or_default_known_values() {
        assert_eq!(
            AutonomyLevel::from_str_or_default("supervised"),
            AutonomyLevel::Supervised
        );
        assert_eq!(
            AutonomyLevel::from_str_or_default("AUTONOMOUS"),
            AutonomyLevel::Autonomous
        );
        assert_eq!(
            AutonomyLevel::from_str_or_default("Aware"),
            AutonomyLevel::Aware
        );
    }

    #[test]
    fn from_str_or_default_unknown_returns_aware() {
        assert_eq!(
            AutonomyLevel::from_str_or_default("turbo"),
            AutonomyLevel::Aware
        );
        assert_eq!(AutonomyLevel::from_str_or_default(""), AutonomyLevel::Aware);
    }

    #[test]
    fn autonomous_allows_failed_and_budget_alert() {
        assert!(should_emit_event(AutonomyLevel::Autonomous, "failed"));
        assert!(should_emit_event(AutonomyLevel::Autonomous, "budget_alert"));
    }

    #[test]
    fn autonomous_allows_paused_recovery_notice() {
        assert!(should_emit_event(AutonomyLevel::Autonomous, "paused"));
    }

    #[test]
    fn autonomous_suppresses_planning_and_step_completed() {
        assert!(!should_emit_event(AutonomyLevel::Autonomous, "planning"));
        assert!(!should_emit_event(
            AutonomyLevel::Autonomous,
            "step_completed"
        ));
    }

    #[test]
    fn aware_allows_planning_and_step_completed() {
        assert!(should_emit_event(AutonomyLevel::Aware, "planning"));
        assert!(should_emit_event(AutonomyLevel::Aware, "step_completed"));
    }
}
