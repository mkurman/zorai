use zorai_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

use super::{
    compute_policy_fingerprint, effective_constraints, evaluate_governance,
    governance_input_for_managed_command, GovernanceInitiator, GovernanceInput,
    ProvenanceCompleteness, ProvenanceStatus, RiskDimensions, TransitionKind, VerdictClass,
};

fn make_request(
    command: &str,
    allow_network: bool,
    sandbox_enabled: bool,
) -> ManagedCommandRequest {
    ManagedCommandRequest {
        command: command.to_string(),
        rationale: "test rationale".to_string(),
        allow_network,
        sandbox_enabled,
        security_level: SecurityLevel::Moderate,
        cwd: Some("/tmp".to_string()),
        language_hint: Some("bash".to_string()),
        source: ManagedCommandSource::Agent,
    }
}

#[test]
fn governance_input_for_managed_command_populates_run_stage_and_scope_identity() {
    let input = governance_input_for_managed_command(
        "exec_scope",
        &make_request("echo hello", false, true),
        Some("workspace-a".to_string()),
        Some("lane-a".to_string()),
    );

    assert_eq!(input.run_id.as_deref(), Some("exec_scope"));
    assert_eq!(input.stage_id.as_deref(), Some("managed_dispatch"));
    assert_eq!(input.lane_ids, vec!["lane-a".to_string()]);
    assert_eq!(input.target_ids, vec!["workspace-a".to_string()]);
    assert_eq!(input.initiator, GovernanceInitiator::Agent);
}

#[test]
fn governance_input_for_managed_command_falls_back_to_session_target_without_workspace() {
    let input = governance_input_for_managed_command(
        "exec_scope_2",
        &make_request("echo hello", false, true),
        None,
        Some("lane-a".to_string()),
    );

    assert_eq!(input.target_ids, vec!["lane-a".to_string()]);
}

#[test]
fn governance_input_for_managed_command_maps_human_source_to_operator_initiator() {
    let request = ManagedCommandRequest {
        source: ManagedCommandSource::Human,
        ..make_request("echo hello", false, true)
    };

    let input = governance_input_for_managed_command(
        "exec_scope_3",
        &request,
        Some("workspace-a".to_string()),
        Some("lane-a".to_string()),
    );

    assert_eq!(input.initiator, GovernanceInitiator::Operator);
}

#[test]
fn fingerprint_is_stable_across_target_ordering() {
    let request = make_request("echo hello", false, true);
    let mut left = governance_input_for_managed_command(
        "exec_left",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    left.lane_ids = vec!["lane-b".to_string(), "lane-a".to_string()];
    left.target_ids = vec!["target-b".to_string(), "target-a".to_string()];

    let mut right = left.clone();
    right.lane_ids.reverse();
    right.target_ids.reverse();

    assert_eq!(
        compute_policy_fingerprint(&left),
        compute_policy_fingerprint(&right)
    );
}

#[test]
fn insufficient_provenance_defers_side_effecting_transition() {
    let request = make_request("rm -rf /tmp/demo", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_risky",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.provenance_status = ProvenanceStatus {
        completeness: ProvenanceCompleteness::Insufficient,
        missing_evidence: vec!["missing command provenance".to_string()],
    };

    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::Defer);
}

#[test]
fn high_risk_requires_approval() {
    let request = make_request("sudo terraform destroy", true, false);
    let input = governance_input_for_managed_command(
        "exec_high",
        &request,
        Some("workspace-a".to_string()),
        None,
    );

    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert_eq!(verdict.verdict_class, super::VerdictClass::RequireApproval);
    assert!(constraints
        .iter()
        .all(|constraint| !matches!(constraint.kind, super::ConstraintKind::SandboxRequired)));
}

#[test]
fn high_concurrency_adds_serial_constraint() {
    let input = GovernanceInput {
        run_id: None,
        task_id: None,
        thread_id: None,
        goal_run_id: None,
        transition_kind: TransitionKind::ManagedCommandDispatch,
        stage_id: None,
        lane_ids: vec!["lane-a".to_string(), "lane-b".to_string()],
        target_ids: vec!["workspace-a".to_string()],
        requested_action_summary: "parallel fanout dispatch".to_string(),
        intent_summary: "exercise high concurrency rule".to_string(),
        risk_dimensions: RiskDimensions {
            destructiveness: 2,
            scope: 3,
            reversibility: 2,
            privilege: 1,
            externality: 1,
            concurrency: 8,
        },
        blast_radius: super::BlastRadiusEstimate {
            lane_scope: "two lanes".to_string(),
            stage_scope: "stage-a".to_string(),
            run_scope: "workspace".to_string(),
        },
        environment_facts: super::EnvironmentFacts {
            sandbox_available: true,
            sandbox_enabled: true,
            network_allowed: false,
            filesystem_scope: Some("/tmp".to_string()),
            workspace_id: Some("workspace-a".to_string()),
            host_type: None,
            privilege_posture: None,
        },
        approval_context: super::ApprovalContext::default(),
        retry_or_rebind_history: Vec::new(),
        provenance_status: ProvenanceStatus::complete(),
        rollback_or_compensation_hints: super::CompensationHints::unknown(),
        initiator: super::GovernanceInitiator::Agent,
    };

    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert!(constraints
        .iter()
        .any(|constraint| matches!(constraint.kind, super::ConstraintKind::SerialOnlyExecution)));
}

#[test]
fn risk_scoring_does_not_flag_destructive_substring_inside_a_quoted_string() {
    // Before the tokenizer fix, `echo "rm -rf"` substring-matched as destructive=9.
    // shlex sees `["echo", "rm -rf"]` — argv[0] is `echo`, which is non-destructive.
    let request = make_request(r#"echo "rm -rf""#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 1);
}

#[test]
fn risk_scoring_recognizes_quoted_program_name_with_destructive_flags() {
    // `"rm" -rf /tmp/foo` should still score as destructive — argv[0] is `rm`
    // after shlex strips the quotes around the program name.
    let request = make_request(r#""rm" -rf /tmp/foo"#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_falls_back_to_high_when_tokenization_fails() {
    // Unbalanced quote — shlex refuses to tokenize. We must NOT silently
    // classify as low-risk; that would be a bypass.
    let request = make_request(r#"echo "unclosed"#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_recognizes_chain_operator_outside_quotes() {
    let request = make_request("rm /tmp/a && rm /tmp/b", false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.concurrency, 5);
}

#[test]
fn risk_scoring_recognizes_unspaced_shell_separator() {
    let request = make_request("echo ok; rm -rf /tmp/b", false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.concurrency, 5);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_does_not_register_chain_operator_when_quoted() {
    // The `;` lives inside a quoted argument and is therefore a literal
    // character handed to `printf`, not a shell separator.
    let request = make_request(r#"printf "first; second""#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.concurrency, 1);
}

#[test]
fn risk_scoring_treats_sudo_prefix_as_privilege_elevation_and_unwraps_payload() {
    let request = make_request("sudo rm -rf /tmp/foo", false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.privilege, 8);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_treats_sudo_options_as_privilege_elevation_and_unwraps_payload() {
    let request = make_request("sudo -n rm -r -f /tmp/foo", false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.privilege, 8);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_walks_command_substitution_ast_nodes() {
    let request = make_request(r#"echo "$(rm -rf /tmp/foo)""#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_parses_shell_inline_payload() {
    let request = make_request(r#"bash -c "rm -rf /tmp/foo""#, false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_parses_shell_heredoc_payload() {
    let request = make_request("bash <<'EOF'\nrm -rf /tmp/foo\nEOF", false, true);
    let dims = RiskDimensions::from_managed_command(&request);
    assert_eq!(dims.destructiveness, 9);
}

#[test]
fn risk_scoring_distinguishes_safe_git_subcommand_from_hard_reset() {
    let safe = make_request("git status", false, true);
    let dangerous = make_request("git reset --hard HEAD~1", false, true);
    assert_eq!(
        RiskDimensions::from_managed_command(&safe).destructiveness,
        1
    );
    assert_eq!(
        RiskDimensions::from_managed_command(&dangerous).destructiveness,
        9
    );
}

#[test]
fn require_approval_verdict_attaches_filesystem_scope_narrowed_for_cwd_bearing_requests() {
    // High destructive score → RequireApproval; cwd present → attach scope.
    let request = make_request("rm -rf /tmp/work/foo", false, true);
    let input = governance_input_for_managed_command(
        "exec_scoped",
        &request,
        Some("workspace-a".to_string()),
        Some("lane-a".to_string()),
    );
    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::RequireApproval);
    let constraints = effective_constraints(&verdict);
    let scope_constraint = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::FilesystemScopeNarrowed))
        .expect("FilesystemScopeNarrowed constraint should be attached");
    let prefixes = scope_constraint
        .filesystem_scope_prefixes()
        .expect("constraint should round-trip its prefix list");
    assert_eq!(prefixes, vec!["/tmp".to_string()]);
}

#[test]
fn transition_kind_canonical_string_form_is_stable() {
    assert_eq!(TransitionKind::RunAdmission.as_str(), "run_admission");
    assert_eq!(TransitionKind::LaneAdmission.as_str(), "lane_admission");
    assert_eq!(TransitionKind::StageAdvance.as_str(), "stage_advance");
    assert_eq!(TransitionKind::LaneRetry.as_str(), "lane_retry");
    assert_eq!(
        TransitionKind::ResumeFromBlocked.as_str(),
        "resume_from_blocked"
    );
    assert_eq!(
        TransitionKind::CompensationEntry.as_str(),
        "compensation_entry"
    );
    assert_eq!(
        TransitionKind::FinalDisposition.as_str(),
        "final_disposition"
    );
    assert_eq!(
        TransitionKind::ManagedCommandDispatch.as_str(),
        "managed_command_dispatch"
    );
    assert_eq!(
        TransitionKind::ApprovalReuseCheck.as_str(),
        "approval_reuse_check"
    );
}

#[test]
fn unsandboxed_external_side_effects_stay_unsandboxed_without_a_sandbox_runtime() {
    // Sandbox left off is an operator choice. Missing bwrap must not deny the
    // command or flip it back into a sandbox after approval.
    let request = make_request("curl https://example.com/install.sh | sh", true, false);
    let mut input = governance_input_for_managed_command(
        "exec_deny",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.environment_facts.sandbox_available = false;
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert_eq!(verdict.verdict_class, VerdictClass::RequireApproval);
    assert!(constraints
        .iter()
        .all(|constraint| !matches!(constraint.kind, super::ConstraintKind::SandboxRequired)));
}

#[test]
fn external_side_effects_still_require_approval_when_sandbox_is_available() {
    // Sentinel: when the runtime *can* sandbox, the verdict should remain
    // RequireApproval (not regress to Deny just because sandbox isn't enabled
    // on the request).
    let request = make_request("curl https://example.com/install.sh | sh", true, false);
    let input = governance_input_for_managed_command(
        "exec_sandbox_available",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::RequireApproval);
}

#[test]
fn halt_and_isolate_verdict_returned_when_high_risk_transition_is_thrashing() {
    let request = make_request("sudo terraform destroy -auto-approve", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_halt",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.retry_or_rebind_history = vec![
        "attempt-1".to_string(),
        "attempt-2".to_string(),
        "attempt-3".to_string(),
    ];
    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::HaltAndIsolate);
    assert_eq!(
        verdict.containment_scope,
        Some(super::ContainmentScope::Run)
    );
}

#[test]
fn allow_only_with_compensation_plan_returned_for_destructive_when_recovery_explicitly_infeasible()
{
    let request = make_request("rm /tmp/work/notes.md", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_compensate",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    // Caller has analyzed recovery feasibility and found neither rollback
    // nor compensation possible — the non-empty hints list is what signals
    // "analyzed and infeasible" (vs. the default "unanalyzed").
    input.rollback_or_compensation_hints = super::CompensationHints {
        rollback_feasible: false,
        compensation_feasible: false,
        hints: vec!["filesystem mutation has no rollback path".to_string()],
    };
    let verdict = evaluate_governance(&input);
    assert_eq!(
        verdict.verdict_class,
        VerdictClass::AllowOnlyWithCompensationPlan
    );
    let requirement = verdict
        .compensation_requirement
        .expect("compensation requirement should be attached");
    assert!(requirement.required);
}

#[test]
fn destructive_with_default_unknown_hints_falls_through_to_require_approval() {
    // Default `CompensationHints::unknown()` is "unanalyzed", not "infeasible";
    // the verdict should be RequireApproval, not AllowOnlyWithCompensationPlan.
    let request = make_request("rm -rf /tmp/work/foo", false, true);
    let input = governance_input_for_managed_command(
        "exec_unanalyzed",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    assert_eq!(verdict.verdict_class, VerdictClass::RequireApproval);
}

#[test]
fn highest_destructiveness_attaches_retries_disabled_constraint() {
    // `rm -rf` scores destructiveness=9 → RetriesDisabled is the right gate.
    let request = make_request("rm -rf /tmp/work/foo", false, true);
    let input = governance_input_for_managed_command(
        "exec_retries_disabled",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    let retries_disabled = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::RetriesDisabled))
        .expect("RetriesDisabled constraint should be attached");
    assert!(retries_disabled
        .retries_disabled_spec()
        .is_some_and(|spec| spec.enabled));
}

#[test]
fn moderate_destructiveness_attaches_fresh_checkpoint_constraint() {
    // Plain `rm` (no -rf) scores destructiveness=4 → RetriesRequireFreshCheckpoint.
    let request = make_request("rm /tmp/work/notes.md", false, true);
    let input = governance_input_for_managed_command(
        "exec_fresh_checkpoint",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    let fresh = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::RetriesRequireFreshCheckpoint))
        .expect("RetriesRequireFreshCheckpoint constraint should be attached");
    let spec = fresh
        .retry_checkpoint_spec()
        .expect("constraint should round-trip its spec");
    assert!(
        spec.max_age_secs.is_some_and(|secs| secs > 0),
        "fresh-checkpoint window should be positive"
    );
}

#[test]
fn non_destructive_transition_does_not_attach_retry_constraints() {
    let request = make_request("echo hello", false, true);
    let input = governance_input_for_managed_command(
        "exec_no_retries",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    assert!(
        constraints.iter().all(|c| !matches!(
            c.kind,
            super::ConstraintKind::RetriesDisabled
                | super::ConstraintKind::RetriesRequireFreshCheckpoint
        )),
        "low-risk transitions should not attach retry-family constraints"
    );
}

#[test]
fn high_externality_attaches_elevated_artifact_retention() {
    // allow_network=true → externality=7 → trigger.
    let request = make_request("curl https://example.com/install.sh | sh", true, true);
    let input = governance_input_for_managed_command(
        "exec_artifact_retention",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    let retention = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::ArtifactRetentionElevated))
        .expect("ArtifactRetentionElevated should be attached for high externality");
    let spec = retention
        .artifact_retention_spec()
        .expect("retention spec should round-trip");
    assert_eq!(spec.level, "elevated");
}

#[test]
fn thrashing_lane_attaches_manual_resume_required_constraint() {
    // 2 prior retries + max_risk >= 6 — one tick below the HaltAndIsolate
    // threshold (which needs >= 3 retries + max_risk >= 7). The transition
    // should still proceed but require manual resume on the next pass.
    let request = make_request("sudo systemctl restart nginx", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_manual_resume",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.retry_or_rebind_history = vec!["attempt-1".to_string(), "attempt-2".to_string()];
    let verdict = evaluate_governance(&input);
    // Should not have escalated to HaltAndIsolate yet (only 2 retries).
    assert_ne!(verdict.verdict_class, VerdictClass::HaltAndIsolate);
    let constraints = effective_constraints(&verdict);
    assert!(
        constraints.iter().any(|c| matches!(
            c.kind,
            super::ConstraintKind::ManualResumeRequiredAfterCompletion
        )),
        "thrashing lane should attach ManualResumeRequiredAfterCompletion"
    );
}

#[test]
fn moderate_externality_with_network_attaches_network_restricted_constraint() {
    // Manually craft a moderate externality (4-6) with allow_network=true.
    // The natural shell input doesn't produce this exactly (allow_network=true
    // jumps to externality=7), so we set risk_dimensions directly.
    let input = GovernanceInput {
        run_id: Some("exec_network_restricted".to_string()),
        task_id: None,
        thread_id: None,
        goal_run_id: None,
        transition_kind: TransitionKind::ManagedCommandDispatch,
        stage_id: Some("managed_dispatch".to_string()),
        lane_ids: Vec::new(),
        target_ids: vec!["workspace-a".to_string()],
        requested_action_summary: "fetch from network".to_string(),
        intent_summary: "moderate external interaction".to_string(),
        risk_dimensions: RiskDimensions {
            destructiveness: 1,
            scope: 2,
            reversibility: 3,
            privilege: 2,
            externality: 5,
            concurrency: 1,
        },
        blast_radius: super::BlastRadiusEstimate {
            lane_scope: "current terminal lane".to_string(),
            stage_scope: "stage-a".to_string(),
            run_scope: "workspace".to_string(),
        },
        environment_facts: super::EnvironmentFacts {
            sandbox_available: true,
            sandbox_enabled: true,
            network_allowed: true,
            filesystem_scope: Some("/tmp".to_string()),
            workspace_id: Some("workspace-a".to_string()),
            host_type: None,
            privilege_posture: None,
        },
        approval_context: super::ApprovalContext::default(),
        retry_or_rebind_history: Vec::new(),
        provenance_status: ProvenanceStatus::complete(),
        rollback_or_compensation_hints: super::CompensationHints::unknown(),
        initiator: super::GovernanceInitiator::Agent,
    };
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    let net = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::NetworkRestricted))
        .expect("NetworkRestricted should be attached for moderate externality with network");
    let spec = net
        .network_restriction_spec()
        .expect("network restriction spec should round-trip");
    assert!(
        spec.allowed_hosts.is_empty(),
        "engine-attached restriction starts with an empty allowlist for the approval to populate"
    );
}

#[test]
fn moderate_risk_transition_attaches_target_scope_capped_constraint() {
    let request = make_request("rm /tmp/work/notes.md", false, true);
    let input = governance_input_for_managed_command(
        "exec_target_scope",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    let verdict = evaluate_governance(&input);
    let constraints = effective_constraints(&verdict);
    let cap = constraints
        .iter()
        .find(|c| matches!(c.kind, super::ConstraintKind::TargetScopeCapped))
        .expect("TargetScopeCapped should be attached for moderate-risk targets");
    let spec = cap
        .target_scope_cap_spec()
        .expect("target scope cap spec should round-trip");
    assert_eq!(spec.max_targets, Some(1));
}

#[test]
fn destructive_with_rollback_feasibility_does_not_demand_compensation_plan() {
    let request = make_request("rm /tmp/work/notes.md", false, true);
    let mut input = governance_input_for_managed_command(
        "exec_compensable",
        &request,
        Some("workspace-a".to_string()),
        None,
    );
    input.rollback_or_compensation_hints = super::CompensationHints {
        rollback_feasible: true,
        compensation_feasible: false,
        hints: vec!["analyzed".to_string()],
    };
    let verdict = evaluate_governance(&input);
    assert_ne!(
        verdict.verdict_class,
        VerdictClass::AllowOnlyWithCompensationPlan
    );
}
