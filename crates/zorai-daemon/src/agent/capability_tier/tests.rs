use super::*;
use crate::agent::engine::AgentEngine;
use crate::agent::types::{
    AgentConfig, ApiTransport, AuthSource, LatestSkillDiscoveryState, ProviderConfig,
};
use crate::session_manager::SessionManager;
use std::sync::Arc;
use tempfile::TempDir;
use zorai_protocol::DaemonMessage;

fn now_millis_local() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis() as u64
}

async fn make_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let engine = AgentEngine::new_test(session_manager, config, temp_dir.path()).await;
    (engine, temp_dir)
}

fn provider_config(base_url: &str, model: &str, api_key: &str) -> ProviderConfig {
    ProviderConfig {
        presence_penalty: None,
        base_url: base_url.to_string(),
        model: model.to_string(),
        api_key: api_key.to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: String::new(),
        context_window_tokens: 0,
        response_schema: None,
        stop_sequences: None,
        temperature: None,
        top_p: None,
        top_k: None,
        metadata: None,
        service_tier: None,
        container: None,
        inference_geo: None,
        cache_control: None,
        max_tokens: None,
        anthropic_tool_choice: None,
        output_effort: None,
        openrouter_provider_order: Vec::new(),
        openrouter_provider_ignore: Vec::new(),
        openrouter_allow_fallbacks: None,
        openrouter_response_cache_enabled: false,
        huggingface_provider: None,
    }
}

fn make_signals() -> TierSignals {
    TierSignals {
        session_count: 0,
        unique_tools_used: 0,
        goal_runs_completed: 0,
        risk_tolerance: RiskTolerance::Moderate,
        user_self_assessment: None,
        user_override: None,
    }
}

#[test]
fn resolve_tier_no_signals_returns_newcomer() {
    let signals = make_signals();
    assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
}

#[test]
fn resolve_tier_familiar_thresholds() {
    let mut signals = make_signals();
    signals.session_count = 5;
    signals.unique_tools_used = 3;
    assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
}

#[test]
fn resolve_tier_power_user_thresholds() {
    let mut signals = make_signals();
    signals.goal_runs_completed = 3;
    signals.unique_tools_used = 5;
    assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
}

#[test]
fn resolve_tier_expert_thresholds() {
    let mut signals = make_signals();
    signals.goal_runs_completed = 10;
    signals.unique_tools_used = 8;
    signals.risk_tolerance = RiskTolerance::Aggressive;
    assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
}

#[test]
fn user_override_expert_always_returns_expert() {
    let mut signals = make_signals();
    signals.user_override = Some(CapabilityTier::Expert);
    assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
}

#[test]
fn user_override_newcomer_returns_newcomer_despite_high_signals() {
    let mut signals = make_signals();
    signals.session_count = 100;
    signals.unique_tools_used = 20;
    signals.goal_runs_completed = 50;
    signals.risk_tolerance = RiskTolerance::Aggressive;
    signals.user_override = Some(CapabilityTier::Newcomer);
    assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
}

#[test]
fn self_assessment_elevates_behavioral() {
    let mut signals = make_signals();
    signals.session_count = 5;
    signals.unique_tools_used = 3;
    signals.user_self_assessment = Some(CapabilityTier::PowerUser);
    assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
}

#[test]
fn self_assessment_does_not_demote_behavioral() {
    let mut signals = make_signals();
    signals.session_count = 5;
    signals.unique_tools_used = 3;
    signals.user_self_assessment = Some(CapabilityTier::Newcomer);
    assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
}

#[test]
fn tier_features_newcomer_sees_fewest() {
    let flags = tier_features_visible(CapabilityTier::Newcomer);
    assert!(!flags.show_goal_runs);
    assert!(!flags.show_task_queue);
    assert!(!flags.show_gateway_config);
    assert!(!flags.show_subagents);
    assert!(!flags.show_advanced_settings);
    assert!(!flags.show_memory_controls);
}

#[test]
fn tier_features_expert_sees_all() {
    let flags = tier_features_visible(CapabilityTier::Expert);
    assert!(flags.show_goal_runs);
    assert!(flags.show_task_queue);
    assert!(flags.show_gateway_config);
    assert!(flags.show_subagents);
    assert!(flags.show_advanced_settings);
    assert!(flags.show_memory_controls);
}

#[test]
fn disclosure_queue_returns_none_same_session() {
    let queue = DisclosureQueue {
        pending_features: vec![FeatureDisclosure {
            feature_id: "goal_runs".to_string(),
            tier: CapabilityTier::Familiar,
            title: "Goal Runs".to_string(),
            description: "Decompose objectives into steps".to_string(),
        }],
        disclosed_features: vec![],
        last_disclosure_session: 42,
    };
    assert!(queue.next_disclosure(42).is_none());
}

#[test]
fn disclosure_queue_returns_first_pending_different_session() {
    let queue = DisclosureQueue {
        pending_features: vec![FeatureDisclosure {
            feature_id: "goal_runs".to_string(),
            tier: CapabilityTier::Familiar,
            title: "Goal Runs".to_string(),
            description: "Decompose objectives into steps".to_string(),
        }],
        disclosed_features: vec![],
        last_disclosure_session: 41,
    };
    let disclosure = queue.next_disclosure(42);
    assert!(disclosure.is_some());
    assert_eq!(disclosure.unwrap().feature_id, "goal_runs");
}

#[test]
fn capability_tier_ordering() {
    assert!(CapabilityTier::Newcomer < CapabilityTier::Familiar);
    assert!(CapabilityTier::Familiar < CapabilityTier::PowerUser);
    assert!(CapabilityTier::PowerUser < CapabilityTier::Expert);
}

#[tokio::test]
async fn status_snapshot_includes_outage_metadata_for_open_provider() {
    let mut config = AgentConfig::default();
    config.providers.insert(
        zorai_shared::providers::PROVIDER_ID_CUSTOM.to_string(),
        provider_config("https://example.invalid/v1", "model-a", "valid-key"),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;
    {
        let breaker = engine
            .circuit_breakers
            .get(zorai_shared::providers::PROVIDER_ID_CUSTOM)
            .await;
        let mut breaker = breaker.lock().await;
        let now = super::super::now_millis();
        for offset in 0..5 {
            breaker.record_failure(now + offset);
        }
    }

    let snapshot = engine.get_status_snapshot().await;
    let DaemonMessage::AgentStatusResponse {
        provider_health_json,
        ..
    } = snapshot
    else {
        panic!("expected agent status response");
    };

    let health: serde_json::Value = serde_json::from_str(&provider_health_json).unwrap();
    let custom = health
        .get(zorai_shared::providers::PROVIDER_ID_CUSTOM)
        .expect("custom provider health");
    assert_eq!(
        custom.get("can_execute").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(custom.get("trip_count").and_then(|v| v.as_u64()), Some(1));
    assert!(
        custom.get("reason").and_then(|v| v.as_str()).is_some(),
        "expected outage reason in provider health snapshot"
    );
    assert!(
        custom
            .get("suggested_alternatives")
            .and_then(|v| v.as_array())
            .is_some(),
        "expected structured alternatives in provider health snapshot"
    );
}

#[tokio::test]
async fn status_snapshot_includes_aline_diagnostics() {
    let mut config = AgentConfig::default();
    config.skill_recommendation.discovery_backend = "mesh".to_string();
    let (engine, _temp_dir) = make_test_engine(config).await;
    engine
        .set_thread_skill_discovery_state(
            "thread-1",
            LatestSkillDiscoveryState {
                query: "debug panic".to_string(),
                confidence_tier: "strong".to_string(),
                recommended_skill: Some("systematic-debugging".to_string()),
                recommended_action: "request_approval systematic-debugging".to_string(),
                mesh_next_step: Some(crate::agent::skill_mesh::types::SkillMeshNextStep::ReadSkill),
                mesh_requires_approval: true,
                mesh_approval_id: Some("approval-1".to_string()),
                read_skill_identifier: Some("systematic-debugging".to_string()),
                skip_rationale: None,
                discovery_pending: false,
                skill_read_completed: true,
                compliant: false,
                updated_at: now_millis_local(),
            },
        )
        .await;
    engine.set_aline_startup_test_availability(true);
    engine
        .record_aline_startup_summary_for_tests(crate::agent::aline_startup::AlineStartupSummary {
            aline_available: true,
            watcher_initial_state: Some(crate::agent::aline_startup::WatcherState::Stopped),
            watcher_started: true,
            discovered_count: 4,
            selected_count: 2,
            imported_count: 1,
            generated_count: 1,
            short_circuit_reason: Some(
                crate::agent::aline_startup::AlineStartupShortCircuitReason::BudgetExhausted,
            ),
            skipped_recently_imported_count: 1,
            budget_exhausted: true,
            failure_stage: None,
            failure_message: None,
            recently_imported_session_ids: vec!["session-1".to_string()],
        })
        .await;

    let snapshot = engine.get_status_snapshot().await;
    let DaemonMessage::AgentStatusResponse {
        diagnostics_json, ..
    } = snapshot
    else {
        panic!("expected agent status response");
    };

    let diagnostics: serde_json::Value = serde_json::from_str(&diagnostics_json).unwrap();
    let aline = diagnostics
        .get("aline")
        .expect("aline diagnostics should exist");
    let skill_mesh = diagnostics
        .get("skill_mesh")
        .expect("skill mesh diagnostics should exist");
    let active_gate = skill_mesh
        .get("active_gate")
        .expect("active gate diagnostics should exist");
    assert_eq!(
        aline.get("available").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        aline.get("watcher_state").and_then(|value| value.as_str()),
        Some("running")
    );
    assert_eq!(
        aline
            .get("short_circuit_reason")
            .and_then(|value| value.as_str()),
        Some("budget_exhausted")
    );
    assert_eq!(
        aline.get("imported_count").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        skill_mesh.get("backend").and_then(|value| value.as_str()),
        Some("mesh")
    );
    assert_eq!(
        skill_mesh.get("state").and_then(|value| value.as_str()),
        Some("fresh")
    );
    assert_eq!(
        active_gate
            .get("recommended_skill")
            .and_then(|value| value.as_str()),
        Some("systematic-debugging")
    );
    assert_eq!(
        active_gate
            .get("recommended_action")
            .and_then(|value| value.as_str()),
        Some("request_approval systematic-debugging")
    );
    assert_eq!(
        active_gate
            .get("requires_approval")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        active_gate
            .get("rationale")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.as_str()),
        Some("matched debug panic")
    );
    assert_eq!(
        active_gate
            .get("capability_family")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|value| value.as_str()),
        Some("development")
    );
}

#[tokio::test]
async fn status_snapshot_uses_lightweight_polling_diagnostics() {
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    let (engine, _temp_dir) = make_test_engine(config).await;
    engine
        .record_operator_message("thread-diagnostics", "Please run tests.", true)
        .await
        .expect("record operator message");

    let snapshot = engine.get_status_snapshot().await;
    let DaemonMessage::AgentStatusResponse {
        diagnostics_json, ..
    } = snapshot
    else {
        panic!("expected agent status response");
    };

    let diagnostics: serde_json::Value = serde_json::from_str(&diagnostics_json).unwrap();
    assert!(
        diagnostics.get("operator_profile_sync_state").is_some(),
        "status polling still needs profile sync diagnostics"
    );
    assert!(
        diagnostics.get("aline").is_some(),
        "status polling still needs Aline watcher diagnostics"
    );
    assert!(
        diagnostics.get("skill_mesh").is_some(),
        "status polling still needs skill mesh diagnostics"
    );
    assert!(
        diagnostics.get("operator_satisfaction").is_none(),
        "status polling must not include heavyweight operator-model diagnostics"
    );
    assert!(
        diagnostics.get("emergent_protocols").is_none(),
        "status polling must not enumerate per-thread protocol stores"
    );
}

#[tokio::test]
async fn status_snapshot_uses_cached_skill_gate_skip_rationale() {
    let mut config = AgentConfig::default();
    config.skill_recommendation.discovery_backend = "mesh".to_string();
    let (engine, _temp_dir) = make_test_engine(config).await;
    engine
        .set_thread_skill_discovery_state(
            "thread-1",
            LatestSkillDiscoveryState {
                query: "obscure request with no local skill".to_string(),
                confidence_tier: "none".to_string(),
                recommended_skill: None,
                recommended_action: zorai_protocol::tool_names::JUSTIFY_SKILL_SKIP.to_string(),
                mesh_next_step: Some(
                    crate::agent::skill_mesh::types::SkillMeshNextStep::JustifySkillSkip,
                ),
                mesh_requires_approval: false,
                mesh_approval_id: None,
                read_skill_identifier: None,
                skip_rationale: Some("no local skill found".to_string()),
                discovery_pending: false,
                skill_read_completed: false,
                compliant: false,
                updated_at: now_millis_local(),
            },
        )
        .await;

    let snapshot = engine.get_status_snapshot().await;
    let DaemonMessage::AgentStatusResponse {
        diagnostics_json, ..
    } = snapshot
    else {
        panic!("expected agent status response");
    };

    let diagnostics: serde_json::Value = serde_json::from_str(&diagnostics_json).unwrap();
    let rationale = diagnostics
        .get("skill_mesh")
        .and_then(|value| value.get("active_gate"))
        .and_then(|value| value.get("rationale"))
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.as_str());

    assert_eq!(rationale, Some("no local skill found"));
}
