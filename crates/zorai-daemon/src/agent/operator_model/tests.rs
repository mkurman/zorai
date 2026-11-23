use super::*;
use tempfile::tempdir;
use zorai_protocol::AgentDbMessage;

#[test]
fn cognitive_style_prefers_terse_for_short_messages() {
    assert_eq!(
        verbosity_preference_for_length(6.0),
        VerbosityPreference::Terse
    );
    assert_eq!(reading_depth_for_length(6.0), ReadingDepth::Skim);
}

#[test]
fn risk_metrics_compute_category_rates_and_tolerance() {
    let mut risk = RiskFingerprint {
        approvals: 4,
        denials: 1,
        category_requests: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 3),
        ]),
        category_approvals: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 2),
        ]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(risk.risk_tolerance, RiskTolerance::Aggressive);
    assert_eq!(
        risk.approval_rate_by_category.get("git").copied(),
        Some(1.0)
    );
    assert_eq!(
        risk.approval_rate_by_category
            .get("network_request")
            .copied(),
        Some(2.0 / 3.0)
    );
}

#[test]
fn classify_command_category_uses_command_heuristics_first() {
    assert_eq!(
        classify_command_category("rm -rf target", "highest"),
        "destructive_delete"
    );
    assert_eq!(
        classify_command_category("curl https://example.com", "moderate"),
        "network_request"
    );
}

#[test]
fn normalize_attention_surface_keeps_supported_separators() {
    assert_eq!(
        normalize_attention_surface(" modal:settings:SubAgents "),
        "modal:settings:subagents"
    );
}

#[test]
fn top_keys_orders_by_count_then_name() {
    let mut histogram = HashMap::new();
    histogram.insert("conversation:chat".to_string(), 4);
    histogram.insert("conversation:input".to_string(), 4);
    histogram.insert("modal:settings:provider".to_string(), 1);

    assert_eq!(
        top_keys(&histogram, 2),
        vec![
            "conversation:chat".to_string(),
            "conversation:input".to_string()
        ]
    );
}

#[test]
fn detect_revision_signal_finds_corrections() {
    assert_eq!(
        detect_revision_signal("Actually, use ripgrep instead."),
        RevisionSignal::Correction
    );
    assert_eq!(
        detect_revision_signal("Next time prefer the shorter answer."),
        RevisionSignal::Revision
    );
    assert_eq!(
        detect_revision_signal("Please run tests."),
        RevisionSignal::None
    );
}

#[test]
fn detect_reading_signal_finds_summary_and_reasoning_preferences() {
    assert_eq!(
        detect_reading_signal("Give me the tl;dr first."),
        ReadingSignal::SummaryRequest
    );
    assert_eq!(
        detect_reading_signal("Just the answer, no reasoning."),
        ReadingSignal::SkipReasoning
    );
    assert_eq!(
        detect_reading_signal("Show me the full trace and walk through it step by step."),
        ReadingSignal::DeepDetailRequest
    );
    assert_eq!(
        detect_reading_signal("Please run tests."),
        ReadingSignal::None
    );
}

#[test]
fn reading_depth_uses_behavioral_signals_not_just_message_length() {
    assert_eq!(reading_depth_for_profile(18.0, 3, 0, 2), ReadingDepth::Skim);
    assert_eq!(
        reading_depth_for_profile(8.0, 0, 3, 0),
        ReadingDepth::Standard
    );
}

#[test]
fn ema_update_basic_calculation() {
    let result = ema_update(10.0, 20.0, 0.3);
    assert!((result - 13.0).abs() < f64::EPSILON);
}

#[test]
fn ema_update_converges_toward_sample() {
    let mut value = 0.0;
    for _ in 0..50 {
        value = ema_update(value, 100.0, 0.3);
    }
    assert!((value - 100.0).abs() < 0.01);
}

#[test]
fn smooth_activity_histogram_applies_ema_to_all_24_hours() {
    let current: HashMap<u8, f64> = HashMap::new();
    let mut observation: HashMap<u8, u64> = HashMap::new();
    observation.insert(9, 5);
    observation.insert(14, 3);

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert_eq!(smoothed.len(), 24);
    assert!((smoothed[&9] - 1.5).abs() < f64::EPSILON);
    assert!((smoothed[&14] - 0.9).abs() < f64::EPSILON);
    assert!((smoothed[&0]).abs() < f64::EPSILON);
}

#[test]
fn smooth_activity_histogram_decays_unobserved_hours() {
    let mut current: HashMap<u8, f64> = HashMap::new();
    current.insert(9, 10.0);
    let observation: HashMap<u8, u64> = HashMap::new();

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert!((smoothed[&9] - 7.0).abs() < f64::EPSILON);
}

#[test]
fn record_attention_event_tracks_dwell_and_rapid_switches() {
    let mut model = OperatorModel::default();
    record_attention_event(&mut model, "conversation:chat", 1_000);
    record_attention_event(&mut model, "modal:settings", 6_000);
    record_attention_event(&mut model, "conversation:chat", 10_000);
    record_attention_event(&mut model, "conversation:chat", 50_000);

    assert_eq!(model.attention_topology.focus_event_count, 4);
    assert_eq!(model.attention_topology.rapid_switch_count, 2);
    assert_eq!(
        model.attention_topology.deep_focus_surface.as_deref(),
        Some("conversation:chat")
    );
    assert!(model.attention_topology.avg_surface_dwell_secs > 0.0);
}

#[test]
fn risk_metrics_learn_auto_approve_and_auto_deny_categories() {
    let mut risk = RiskFingerprint {
        approvals: 6,
        denials: 4,
        category_requests: HashMap::from([
            ("git".to_string(), 4),
            ("destructive_delete".to_string(), 4),
            ("network_request".to_string(), 2),
        ]),
        category_approvals: HashMap::from([
            ("git".to_string(), 4),
            ("destructive_delete".to_string(), 0),
            ("network_request".to_string(), 2),
        ]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(
        risk.auto_approve_categories,
        vec!["git".to_string()],
        "high-volume, always-approved categories should become shortcuts"
    );
    assert_eq!(
        risk.auto_deny_categories,
        vec!["destructive_delete".to_string()],
        "high-volume, never-approved categories should become auto-deny heuristics"
    );
}

#[test]
fn risk_metrics_learn_auto_deny_from_repeated_fast_denials() {
    let mut risk = RiskFingerprint {
        approvals: 1,
        denials: 3,
        category_requests: HashMap::from([("network_request".to_string(), 4)]),
        category_approvals: HashMap::from([("network_request".to_string(), 1)]),
        fast_denials_by_category: HashMap::from([("network_request".to_string(), 3)]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(
        risk.auto_deny_categories,
        vec!["network_request".to_string()],
        "three fast denials in one category should learn an approval bypass even before the long-run approval rate drops to zero"
    );
}

#[tokio::test]
async fn reset_operator_model_clears_learned_shortcuts() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.risk_fingerprint.auto_approve_categories = vec!["git".to_string()];
        model.risk_fingerprint.auto_deny_categories = vec!["destructive_delete".to_string()];
        persist_operator_model(&engine.data_dir, &model).expect("persist learned model");
    }

    engine
        .reset_operator_model()
        .await
        .expect("reset operator model");

    let json = engine
        .operator_model_json()
        .await
        .expect("reload operator model json");
    let parsed: OperatorModel = serde_json::from_str(&json).expect("parse operator model json");
    assert!(parsed.risk_fingerprint.auto_approve_categories.is_empty());
    assert!(parsed.risk_fingerprint.auto_deny_categories.is_empty());
}

#[tokio::test]
async fn repeated_fast_denials_enable_learned_auto_deny() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let command = "curl https://example.com/status";
    let risk_level = "moderate";

    let approved = ToolPendingApproval {
        approval_id: "approval-approve".to_string(),
        execution_id: "exec-approve".to_string(),
        command: command.to_string(),
        rationale: "Fetch service status".to_string(),
        risk_level: risk_level.to_string(),
        blast_radius: "single endpoint".to_string(),
        reasons: vec!["network access".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&approved)
        .await
        .expect("record approval request");
    engine
        .record_operator_approval_resolution(&approved.approval_id, ApprovalDecision::ApproveOnce)
        .await
        .expect("record approval resolution");

    for idx in 0..3 {
        let denial = ToolPendingApproval {
            approval_id: format!("approval-deny-{idx}"),
            execution_id: format!("exec-deny-{idx}"),
            command: command.to_string(),
            rationale: "Fetch service status".to_string(),
            risk_level: risk_level.to_string(),
            blast_radius: "single endpoint".to_string(),
            reasons: vec!["network access".to_string()],
            session_id: None,
        };
        engine
            .record_operator_approval_requested(&denial)
            .await
            .expect("record denial request");
        engine
            .record_operator_approval_resolution(&denial.approval_id, ApprovalDecision::Deny)
            .await
            .expect("record denial resolution");
    }

    assert!(matches!(
        engine.learned_approval_decision(command, risk_level).await,
        Some(ApprovalDecision::Deny)
    ));
}

#[tokio::test]
async fn high_approval_latency_suppresses_duplicate_low_value_approval_bundles() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
    }

    let existing = ToolPendingApproval {
        approval_id: "approval-existing".to_string(),
        execution_id: "exec-existing".to_string(),
        command: "git status".to_string(),
        rationale: "Inspect repo status".to_string(),
        risk_level: "lowest".to_string(),
        blast_radius: "repo metadata".to_string(),
        reasons: vec!["reads git metadata".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&existing)
        .await
        .expect("record existing approval request");

    let duplicate = ToolPendingApproval {
        approval_id: "approval-duplicate".to_string(),
        execution_id: "exec-duplicate".to_string(),
        command: "git diff --stat".to_string(),
        rationale: "Inspect repo delta".to_string(),
        risk_level: "lowest".to_string(),
        blast_radius: "repo metadata".to_string(),
        reasons: vec!["reads git metadata".to_string()],
        session_id: None,
    };

    assert!(
        engine
            .should_suppress_duplicate_low_value_approval_bundle(&duplicate)
            .await
    );
}

#[tokio::test]
async fn high_approval_latency_keeps_high_value_approvals_visible() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
    }

    let existing = ToolPendingApproval {
        approval_id: "approval-existing".to_string(),
        execution_id: "exec-existing".to_string(),
        command: "git status".to_string(),
        rationale: "Inspect repo status".to_string(),
        risk_level: "lowest".to_string(),
        blast_radius: "repo metadata".to_string(),
        reasons: vec!["reads git metadata".to_string()],
        session_id: None,
    };
    engine
        .record_operator_approval_requested(&existing)
        .await
        .expect("record existing approval request");

    let high_value = ToolPendingApproval {
        approval_id: "approval-high-value".to_string(),
        execution_id: "exec-high-value".to_string(),
        command: "curl https://example.com/status".to_string(),
        rationale: "Fetch service status".to_string(),
        risk_level: "moderate".to_string(),
        blast_radius: "single endpoint".to_string(),
        reasons: vec!["network access".to_string()],
        session_id: None,
    };

    assert!(
        !engine
            .should_suppress_duplicate_low_value_approval_bundle(&high_value)
            .await
    );
}

#[tokio::test]
async fn high_confirmation_seeking_suppresses_learned_auto_approve_shortcuts() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.cognitive_style.confirmation_seeking = 0.92;
        model.risk_fingerprint.auto_approve_categories = vec!["git".to_string()];
    }

    assert!(
        engine
            .learned_approval_decision("git status", "lowest")
            .await
            .is_none(),
        "high confirmation-seeking should suppress learned auto-approval and require explicit approval"
    );
}

#[tokio::test]
async fn operator_messages_learn_summary_first_reasoning_on_demand_preferences() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let messages = [
        "Give me the tl;dr first.",
        "Use the short version again.",
        "Just the answer, no reasoning.",
        "Skip the explanation and summarize it.",
    ];
    for (index, message) in messages.iter().enumerate() {
        engine
            .record_operator_message("thread-reading", message, index == 0)
            .await
            .expect("record operator message");
    }

    let json = engine
        .operator_model_json()
        .await
        .expect("read operator model json");
    let parsed: OperatorModel = serde_json::from_str(&json).expect("parse operator model json");
    assert!(parsed.cognitive_style.prefers_summaries);
    assert!(parsed.cognitive_style.skips_reasoning);
    assert_eq!(parsed.cognitive_style.reading_depth, ReadingDepth::Skim);

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("summary-first"));
    assert!(summary.contains("reasoning on demand"));
}

#[test]
fn operator_satisfaction_uses_signal_gates_and_friction() {
    let mut model = OperatorModel::default();
    refresh_operator_satisfaction(&mut model);
    assert_eq!(model.operator_satisfaction.label, "unknown");
    assert!((model.operator_satisfaction.score - 0.8).abs() < f64::EPSILON);
    assert!(model.diagnostic_summary().contains("strong >=0.80"));

    model.cognitive_style.message_count = 1;
    refresh_operator_satisfaction(&mut model);
    assert_eq!(model.operator_satisfaction.label, "strong");
    assert!((model.operator_satisfaction.score - 0.8).abs() < f64::EPSILON);
    assert!(model.diagnostic_summary().contains("signal present"));

    model.implicit_feedback.tool_hesitation_count = 1;
    model.implicit_feedback.revision_message_count = 1;
    model.implicit_feedback.correction_message_count = 1;
    model.implicit_feedback.fast_denial_count = 1;
    model.attention_topology.rapid_switch_count = 2;
    refresh_operator_satisfaction(&mut model);

    assert_eq!(model.operator_satisfaction.label, "strained");
    assert!((model.operator_satisfaction.score - 0.18).abs() < 1e-9);
}

#[test]
fn operator_satisfaction_uses_friction_rates_so_historical_counts_can_recover() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 35;
    model.attention_topology.focus_event_count = 555;
    model.attention_topology.rapid_switch_count = 294;
    model.implicit_feedback.tool_hesitation_count = 47;
    model.implicit_feedback.revision_message_count = 5;
    model.implicit_feedback.rapid_revert_count = 13;

    refresh_operator_satisfaction(&mut model);

    assert_eq!(model.operator_satisfaction.label, "healthy");
    assert!(
        model.operator_satisfaction.score >= 0.65,
        "historical friction should not permanently clamp satisfaction to strained"
    );
}

#[test]
fn operator_model_diagnostic_summary_exposes_thresholds_and_friction() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 1;
    model.implicit_feedback.tool_hesitation_count = 2;
    model.implicit_feedback.correction_message_count = 1;
    model.attention_topology.rapid_switch_count = 3;
    refresh_operator_satisfaction(&mut model);

    let summary = model.diagnostic_summary();
    assert!(summary.contains("satisfaction="));
    assert!(summary.contains("strained <0.35, fragile <0.55, healthy <0.80, strong >=0.80"));
    assert!(summary.contains("signal present"));
    assert!(summary.contains("corrections 1"));
    assert!(summary.contains("tool fallbacks 2"));
    assert!(summary.contains("rapid switches 3"));
    assert!(summary.contains("rapid reverts 0"));
}

#[tokio::test]
async fn tool_hesitation_refreshes_persisted_operator_satisfaction_and_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-satisfaction", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    let json = engine
        .operator_model_json()
        .await
        .expect("read operator model json");
    let parsed: OperatorModel = serde_json::from_str(&json).expect("parse operator model json");
    assert_eq!(parsed.cognitive_style.message_count, 1);
    assert_eq!(parsed.implicit_feedback.tool_hesitation_count, 1);
    assert_eq!(parsed.operator_satisfaction.label, "healthy");
    assert!((parsed.operator_satisfaction.score - 0.68).abs() < 1e-9);

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("Implicit feedback: 1 tool fallback(s), 0 revision-style operator message(s), 0 fast denial(s); common fallback read_file -> search_files"));
    assert!(summary.contains("Satisfaction signal: healthy (0.68); friction markers revisions 0, corrections 0, tool fallbacks 1, fast denials 0"));
    assert!(summary.contains("Adaptive response mode: keep a normal proactive cadence"));
    assert!(summary.contains("prefer the later successful fallback earlier"));
}

#[tokio::test]
async fn strong_operator_satisfaction_adds_proactive_guidance_without_friction() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-strong", "Please run tests.", true)
        .await
        .expect("record operator message");

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("Satisfaction signal: strong (0.80); friction markers revisions 0, corrections 0, tool fallbacks 0, fast denials 0"));
    assert!(summary
        .contains("Adaptive response mode: trust is high, so stay proactive and exploratory"));
    assert!(summary.contains("Adaptive delivery rule: start with the conclusion"));
}

#[tokio::test]
async fn fast_aggressive_approvals_add_proactive_approval_guidance() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 4;
        model.risk_fingerprint.denials = 0;
        model.risk_fingerprint.avg_response_time_secs = 3.0;
        model.risk_fingerprint.risk_tolerance = RiskTolerance::Aggressive;
        refresh_operator_satisfaction(&mut model);
    }

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains(
        "Risk tolerance: aggressive (4 approvals across 4 approval requests, avg response 3.0s)"
    ));
    assert!(summary.contains(
        "Adaptive approval rule: approvals resolve quickly and usually favor proceeding"
    ));
    assert!(summary.contains("avoid redundant confirmation loops for low-risk progress"));
}

#[tokio::test]
async fn slow_approval_latency_adds_deliberate_approval_guidance() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_approval_learning = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        model.risk_fingerprint.risk_tolerance = RiskTolerance::Moderate;
        refresh_operator_satisfaction(&mut model);
    }

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains(
        "Risk tolerance: moderate (2 approvals across 4 approval requests, avg response 45.0s)"
    ));
    assert!(summary.contains("Adaptive approval rule: approval responses are deliberate"));
    assert!(summary.contains("keep only one pending approval live at a time"));
}

#[tokio::test]
async fn strained_operator_satisfaction_adds_recovery_guidance() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-strained", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    {
        let mut model = engine.operator_model.write().await;
        model.implicit_feedback.revision_message_count = 1;
        model.implicit_feedback.correction_message_count = 1;
        model.implicit_feedback.fast_denial_count = 1;
        model.attention_topology.rapid_switch_count = 2;
        refresh_operator_satisfaction(&mut model);
    }

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("Satisfaction signal: strained (0.18); friction markers revisions 1, corrections 1, tool fallbacks 1, fast denials 1"));
    assert!(summary.contains("Adaptive response mode: reduce friction aggressively"));
    assert!(summary.contains("Adaptive delivery rule: keep the answer compact"));
    assert!(summary.contains("Adaptive clarification rule: when intent is underspecified, ask one targeted question before guessing broadly"));
    assert!(summary.contains("prefer the later successful fallback earlier"));
}

#[tokio::test]
async fn fragile_operator_satisfaction_adds_compact_delivery_and_clarification_guidance() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.operator_satisfaction.label = "fragile".to_string();
        model.operator_satisfaction.score = 0.54;
        model.implicit_feedback.correction_message_count = 1;
    }

    let summary = engine
        .build_operator_model_prompt_summary()
        .await
        .expect("operator model prompt summary");
    assert!(summary.contains("Satisfaction signal: fragile (0.54); friction markers revisions 0, corrections 1, tool fallbacks 0, fast denials 0"));
    assert!(summary.contains("Adaptive response mode: tighten the loop"));
    assert!(summary.contains("Adaptive delivery rule: keep the answer compact"));
    assert!(summary.contains("Adaptive clarification rule: when intent is underspecified, ask one targeted question before guessing broadly"));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_operator_satisfaction_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-diagnostics", "Please run tests.", true)
        .await
        .expect("record operator message");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let satisfaction = &snapshot["operator_satisfaction"];
    assert_eq!(satisfaction["label"], "strong");
    assert_eq!(satisfaction["message_count"], 1);
    let summary = satisfaction["summary"]
        .as_str()
        .expect("operator satisfaction summary string");
    assert!(summary.contains("strong >=0.80"));
    assert!(summary.contains("signal present"));
    let resonance = &snapshot["cognitive_resonance"];
    assert_eq!(resonance["state"], "flow");
    assert_eq!(resonance["compact_response"].as_bool(), Some(false));
    assert!(resonance["adjustments"]["proactiveness"]
        .as_f64()
        .is_some_and(|value| value >= 0.8));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_ranked_intent_prediction_confidences() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = engine
        .enqueue_task(
            "Need approval".to_string(),
            "Need approval".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-intent-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    task.status = TaskStatus::AwaitingApproval;
    // `enqueue_task` routes the 11th param into `parent_thread_id` and
    // leaves `thread_id = None`. Set `thread_id` so the predictor's
    // persisted-history query hits the primary column.
    task.thread_id = Some("thread-intent-diag".to_string());
    // `refresh_task_queue_state` (called from every `list_tasks_filtered`,
    // including the one inside `run_anticipatory_tick`) treats
    // `AwaitingApproval` with no `awaiting_approval_id` as a stale state
    // and reverts the task to `Queued`. Set an approval id so the test
    // fixture matches the production invariant — an AwaitingApproval task
    // always carries the id of the approval it's blocked on.
    task.awaiting_approval_id = Some(format!("approval-{}", task.id));
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|existing| existing.id == task.id) {
            *existing = task.clone();
        }
    }
    engine.persist_tasks().await;
    engine
        .record_operator_attention("conversation:chat", Some("thread-intent-diag"), None)
        .await
        .expect("record operator attention");

    engine.run_anticipatory_tick().await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let intent = &snapshot["intent_prediction"];
    assert_eq!(intent["primary_action"], "review pending approval");
    assert!(intent["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.86));
    let ranked = intent["ranked_actions"]
        .as_array()
        .expect("ranked actions should be present in diagnostics");
    assert!(ranked.len() >= 3);
    assert_eq!(ranked[0]["rank"].as_u64(), Some(1));
    assert_eq!(ranked[0]["action"], "review pending approval");
    assert!(ranked[0]["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.86));
}

#[tokio::test]
async fn status_diagnostics_snapshot_exposes_cached_prewarm_for_intent_prediction() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.predictive_hydration = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-intent-cache-diag"), None)
        .await
        .expect("record operator attention");
    engine.thread_work_contexts.write().await.insert(
        "thread-intent-cache-diag".to_string(),
        ThreadWorkContext {
            thread_id: "thread-intent-cache-diag".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/main.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some("/tmp/repo".to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine.anticipatory.write().await.prewarm_cache_by_thread.insert(
        "thread-intent-cache-diag".to_string(),
        crate::agent::anticipatory::AnticipatoryPrewarmSnapshot {
            summary: "branch main; dirty=true; modified 1; staged 0; untracked 0; ahead 0; behind 0; context entries 1".to_string(),
            precomputation_id: None,
        },
    );

    engine.run_anticipatory_tick().await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let intent = &snapshot["intent_prediction"];
    assert_eq!(
        intent["thread_id"].as_str(),
        Some("thread-intent-cache-diag")
    );
    assert_eq!(
        intent["cached_prewarm_summary"].as_str(),
        Some(
            "branch main; dirty=true; modified 1; staged 0; untracked 0; ahead 0; behind 0; context entries 1"
        )
    );
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_memory_distillation_activity() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .history
        .append_memory_distillation_log(
            "thread-distill-diag",
            Some("last_turn"),
            None,
            "Use the cargo package name `zorai-daemon` for `cargo -p`.",
            "MEMORY.md",
            "convention",
            0.91,
            1_717_190_001,
            true,
            "rarog",
        )
        .await
        .expect("append distillation log");
    engine
        .history
        .upsert_memory_distillation_progress(&crate::history::MemoryDistillationProgressRow {
            source_thread_id: "thread-distill-diag".to_string(),
            last_processed_cursor: crate::history::AgentMessageCursor {
                created_at: 1_717_190_000,
                message_id: "m-last".to_string(),
            },
            last_processed_span: Some(crate::history::AgentMessageSpan::LastTurn {
                message: crate::history::AgentMessageCursor {
                    created_at: 1_717_190_000,
                    message_id: "m-last".to_string(),
                },
            }),
            last_run_at_ms: 1_717_190_010,
            updated_at_ms: 1_717_190_020,
            agent_id: "rarog".to_string(),
        })
        .await
        .expect("upsert distillation progress");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let distillation = &snapshot["memory_distillation"];
    let recent = distillation["recent_activity"]
        .as_array()
        .expect("recent distillation activity array");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["source_thread_id"], "thread-distill-diag");
    assert_eq!(recent[0]["target_file"], "MEMORY.md");
    assert_eq!(recent[0]["category"], "convention");
    assert_eq!(recent[0]["applied_to_memory"].as_bool(), Some(true));
    assert!(recent[0]["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.9));

    let progress = distillation["progress_by_thread"]
        .as_array()
        .expect("distillation progress array");
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0]["source_thread_id"], "thread-distill-diag");
    assert_eq!(progress[0]["agent_id"], "rarog");
    assert_eq!(progress[0]["last_processed_message_id"], "m-last");
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_forge_pass_activity() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .history
        .conn
        .call(|conn| {
            conn.execute(
                "INSERT INTO forge_pass_log (agent_id, period_start_ms, period_end_ms, traces_analyzed, patterns_found, hints_applied, hints_logged, completed_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    "svarog",
                    1_717_200_000_i64,
                    1_717_203_600_i64,
                    11_i64,
                    3_i64,
                    1_i64,
                    2_i64,
                    1_717_203_700_i64,
                ],
            )?;
            Ok(())
        })
        .await
        .expect("insert forge pass log");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let forge = &snapshot["forge_reflection"];
    let passes = forge["recent_passes"]
        .as_array()
        .expect("recent forge passes array");
    assert_eq!(passes.len(), 1);
    assert_eq!(passes[0]["agent_id"], "svarog");
    assert_eq!(passes[0]["traces_analyzed"].as_i64(), Some(11));
    assert_eq!(passes[0]["patterns_found"].as_i64(), Some(3));
    assert_eq!(passes[0]["hints_applied"].as_i64(), Some(1));
    assert_eq!(passes[0]["hints_logged"].as_i64(), Some(2));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_routing_confidence() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .history
        .conn
        .call(|conn| {
            conn.execute(
                "INSERT INTO handoff_log (id, from_task_id, to_specialist_id, to_task_id, task_description, acceptance_criteria_json, context_bundle_json, capability_tags_json, handoff_depth, outcome, confidence_band, routing_method, routing_score, fallback_used, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                rusqlite::params![
                    "handoff-diag-1",
                    "task-parent-1",
                    "specialist-research",
                    "task-child-1",
                    "Investigate routing confidence",
                    "{}",
                    "{}",
                    serde_json::json!(["research", "analysis"]).to_string(),
                    0_i64,
                    "dispatched",
                    Option::<String>::None,
                    "probabilistic",
                    0.92_f64,
                    0_i64,
                    1_717_210_000_i64,
                ],
            )?;
            Ok(())
        })
        .await
        .expect("insert handoff log");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let routing = &snapshot["routing_decision"];
    assert_eq!(routing["specialist_id"], "specialist-research");
    assert_eq!(routing["routing_method"], "probabilistic");
    assert_eq!(routing["fallback_used"].as_bool(), Some(false));
    assert!(routing["routing_score"]
        .as_f64()
        .is_some_and(|value| value >= 0.9));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_latest_debate_session_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let session_id = engine
        .start_debate_session("cache strategy", None, "thread-debate-diag", Some("goal-1"))
        .await
        .expect("start debate session");

    let _ = engine
        .complete_debate_session(&session_id)
        .await
        .expect("complete debate session");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let debate = &snapshot["debate_session"];
    assert_eq!(debate["session_id"].as_str(), Some(session_id.as_str()));
    assert_eq!(debate["topic"].as_str(), Some("cache strategy"));
    assert_eq!(debate["status"].as_str(), Some("completed"));
    assert_eq!(
        debate["completion_reason"].as_str(),
        Some("manual_completion")
    );
    assert!(debate["current_round"].as_u64().is_some());
    assert!(debate["max_rounds"].as_u64().is_some());
    assert_eq!(debate["has_verdict"].as_bool(), Some(true));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_recursive_subagent_tree_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Parent coordinator".to_string(),
            "Coordinate the child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Depth child".to_string(),
            "Inspect deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/3".to_string());

    let mut grandchild = engine
        .enqueue_task(
            "Grandchild helper".to_string(),
            "Inspect one narrow edge case".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(child.id.clone()),
            Some("thread-parent-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    grandchild.containment_scope = Some("subagent-depth:2/3".to_string());

    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == grandchild.id) {
            *existing = grandchild.clone();
        }
    }

    let snapshot = engine.status_diagnostics_snapshot().await;
    let subtree = &snapshot["recursive_subagents"];
    assert_eq!(subtree["active_subagent_count"].as_u64(), Some(2));
    assert_eq!(subtree["max_observed_depth"].as_u64(), Some(2));
    assert_eq!(subtree["max_observed_allowed_depth"].as_u64(), Some(3));
    let roots = subtree["root_parent_task_ids"]
        .as_array()
        .expect("root parent task ids array");
    assert!(roots
        .iter()
        .any(|value| value.as_str() == Some(parent.id.as_str())));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_persisted_recursive_subagent_tree_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let parent = engine
        .enqueue_task(
            "Persisted parent coordinator".to_string(),
            "Coordinate persisted child work".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-parent-persisted-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;

    let mut child = engine
        .enqueue_task(
            "Persisted depth child".to_string(),
            "Inspect persisted deployment risks".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(parent.id.clone()),
            Some("thread-parent-persisted-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    child.containment_scope = Some("subagent-depth:1/3".to_string());

    let mut grandchild = engine
        .enqueue_task(
            "Persisted grandchild helper".to_string(),
            "Inspect one persisted edge case".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "subagent",
            None,
            Some(child.id.clone()),
            Some("thread-parent-persisted-diag".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    grandchild.containment_scope = Some("subagent-depth:2/3".to_string());

    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == child.id) {
            *existing = child.clone();
        }
        if let Some(existing) = tasks.iter_mut().find(|task| task.id == grandchild.id) {
            *existing = grandchild.clone();
        }
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let snapshot = engine.status_diagnostics_snapshot().await;
    let subtree = &snapshot["recursive_subagents"];
    assert_eq!(subtree["active_subagent_count"].as_u64(), Some(2));
    assert_eq!(subtree["max_observed_depth"].as_u64(), Some(2));
    assert_eq!(subtree["max_observed_allowed_depth"].as_u64(), Some(3));
    let roots = subtree["root_parent_task_ids"]
        .as_array()
        .expect("root parent task ids array");
    assert!(roots
        .iter()
        .any(|value| value.as_str() == Some(parent.id.as_str())));
}

#[test]
fn preferred_tool_fallback_targets_deduplicates_and_skips_invalid_pairs() {
    let preferred = preferred_tool_fallback_targets(
        &[
            "read_file -> search_files".to_string(),
            "READ_FILE -> Search_Files".to_string(),
            "search_files -> read_file".to_string(),
            "invalid-pair".to_string(),
            "tool_a ->   ".to_string(),
        ],
        3,
    );

    assert_eq!(
        preferred,
        vec![
            zorai_protocol::tool_names::SEARCH_FILES.to_string(),
            zorai_protocol::tool_names::READ_FILE.to_string()
        ]
    );
}

#[tokio::test]
async fn implicit_feedback_persistence_records_signal_rows_and_score_history() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-persisted-satisfaction", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    let signals = engine
        .history
        .list_implicit_signals("global", 10)
        .await
        .expect("load implicit signals");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal_type, "tool_fallback");
    assert!((signals[0].weight + 0.12).abs() < f64::EPSILON);
    assert!(signals[0]
        .context_snapshot_json
        .as_deref()
        .is_some_and(|json| json.contains(zorai_protocol::tool_names::SEARCH_FILES)));

    let scores = engine
        .history
        .list_satisfaction_scores("global", 10)
        .await
        .expect("load satisfaction scores");
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0].label, "healthy");
    assert_eq!(scores[0].signal_count, 1);
    assert!((scores[0].score - 0.68).abs() < 1e-9);
}

#[tokio::test]
async fn operator_correction_persists_thread_scoped_signal_and_score_snapshot() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message(
            "thread-correction-persist",
            "Actually, use ripgrep instead.",
            true,
        )
        .await
        .expect("record correction message");

    let signals = engine
        .history
        .list_implicit_signals("thread-correction-persist", 10)
        .await
        .expect("load correction signals");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal_type, "operator_correction");
    assert!((signals[0].weight + 0.16).abs() < f64::EPSILON);

    let scores = engine
        .history
        .list_satisfaction_scores("thread-correction-persist", 10)
        .await
        .expect("load correction satisfaction scores");
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0].label, "fragile");
    assert_eq!(scores[0].signal_count, 2);
    assert!((scores[0].score - 0.54).abs() < 1e-9);
}

#[tokio::test]
async fn rapid_revert_persists_thread_scoped_signal_when_agent_file_edit_is_quickly_reverted() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.set_aline_startup_test_availability(false);

    let git_dir = root.path();
    let git_init = std::process::Command::new("git")
        .args(["init"])
        .current_dir(git_dir)
        .output()
        .expect("git init should spawn");
    assert!(git_init.status.success(), "git init should succeed");

    let git_user_name = std::process::Command::new("git")
        .args(["config", "user.name", "zorai tests"])
        .current_dir(git_dir)
        .output()
        .expect("git config user.name should spawn");
    assert!(
        git_user_name.status.success(),
        "git config user.name should succeed"
    );

    let git_user_email = std::process::Command::new("git")
        .args(["config", "user.email", "zorai@example.com"])
        .current_dir(git_dir)
        .output()
        .expect("git config user.email should spawn");
    assert!(
        git_user_email.status.success(),
        "git config user.email should succeed"
    );

    let src_dir = root.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let file_path = src_dir.join("lib.rs");
    let baseline = "pub fn answer() -> u32 {\n    41\n}\n";
    std::fs::write(&file_path, baseline).expect("write baseline file");

    let git_add = std::process::Command::new("git")
        .args(["add", "src/lib.rs"])
        .current_dir(git_dir)
        .output()
        .expect("git add should spawn");
    assert!(git_add.status.success(), "git add should succeed");

    let git_commit = std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(git_dir)
        .output()
        .expect("git commit should spawn");
    assert!(git_commit.status.success(), "git commit should succeed");

    let agent_version = "pub fn answer() -> u32 {\n    42\n}\n";
    std::fs::write(&file_path, agent_version).expect("write agent version");
    engine
        .record_file_work_context(
            "thread-rapid-revert",
            None,
            zorai_protocol::tool_names::WRITE_FILE,
            file_path.to_str().expect("utf-8 file path"),
        )
        .await;

    std::fs::write(&file_path, baseline).expect("revert file back to baseline");
    engine
        .refresh_thread_repo_context("thread-rapid-revert")
        .await;

    let signals = engine
        .history
        .list_implicit_signals("thread-rapid-revert", 10)
        .await
        .expect("load rapid revert signals");
    assert_eq!(
        signals.len(),
        1,
        "rapid revert should persist exactly one implicit feedback signal"
    );
    assert_eq!(signals[0].signal_type, "rapid_revert");
    assert!(
        signals[0].weight < -0.16,
        "rapid revert should be a stronger negative signal than an operator correction"
    );
    assert!(signals[0]
        .context_snapshot_json
        .as_deref()
        .is_some_and(|json| json.contains("src/lib.rs")
            && json.contains(zorai_protocol::tool_names::WRITE_FILE)));
}

#[tokio::test]
async fn rapid_revert_detection_skips_existing_path_signal_beyond_recent_unrelated_window() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let git_dir = root.path().join("repo-rapid-revert-existing");
    std::fs::create_dir_all(git_dir.join("src")).expect("create git repo");
    let file_path = git_dir.join("src/lib.rs");
    let baseline = "pub fn answer() -> u32 {\n    7\n}\n";
    std::fs::write(&file_path, baseline).expect("write baseline");

    let git_init = std::process::Command::new("git")
        .arg("init")
        .current_dir(&git_dir)
        .output()
        .expect("git init should spawn");
    assert!(git_init.status.success(), "git init should succeed");
    for (key, value) in [
        ("user.email", "test@example.com"),
        ("user.name", "Test User"),
    ] {
        let output = std::process::Command::new("git")
            .args(["config", key, value])
            .current_dir(&git_dir)
            .output()
            .expect("git config should spawn");
        assert!(output.status.success(), "git config {key} should succeed");
    }
    let git_add = std::process::Command::new("git")
        .args(["add", "src/lib.rs"])
        .current_dir(&git_dir)
        .output()
        .expect("git add should spawn");
    assert!(git_add.status.success(), "git add should succeed");
    let git_commit = std::process::Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&git_dir)
        .output()
        .expect("git commit should spawn");
    assert!(git_commit.status.success(), "git commit should succeed");

    let agent_version = "pub fn answer() -> u32 {\n    42\n}\n";
    std::fs::write(&file_path, agent_version).expect("write agent version");
    engine
        .record_file_work_context(
            "thread-rapid-revert-existing",
            None,
            zorai_protocol::tool_names::WRITE_FILE,
            file_path.to_str().expect("utf-8 file path"),
        )
        .await;

    let now = now_millis();
    engine
        .history
        .insert_implicit_signal(&crate::history::ImplicitSignalRow {
            id: "old-rapid-revert-src-lib".to_string(),
            session_id: "thread-rapid-revert-existing".to_string(),
            signal_type: "rapid_revert".to_string(),
            weight: -0.18,
            timestamp_ms: now.saturating_sub(60_000),
            context_snapshot_json: Some(
                serde_json::json!({
                    "path": "src/lib.rs",
                    "source": zorai_protocol::tool_names::WRITE_FILE,
                })
                .to_string(),
            ),
        })
        .await
        .expect("insert existing rapid revert signal");
    for index in 0..50u64 {
        engine
            .history
            .insert_implicit_signal(&crate::history::ImplicitSignalRow {
                id: format!("newer-unrelated-rapid-window-{index}"),
                session_id: "thread-rapid-revert-existing".to_string(),
                signal_type: "tool_fallback".to_string(),
                weight: -0.01,
                timestamp_ms: now.saturating_sub(10_000).saturating_add(index),
                context_snapshot_json: None,
            })
            .await
            .expect("insert unrelated signal");
    }

    std::fs::write(&file_path, baseline).expect("revert file back to baseline");
    engine
        .refresh_thread_repo_context("thread-rapid-revert-existing")
        .await;

    let signals = engine
        .history
        .list_implicit_signals("thread-rapid-revert-existing", 100)
        .await
        .expect("load implicit signals");
    let rapid_revert_count = signals
        .iter()
        .filter(|signal| signal.signal_type == "rapid_revert")
        .count();
    assert_eq!(
        rapid_revert_count, 1,
        "rapid-revert detection should use SQL type filtering before limiting existing signals"
    );
}

#[tokio::test]
async fn work_context_goal_context_resolves_persisted_task_by_id() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let task = engine
        .enqueue_task(
            "Persisted work-context task".to_string(),
            "Resolve task metadata without scanning the live queue.".to_string(),
            "normal",
            None,
            Some("session-work-context-sql".to_string()),
            Vec::new(),
            None,
            "goal_run",
            Some("goal-work-context-sql".to_string()),
            None,
            Some("thread-work-context-sql".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();

    let (goal_run_id, step_index, session_id) = engine.goal_context_for_task(Some(&task.id)).await;

    assert_eq!(goal_run_id.as_deref(), Some("goal-work-context-sql"));
    assert_eq!(step_index, None);
    assert_eq!(session_id.as_deref(), Some("session-work-context-sql"));
}

#[tokio::test]
async fn work_context_goal_todo_context_resolves_persisted_task_and_goal_by_id() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run = engine
        .start_goal_run(
            "Persisted todo context".to_string(),
            Some("Persisted todo context".to_string()),
            Some("thread-persisted-todo-context".to_string()),
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    let task = engine
        .enqueue_task(
            "Persisted todo task".to_string(),
            "Resolve todo context from persisted task and goal rows.".to_string(),
            "normal",
            None,
            Some("session-persisted-todo-context".to_string()),
            Vec::new(),
            None,
            "goal_run",
            Some(goal_run.id.clone()),
            None,
            goal_run.thread_id.clone(),
            Some("daemon".to_string()),
        )
        .await;
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|item| item.id == task.id) {
            *existing = task.clone();
        }
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    engine.goal_runs.lock().await.clear();

    let context = engine
        .goal_todo_context_for_task(&task.id)
        .await
        .expect("goal todo context should resolve persisted rows");

    assert_eq!(context.goal_run_id, goal_run.id);
    assert_eq!(context.current_step_index, goal_run.current_step_index);
    assert!(context.authoritative);
}

#[tokio::test]
async fn resolve_thread_repo_root_uses_persisted_task_session_for_thread() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let repo_root = root.path().join("repo-root-from-persisted-task");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let git_init = std::process::Command::new("git")
        .arg("init")
        .current_dir(&repo_root)
        .output()
        .expect("git init should spawn");
    assert!(git_init.status.success(), "git init should succeed");
    let Ok((session_id, _rx)) = manager
        .spawn(
            Some(std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())),
            Some(repo_root.to_string_lossy().to_string()),
            None,
            None,
            80,
            24,
        )
        .await
    else {
        return;
    };
    let mut task = engine
        .enqueue_task(
            "Persisted repo session task".to_string(),
            "Resolve repo root from persisted task session binding.".to_string(),
            "normal",
            None,
            Some(session_id.to_string()),
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-persisted-session-repo".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    task.thread_id = Some("thread-persisted-session-repo".to_string());
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|item| item.id == task.id) {
            *existing = task.clone();
        }
    }
    engine.persist_tasks().await;
    engine.tasks.lock().await.clear();
    assert_eq!(
        engine
            .history
            .latest_agent_task_session_for_thread("thread-persisted-session-repo")
            .await
            .expect("task session query should succeed")
            .as_deref(),
        Some(session_id.to_string().as_str())
    );
    assert!(
        manager
            .list()
            .await
            .into_iter()
            .any(|session| session.id == session_id
                && session.cwd.as_deref() == Some(repo_root.to_string_lossy().as_ref())),
        "spawned session should expose repo cwd"
    );

    let resolved = engine
        .resolve_thread_repo_root("thread-persisted-session-repo")
        .await
        .expect("repo root should resolve from persisted task session");

    let expected_session_id = session_id.to_string();
    assert_eq!(resolved.0, repo_root.to_string_lossy().to_string());
    assert_eq!(resolved.1, None);
    assert_eq!(resolved.2.as_deref(), Some(expected_session_id.as_str()));
    assert_eq!(
        task.thread_id.as_deref(),
        Some("thread-persisted-session-repo")
    );
}

#[tokio::test]
async fn very_short_attention_dwell_persists_short_dwell_signal_with_duration() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    config.operator_model.allow_attention_tracking = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_attention_surface("conversation:chat")
        .await
        .expect("record first attention surface");
    tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
    engine
        .record_attention_surface("modal:command_palette")
        .await
        .expect("record second attention surface");

    let signals = engine
        .history
        .list_implicit_signals("global", 10)
        .await
        .expect("load implicit signals");
    let short_dwell = signals
        .iter()
        .find(|signal| signal.signal_type == "short_dwell")
        .expect("short dwell signal should be persisted");
    assert!(short_dwell.weight < 0.0);
    assert!(short_dwell
        .context_snapshot_json
        .as_deref()
        .is_some_and(|json| json.contains("dwell_secs") && json.contains("conversation:chat")));

    let scores = engine
        .history
        .list_satisfaction_scores("global", 10)
        .await
        .expect("load short dwell satisfaction scores");
    assert_eq!(scores.len(), 1);
    assert_eq!(
        scores[0].signal_count, 1,
        "short dwell should contribute to the satisfaction snapshot signal count"
    );
    assert!(scores[0].score < 0.8);
}

#[tokio::test]
async fn record_attention_surface_bootstraps_missing_agent_data_dir() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let history = HistoryStore::new_test_store(root.path())
        .await
        .expect("history store");
    let data_dir = root.path().join("agent");
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_attention_tracking = true;
    let engine = AgentEngine::new_with_storage_and_http_client(
        manager,
        config,
        history,
        data_dir.clone(),
        reqwest::Client::new(),
    );

    engine
        .record_attention_surface("conversation:chat")
        .await
        .expect("record attention surface on first run");

    assert!(
        data_dir.join("operator_model.json").exists(),
        "recording attention should bootstrap operator model persistence on a fresh install"
    );
}

#[tokio::test]
async fn deleting_thread_right_after_assistant_reply_persists_session_abandon_signal() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut assistant = AgentMessage::user("Here is the answer.", now);
    assistant.role = MessageRole::Assistant;

    engine.threads.write().await.insert(
        "thread-session-abandon".to_string(),
        AgentThread {
            id: "thread-session-abandon".to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Abandonable thread".to_string(),
            messages: vec![
                AgentMessage::user("Help me.", now.saturating_sub(1_000)),
                assistant,
            ],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: now.saturating_sub(1_000),
            updated_at: now,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    assert!(engine.delete_thread("thread-session-abandon").await);

    let signals = engine
        .history
        .list_implicit_signals("thread-session-abandon", 10)
        .await
        .expect("load session abandonment signals");
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].signal_type, "session_abandon");
    assert!(signals[0].weight < 0.0);
    assert!(signals[0]
        .context_snapshot_json
        .as_deref()
        .is_some_and(
            |json| json.contains("thread-session-abandon") && json.contains("Here is the answer.")
        ));

    let scores = engine
        .history
        .list_satisfaction_scores("thread-session-abandon", 10)
        .await
        .expect("load abandonment satisfaction scores");
    assert_eq!(scores.len(), 1);
    assert!(matches!(
        scores[0].label.as_str(),
        "fragile" | "strained" | "healthy"
    ));
    assert_eq!(scores[0].signal_count, 1);
}

#[test]
fn persisted_satisfaction_decay_uses_recent_signal_history() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 1;

    let now = 1_717_400_000_000u64;
    let applied = apply_persisted_satisfaction_decay(
        &mut model,
        &[
            PersistedSatisfactionSignalSample {
                weight: -0.12,
                timestamp_ms: now - 1_000,
            },
            PersistedSatisfactionSignalSample {
                weight: -0.16,
                timestamp_ms: now - 2_000,
            },
            PersistedSatisfactionSignalSample {
                weight: -0.18,
                timestamp_ms: now - 3_000,
            },
        ],
        now,
    );

    assert!(applied);
    assert_eq!(model.operator_satisfaction.label, "strained");
    assert!((model.operator_satisfaction.score - 0.34).abs() < 0.02);
}

#[test]
fn cognitive_resonance_snapshot_maps_strained_feedback_to_frustrated_state() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 1;
    model.operator_satisfaction.label = "strained".to_string();
    model.operator_satisfaction.score = 0.18;
    model.implicit_feedback.correction_message_count = 1;
    model.implicit_feedback.top_tool_fallbacks = vec![
        "read_file -> search_files".to_string(),
        "bash_command -> read_file".to_string(),
    ];

    let resonance = CognitiveResonanceSnapshot::from_model(&model);
    assert_eq!(resonance.state, CognitiveResonanceState::Frustrated);
    assert!((resonance.score - 0.18).abs() < f64::EPSILON);
    assert!(resonance.compact_response);
    assert!(resonance.prompt_for_clarification);
    assert!(resonance.adjustments.verbosity <= 0.2);
    assert!(resonance.adjustments.proactiveness <= 0.15);
    assert!(resonance.adjustments.memory_urgency >= 0.8);
    assert_eq!(
        resonance.preferred_tool_fallbacks,
        vec![
            zorai_protocol::tool_names::SEARCH_FILES.to_string(),
            zorai_protocol::tool_names::READ_FILE.to_string()
        ]
    );
}

#[test]
fn cognitive_resonance_snapshot_maps_strong_feedback_to_flow_state() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 1;
    model.operator_satisfaction.label = "strong".to_string();
    model.operator_satisfaction.score = 0.8;
    model.risk_fingerprint.risk_tolerance = RiskTolerance::Aggressive;

    let resonance = CognitiveResonanceSnapshot::from_model(&model);
    assert_eq!(resonance.state, CognitiveResonanceState::Flow);
    assert!((resonance.score - 0.8).abs() < f64::EPSILON);
    assert!(!resonance.compact_response);
    assert!(!resonance.prompt_for_clarification);
    assert!(resonance.adjustments.verbosity >= 0.9);
    assert!(resonance.adjustments.risk_tolerance >= 0.85);
    assert!(resonance.adjustments.proactiveness >= 0.8);
    assert!(resonance.adjustments.memory_urgency <= 0.3);
}

#[tokio::test]
async fn operator_profile_summary_json_exposes_behavior_adaptation_from_satisfaction_signals() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-summary-adaptation", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    {
        let mut model = engine.operator_model.write().await;
        model.implicit_feedback.revision_message_count = 1;
        model.implicit_feedback.correction_message_count = 1;
        model.implicit_feedback.fast_denial_count = 1;
        model.attention_topology.rapid_switch_count = 2;
        refresh_operator_satisfaction(&mut model);
    }

    let summary_json = engine
        .get_operator_profile_summary_json()
        .await
        .expect("operator profile summary json");
    let payload: serde_json::Value =
        serde_json::from_str(&summary_json).expect("valid operator profile summary json");

    assert_eq!(
        payload["behavior_adaptation"]["mode"].as_str(),
        Some("minimal")
    );
    assert_eq!(
        payload["behavior_adaptation"]["compact_response"].as_bool(),
        Some(true)
    );
    assert_eq!(
        payload["behavior_adaptation"]["prompt_for_clarification"].as_bool(),
        Some(true)
    );
    assert!(payload["behavior_adaptation"]["preferred_tool_fallbacks"]
        .as_array()
        .is_some_and(|items| items
            .iter()
            .any(|item| item.as_str() == Some(zorai_protocol::tool_names::SEARCH_FILES))));
    assert_eq!(
        payload["cognitive_resonance"]["state"].as_str(),
        Some("frustrated")
    );
    assert_eq!(
        payload["cognitive_resonance"]["compact_response"].as_bool(),
        Some(true)
    );
    assert_eq!(
        payload["cognitive_resonance"]["prompt_for_clarification"].as_bool(),
        Some(true)
    );
    assert!(
        payload["cognitive_resonance"]["adjustments"]["memory_urgency"]
            .as_f64()
            .is_some_and(|value| value >= 0.8)
    );
}

#[tokio::test]
async fn operator_profile_summary_json_exposes_implicit_feedback_learning_history() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_message_statistics = true;
    config.operator_model.allow_implicit_feedback = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-summary-learning", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    let summary_json = engine
        .get_operator_profile_summary_json()
        .await
        .expect("operator profile summary json");
    let payload: serde_json::Value =
        serde_json::from_str(&summary_json).expect("valid operator profile summary json");

    let learning = &payload["implicit_feedback_learning"];
    assert!(learning["recent_implicit_signals"]
        .as_array()
        .is_some_and(|items| items
            .iter()
            .any(|item| { item["signal_type"].as_str() == Some("tool_fallback") })));
    assert!(learning["recent_satisfaction_scores"]
        .as_array()
        .is_some_and(|items| items
            .iter()
            .any(|item| { item["label"].as_str() == Some("healthy") })));
}

#[test]
fn persisted_satisfaction_decay_requires_enough_history() {
    let mut model = OperatorModel::default();
    model.cognitive_style.message_count = 1;
    model.operator_satisfaction.score = 0.8;
    model.operator_satisfaction.label = "strong".to_string();

    let applied = apply_persisted_satisfaction_decay(
        &mut model,
        &[
            PersistedSatisfactionSignalSample {
                weight: -0.12,
                timestamp_ms: 10,
            },
            PersistedSatisfactionSignalSample {
                weight: -0.10,
                timestamp_ms: 20,
            },
        ],
        1_000,
    );

    assert!(!applied);
    assert_eq!(model.operator_satisfaction.label, "strong");
    assert!((model.operator_satisfaction.score - 0.8).abs() < f64::EPSILON);
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_persisted_implicit_feedback_history() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.operator_model.enabled = true;
    config.operator_model.allow_implicit_feedback = true;
    config.operator_model.allow_message_statistics = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_message("thread-diagnostics-persisted", "Please run tests.", true)
        .await
        .expect("record operator message");
    engine
        .record_tool_hesitation(
            zorai_protocol::tool_names::READ_FILE,
            zorai_protocol::tool_names::SEARCH_FILES,
            true,
            false,
        )
        .await
        .expect("record tool hesitation");

    let snapshot = engine.status_diagnostics_snapshot().await;
    let satisfaction = &snapshot["operator_satisfaction"];
    let signals = satisfaction["recent_implicit_signals"]
        .as_array()
        .expect("recent implicit signals array");
    let scores = satisfaction["recent_satisfaction_scores"]
        .as_array()
        .expect("recent satisfaction scores array");

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0]["signal_type"], "tool_fallback");
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0]["label"], "healthy");
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_system_outcome_foresight_details() {
    let root = tempdir().expect("tempdir");
    let repo_root = root.path().join("repo-build-risk-diagnostics");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&repo_root)
        .output()
        .expect("git init");
    std::fs::write(
        repo_root.join("Cargo.toml"),
        "[package]\nname='demo'\nversion='0.1.0'\n",
    )
    .expect("write cargo manifest");
    std::fs::create_dir_all(repo_root.join("src")).expect("create src dir");
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn broken() {}\n").expect("write lib");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    // The `predict_system_outcome` path now gates on
    // `repo_monitor_enabled_for_repo`, which requires a workspace settings
    // row covering this repo path with a non-empty `include_dirs`. Without
    // it the foresight short-circuits to None before ever consulting the
    // health log, so the snapshot's `system_outcome_foresight` would
    // remain null and the test could not exercise the build/test risk
    // branch at all.
    let workspace_settings = zorai_protocol::WorkspaceSettings {
        workspace_id: "workspace-build-risk-diagnostics".to_string(),
        workspace_root: Some(repo_root.to_string_lossy().to_string()),
        operator: zorai_protocol::WorkspaceOperator::Svarog,
        repo_monitor_enabled: true,
        repo_monitor_include_dirs: vec!["src".to_string()],
        repo_monitor_exclude_dirs: Vec::new(),
        created_at: now_millis(),
        updated_at: now_millis(),
    };
    engine
        .history
        .upsert_workspace_settings(&workspace_settings)
        .await
        .expect("register workspace settings for repo monitor gate");

    engine
        .record_operator_attention(
            "conversation:chat",
            Some("thread-build-risk-diagnostics"),
            None,
        )
        .await
        .expect("record operator attention");
    engine.thread_work_contexts.write().await.insert(
        "thread-build-risk-diagnostics".to_string(),
        ThreadWorkContext {
            thread_id: "thread-build-risk-diagnostics".to_string(),
            entries: vec![WorkContextEntry {
                path: "src/lib.rs".to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::RepoChange,
                source: "repo_scan".to_string(),
                change_kind: Some("modified".to_string()),
                repo_root: Some(repo_root.to_string_lossy().to_string()),
                goal_run_id: None,
                step_index: None,
                session_id: None,
                operation_id: None,
                task_id: None,
                before_hash: None,
                after_hash: None,
                is_text: true,
                updated_at: now_millis(),
            }],
        },
    );
    engine
        .history
        .insert_health_log(
            "health-build-risk-diagnostics",
            "task",
            "cargo-test",
            "degraded",
            Some("{\"tool\":\"cargo test\",\"error\":\"Command failed\"}"),
            Some("recent cargo test failed in this repo"),
            now_millis(),
        )
        .await
        .expect("save health log");

    engine.run_anticipatory_tick().await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let foresight = &snapshot["system_outcome_foresight"];
    assert_eq!(
        foresight["thread_id"].as_str(),
        Some("thread-build-risk-diagnostics")
    );
    assert_eq!(
        foresight["prediction_type"].as_str(),
        Some("build_test_risk")
    );
    assert_eq!(
        foresight["predicted_outcome"].as_str(),
        Some("build/test failure")
    );
    assert!(foresight["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.7));
    assert!(foresight["summary"]
        .as_str()
        .is_some_and(|text| text.contains("build/test failure risk")));
    assert!(foresight["bullets"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("prediction_type=build_test_risk")))));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_proactive_suppression_transparency() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut task = engine
        .enqueue_task(
            "Need approval".to_string(),
            "Need approval".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            Some("thread-latency-diagnostics".to_string()),
            Some("daemon".to_string()),
        )
        .await;
    task.status = TaskStatus::AwaitingApproval;
    {
        let mut tasks = engine.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|existing| existing.id == task.id) {
            *existing = task.clone();
        }
    }
    engine
        .record_operator_attention(
            "conversation:chat",
            Some("thread-latency-diagnostics"),
            None,
        )
        .await
        .expect("record operator attention");
    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.risk_fingerprint.approval_requests = 4;
        model.risk_fingerprint.approvals = 2;
        model.risk_fingerprint.denials = 2;
        model.risk_fingerprint.avg_response_time_secs = 45.0;
        refresh_risk_metrics(&mut model.risk_fingerprint);
        refresh_operator_satisfaction(&mut model);
    }

    engine.run_anticipatory_tick().await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let suppression = &snapshot["proactive_suppression"];
    assert_eq!(
        suppression["thread_id"].as_str(),
        Some("thread-latency-diagnostics")
    );
    assert!(suppression["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.7));
    assert!(suppression["summary"]
        .as_str()
        .is_some_and(|text| text.contains("suppressed") || text.contains("tightened")));
    assert!(suppression["bullets"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item
            .as_str()
            .is_some_and(|text| text.contains("suppressed_kinds=")))));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_speculative_queue_summary() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let now = now_millis();
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.opportunity_queue.push_back(SpeculativeOpportunity {
            id: "spec-queue-1".to_string(),
            thread_id: Some("thread-spec-queue-diag".to_string()),
            source_kind: "intent_prediction".to_string(),
            action_kind: "repo_context_refresh".to_string(),
            confidence: 0.91,
            created_at_ms: now,
            expires_at_ms: now + 30_000,
            status: SpeculativeOpportunityStatus::Queued,
            summary: "Prefetch repo context for the active thread".to_string(),
        });
    }

    let snapshot = engine.status_diagnostics_snapshot().await;
    let speculative = &snapshot["speculative_execution"];
    assert_eq!(speculative["queue_depth"].as_u64(), Some(1));

    let queued = speculative["queued_opportunities"]
        .as_array()
        .expect("queued speculative opportunities array");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0]["id"].as_str(), Some("spec-queue-1"));
    assert_eq!(
        queued[0]["thread_id"].as_str(),
        Some("thread-spec-queue-diag")
    );
    assert_eq!(queued[0]["source_kind"].as_str(), Some("intent_prediction"));
    assert_eq!(
        queued[0]["action_kind"].as_str(),
        Some("repo_context_refresh")
    );
    assert_eq!(queued[0]["status"].as_str(), Some("queued"));
    assert!(queued[0]["confidence"]
        .as_f64()
        .is_some_and(|value| value >= 0.9));
    assert!(queued[0]["expires_in_ms"]
        .as_u64()
        .is_some_and(|value| value > 0));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_cached_speculative_results_with_usage_and_ttl() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let now = now_millis();
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.speculative_results_by_thread.insert(
            "thread-spec-result-diag".to_string(),
            vec![
                SpeculativeResult {
                    opportunity_id: "spec-result-used".to_string(),
                    action_kind: "repo_context_refresh".to_string(),
                    thread_id: Some("thread-spec-result-diag".to_string()),
                    summary: "Cached repo summary for active thread".to_string(),
                    artifact: serde_json::json!({"summary": "branch main; dirty=true"}),
                    completed_at_ms: now - 5_000,
                    expires_at_ms: now + 60_000,
                    used_at_ms: Some(now - 1_000),
                    precomputation_id: Some(42),
                },
                SpeculativeResult {
                    opportunity_id: "spec-result-expired".to_string(),
                    action_kind: "repo_context_refresh".to_string(),
                    thread_id: Some("thread-spec-result-diag".to_string()),
                    summary: "Expired cached repo summary".to_string(),
                    artifact: serde_json::json!({"summary": "expired"}),
                    completed_at_ms: now - 10_000,
                    expires_at_ms: now - 1,
                    used_at_ms: None,
                    precomputation_id: None,
                },
            ],
        );
    }

    let snapshot = engine.status_diagnostics_snapshot().await;
    let speculative = &snapshot["speculative_execution"];
    assert_eq!(speculative["cached_result_count"].as_u64(), Some(2));

    let results = speculative["cached_results"]
        .as_array()
        .expect("cached speculative results array");
    assert_eq!(results.len(), 2);

    let used = results
        .iter()
        .find(|item| item["opportunity_id"].as_str() == Some("spec-result-used"))
        .expect("used speculative result should be present");
    assert_eq!(used["thread_id"].as_str(), Some("thread-spec-result-diag"));
    assert_eq!(used["action_kind"].as_str(), Some("repo_context_refresh"));
    assert_eq!(used["precomputation_id"].as_i64(), Some(42));
    assert_eq!(used["used"].as_bool(), Some(true));
    assert_eq!(used["is_expired"].as_bool(), Some(false));
    assert!(used["expires_in_ms"]
        .as_u64()
        .is_some_and(|value| value > 0));
    assert_eq!(
        used["artifact"]["summary"].as_str(),
        Some("branch main; dirty=true")
    );

    let expired = results
        .iter()
        .find(|item| item["opportunity_id"].as_str() == Some("spec-result-expired"))
        .expect("expired speculative result should be present");
    assert_eq!(expired["used"].as_bool(), Some(false));
    assert_eq!(expired["is_expired"].as_bool(), Some(true));
    assert_eq!(expired["expires_in_ms"].as_u64(), Some(0));
    assert_eq!(expired["precomputation_id"].as_i64(), None);
}

#[tokio::test]
async fn emergent_protocols_summary_hidden_when_no_protocol_state_exists() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let thread_id = "thread-emergent-empty-diag";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "empty emergent protocol diagnostics".to_string(),
            messages: Vec::new(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 0,
            updated_at: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );

    let snapshot = engine.status_diagnostics_snapshot().await;
    assert!(snapshot["emergent_protocols"].is_null());
}

#[tokio::test]
async fn emergent_protocols_summary_visible_for_pending_proposals_and_registry_entries() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let proposed_thread_id = "thread-emergent-diag-proposed";
    let accepted_thread_id = "thread-emergent-diag-accepted";

    for thread_id in [proposed_thread_id, accepted_thread_id] {
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: format!("emergent protocol diagnostics {thread_id}"),
                messages: Vec::new(),
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: 0,
                updated_at: 0,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
    }

    let build_messages = |thread_id: &str| {
        vec![
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m1".to_string(),
                thread_id: thread_id.to_string(),
                created_at: 1,
                role: "user".to_string(),
                content: "continue".to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m2".to_string(),
                thread_id: thread_id.to_string(),
                created_at: 2,
                role: "assistant".to_string(),
                content: "working".to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m3".to_string(),
                thread_id: thread_id.to_string(),
                created_at: 3,
                role: "user".to_string(),
                content: "continue".to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m4".to_string(),
                thread_id: thread_id.to_string(),
                created_at: 4,
                role: "assistant".to_string(),
                content: "still working".to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
            AgentDbMessage {
                reasoning_tokens: None,
                thinking_ms: None,
                id: "m5".to_string(),
                thread_id: thread_id.to_string(),
                created_at: 5,
                role: "user".to_string(),
                content: "continue".to_string(),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            },
        ]
    };

    let proposed = engine
        .analyze_emergent_protocol_from_messages(
            proposed_thread_id,
            &build_messages(proposed_thread_id),
        )
        .await
        .expect("analysis should succeed")
        .expect("candidate store should be returned");
    assert!(proposed.candidates.iter().any(|candidate| {
        candidate.state == crate::agent::emergent_protocol::types::ProtocolCandidateState::Proposed
            && candidate.normalized_pattern == "continue"
    }));

    let accepted = engine
        .analyze_emergent_protocol_from_messages(
            accepted_thread_id,
            &build_messages(accepted_thread_id),
        )
        .await
        .expect("analysis should succeed")
        .expect("candidate store should be returned");
    assert!(accepted.candidates.iter().any(|candidate| {
        candidate.state == crate::agent::emergent_protocol::types::ProtocolCandidateState::Proposed
            && candidate.normalized_pattern == "continue"
    }));

    let store = engine
        .get_thread_protocol_candidate_store(accepted_thread_id)
        .await
        .expect("candidate store should load");
    let candidate_id = store
        .candidates
        .iter()
        .find(|candidate| {
            candidate.state
                == crate::agent::emergent_protocol::types::ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        })
        .map(|candidate| candidate.id.clone())
        .expect("proposed candidate should exist");
    let response = engine
        .respond_to_protocol_proposal(accepted_thread_id, &candidate_id, true)
        .await
        .expect("proposal acceptance should succeed");
    assert_eq!(response["status"].as_str(), Some("accepted"));
    engine.threads.write().await.clear();

    let snapshot = engine.status_diagnostics_snapshot().await;
    let protocols = &snapshot["emergent_protocols"];
    assert_eq!(protocols["proposal_count"].as_u64(), Some(1));
    assert_eq!(protocols["protocol_count"].as_u64(), Some(1));
    assert!(protocols["proposals"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| {
            item["thread_id"].as_str() == Some(proposed_thread_id)
                && item["normalized_pattern"].as_str() == Some("continue")
        })));
    assert!(protocols["protocols"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| {
            item["thread_id"].as_str() == Some(accepted_thread_id)
                && item["normalized_pattern"].as_str() == Some("continue")
                && item["token"]
                    .as_str()
                    .is_some_and(|token| token.starts_with("@proto_"))
        })));
}

#[tokio::test]
async fn adaptive_carryover_summary_hidden_when_no_events_exist() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    assert!(snapshot["adaptive_carryover"].is_null());
}

#[tokio::test]
async fn adaptive_carryover_summary_visible_when_dream_hint_provenance_exists() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_dream_hints_persisted(
            crate::agent::agent_identity::WELES_AGENT_ID,
            2,
            &[
                "Prefer structured file/context tools before raw shell inspection.".to_string(),
                "Inspect the full task context before executing the next step.".to_string(),
            ],
            Some(42),
        )
        .await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let carryover = &snapshot["adaptive_carryover"];
    assert_eq!(
        carryover["inspection_tool"].as_str(),
        Some(zorai_protocol::tool_names::SHOW_DREAMS)
    );
    assert_eq!(carryover["persisted_event_count"].as_u64(), Some(1));
    assert_eq!(carryover["dream_hint_event_count"].as_u64(), Some(1));
    assert_eq!(carryover["forge_hint_event_count"].as_u64(), Some(0));
    let recent = carryover["recent_events"]
        .as_array()
        .expect("recent adaptive carryover provenance events array");
    assert_eq!(recent.len(), 1);
    assert_eq!(
        recent[0]["event_type"].as_str(),
        Some(crate::agent::provenance::PROVENANCE_EVENT_DREAM_HINTS_PERSISTED)
    );
    assert_eq!(recent[0]["source_kind"].as_str(), Some("dream_state"));
    assert!(recent[0]["sequence"].as_u64().is_some());
    assert!(recent[0]["provenance_ref"]
        .as_str()
        .is_some_and(|value| value.starts_with("provenance:")));
    assert_eq!(recent[0]["thread_id"].as_str(), None);
    assert_eq!(recent[0]["task_id"].as_str(), None);
    assert_eq!(recent[0]["goal_run_id"].as_str(), None);
    assert_eq!(recent[0]["causal_trace_id"].as_str(), None);
}

#[tokio::test]
async fn proactive_audit_summary_visible_in_diagnostics() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_provenance_event(
            crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED,
            "Prepared proactive cache for thread thread-audit-diag",
            serde_json::json!({
                "thread_id": "thread-audit-diag",
                "summary": "branch main; dirty=true",
            }),
            None,
            None,
            Some("thread-audit-diag"),
            None,
            None,
        )
        .await;
    engine
        .record_provenance_event(
            crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_USED,
            "Used speculative repo_context_refresh for thread thread-audit-diag",
            serde_json::json!({
                "thread_id": "thread-audit-diag",
                "action_kind": "repo_context_refresh",
                "source": "anticipatory_prompt_context",
            }),
            None,
            None,
            Some("thread-audit-diag"),
            None,
            None,
        )
        .await;

    let snapshot = engine.status_diagnostics_snapshot().await;
    let provenance = &snapshot["proactive_provenance"];
    assert_eq!(provenance["prepared_count"].as_u64(), Some(1));
    assert_eq!(provenance["used_count"].as_u64(), Some(1));
    assert_eq!(provenance["prepared_cache_count"].as_u64(), Some(1));
    assert_eq!(provenance["used_speculative_count"].as_u64(), Some(1));

    let recent = provenance["recent_events"]
        .as_array()
        .expect("recent proactive provenance events array");
    assert_eq!(recent.len(), 2);
    assert!(recent.iter().any(|entry| {
        entry["event_type"].as_str()
            == Some(crate::agent::provenance::PROVENANCE_EVENT_PROACTIVE_CACHE_PREPARED)
            && entry["thread_id"].as_str() == Some("thread-audit-diag")
    }));
    assert!(recent.iter().any(|entry| {
        entry["event_type"].as_str()
            == Some(crate::agent::provenance::PROVENANCE_EVENT_SPECULATIVE_RESULT_USED)
            && entry["thread_id"].as_str() == Some("thread-audit-diag")
    }));
}

#[tokio::test]
async fn status_diagnostics_snapshot_includes_meta_cognitive_self_model() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    {
        let mut model = engine.meta_cognitive_self_model.write().await;
        model.agent_id = "svarog".to_string();
        model.calibration_offset = -0.22;
        model.last_updated_ms = 1_717_299_999;
        if let Some(bias) = model
            .biases
            .iter_mut()
            .find(|bias| bias.name == "sunk_cost")
        {
            bias.occurrence_count = 4;
            bias.severity = 0.81;
        }
        if let Some(profile) = model
            .workflow_profiles
            .iter_mut()
            .find(|profile| profile.name == "debug_loop")
        {
            profile.avg_success_rate = 0.63;
            profile.avg_steps = 7;
        }
    }

    let snapshot = engine.status_diagnostics_snapshot().await;
    let self_model = &snapshot["meta_cognitive_self_model"];
    assert_eq!(self_model["agent_id"], "svarog");
    assert_eq!(self_model["last_updated_ms"].as_u64(), Some(1_717_299_999));
    assert!(self_model["calibration_offset"]
        .as_f64()
        .is_some_and(|value| (value + 0.22).abs() < f64::EPSILON));
    assert!(self_model["biases"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| {
            item["name"].as_str() == Some("sunk_cost")
                && item["occurrence_count"].as_u64() == Some(4)
        })));
    assert!(self_model["workflow_profiles"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| {
            item["name"].as_str() == Some("debug_loop") && item["avg_steps"].as_u64() == Some(7)
        })));
}
