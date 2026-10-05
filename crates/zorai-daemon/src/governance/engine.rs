use super::{
    compute_policy_fingerprint, CompensationRequirement, ContainmentScope, GovernanceConstraint,
    GovernanceInput, GovernanceVerdict, ProvenanceCompleteness, RiskClass,
};

/// Threshold of `retry_or_rebind_history.len()` above which a still-risky
/// transition is considered to be thrashing and gets halted-and-isolated
/// rather than retried again.
const HALT_AND_ISOLATE_RETRY_THRESHOLD: usize = 3;

/// One below `HALT_AND_ISOLATE_RETRY_THRESHOLD`: at this point the lane
/// hasn't earned containment yet, but the next failure should require an
/// explicit operator unblock rather than another autonomous retry.
const MANUAL_RESUME_RETRY_THRESHOLD: usize = 2;

/// `retries_require_fresh_checkpoint` won't accept a checkpoint older than this
/// many seconds when a retry is attempted. 5 minutes is short enough that the
/// captured state still reflects pre-retry conditions on most workflows.
const RETRY_CHECKPOINT_MAX_AGE_SECS: u64 = 300;

pub(crate) fn evaluate_governance(input: &GovernanceInput) -> GovernanceVerdict {
    let fingerprint = compute_policy_fingerprint(input);
    let mut rationale = Vec::new();
    let mut constraints = Vec::new();

    if matches!(
        input.provenance_status.completeness,
        ProvenanceCompleteness::Insufficient
    ) && input.risk_dimensions.destructiveness >= 4
    {
        rationale.push("provenance is insufficient for a side-effecting transition".to_string());
        return GovernanceVerdict::defer(fingerprint, rationale);
    }

    let max_risk = input.risk_dimensions.max_score();

    // Halt-and-isolate: a risky transition has already been retried/rebound
    // several times. Continuing to retry would just thrash; freeze the work
    // and surface the loop to the operator. Detected here so it short-circuits
    // before any approval prompt is generated for what's clearly a stuck loop.
    if input.retry_or_rebind_history.len() >= HALT_AND_ISOLATE_RETRY_THRESHOLD && max_risk >= 7 {
        rationale.push(format!(
            "transition has been retried {} times at high risk; halting and isolating to prevent thrash",
            input.retry_or_rebind_history.len()
        ));
        return GovernanceVerdict::halt_and_isolate(
            fingerprint,
            RiskClass::Critical,
            rationale,
            ContainmentScope::Run,
        );
    }

    if input.risk_dimensions.concurrency >= 7 {
        constraints.push(GovernanceConstraint::serial_only(
            "high concurrency blast radius requires serialized execution",
        ));
    }

    // Attach the secondary constraint family (retries, checkpoints, retention,
    // manual resume, network/target scope) *before* the early-return verdict
    // paths so the constraints flow through into RequireApproval /
    // AllowWithConstraints / Allow verdicts uniformly.
    attach_secondary_constraints(input, &mut constraints);

    if input.risk_dimensions.externality >= 7 && !input.environment_facts.sandbox_enabled {
        // Caller left sandbox off (operator setting or explicit request).
        // Approval still gates the external side effect. Do not rewrite the
        // command into bubblewrap: that mount set hides /run, so
        // /etc/resolv.conf → ../run/systemd/resolve/stub-resolv.conf cannot
        // resolve and every networked command fails closed.
        rationale.push("external side effects without sandboxing require approval".to_string());
        attach_filesystem_scope_constraint(input, &mut constraints);
        return GovernanceVerdict::require_approval(
            fingerprint,
            RiskClass::High,
            rationale,
            constraints,
        );
    }

    // Allow-only-with-compensation-plan: destructive work with no automatic
    // rollback and no compensation feasibility is a class apart from regular
    // require-approval. The operator must commit to a recovery plan before
    // the work runs; a bare yes/no approval would obscure the lack of an
    // exit strategy.
    //
    // Gating: the trigger requires a non-empty `hints` list. The default
    // `CompensationHints::unknown()` leaves both feasibility flags false AND
    // hints empty, which we treat as "unanalyzed" rather than "known
    // infeasible" — falling through to RequireApproval. Only callers that
    // explicitly populate hints (signalling that recovery feasibility *was*
    // analyzed and came back negative) trip this verdict.
    if input.risk_dimensions.destructiveness >= 4
        && !input.rollback_or_compensation_hints.rollback_feasible
        && !input.rollback_or_compensation_hints.compensation_feasible
        && !input.rollback_or_compensation_hints.hints.is_empty()
    {
        rationale.push(
            "destructive transition lacks both rollback and compensation feasibility; \
             explicit compensation plan required before execution"
                .to_string(),
        );
        return GovernanceVerdict::allow_only_with_compensation_plan(
            fingerprint,
            RiskClass::High,
            rationale,
            CompensationRequirement {
                required: true,
                reason: "no rollback or compensation path is available for this transition"
                    .to_string(),
                plan_reference: None,
            },
        );
    }

    if max_risk >= 8 {
        rationale.push("risk score exceeded approval threshold".to_string());
        attach_filesystem_scope_constraint(input, &mut constraints);
        return GovernanceVerdict::require_approval(
            fingerprint,
            RiskClass::High,
            rationale,
            constraints,
        );
    }

    let risk_class = if max_risk >= 5 {
        RiskClass::Medium
    } else {
        RiskClass::Low
    };

    GovernanceVerdict::allow_with_optional_constraints(
        fingerprint,
        risk_class,
        rationale,
        constraints,
    )
}

/// Attach the secondary constraint family. These constraints don't decide
/// the verdict class on their own — the engine's top-level branches do that —
/// but they ride alongside the verdict to express what *must* be true if the
/// transition proceeds. Each producer mirrors a consumer in
/// `governance::constraints::can_honor_constraints` so attached constraints
/// actually gate dispatch.
fn attach_secondary_constraints(
    input: &GovernanceInput,
    constraints: &mut Vec<GovernanceConstraint>,
) {
    let dims = &input.risk_dimensions;

    // RetriesDisabled: highest-tier destructive actions should never be
    // auto-retried. A retry would duplicate the destructive side effect with
    // no guarantee the operator wanted it twice.
    if dims.destructiveness >= 8 {
        constraints.push(GovernanceConstraint::retries_disabled(
            "destructive transition must not be auto-retried; a retry would duplicate side effects",
        ));
    } else if dims.destructiveness >= 4 {
        // RetriesRequireFreshCheckpoint: moderate-destructive actions may retry
        // but only after a fresh pre-execution checkpoint. Stale checkpoints
        // would let a retry overwrite recovery state captured before the
        // first attempt.
        constraints.push(GovernanceConstraint::retries_require_fresh_checkpoint(
            Some(RETRY_CHECKPOINT_MAX_AGE_SECS),
            "moderate-destructive transition may retry only with a fresh pre-execution checkpoint",
        ));
    }

    // ArtifactRetentionElevated: high-externality or destructive transitions
    // produce evidence the operator will want to keep for post-incident
    // analysis. Elevate retention so artifacts aren't garbage-collected
    // before review.
    if dims.externality >= 7 || dims.destructiveness >= 8 {
        constraints.push(GovernanceConstraint::artifact_retention_elevated(
            "elevated",
            "high-impact transition: retain execution artifacts at elevated level for post-incident review",
        ));
    }

    // ManualResumeRequiredAfterCompletion: precursor to HaltAndIsolate. If
    // the lane is already thrashing (2+ retries) at moderate-or-higher risk,
    // the next completion of this transition must wait for an explicit
    // operator resume before any further work runs on the lane.
    if input.retry_or_rebind_history.len() >= MANUAL_RESUME_RETRY_THRESHOLD && dims.max_score() >= 6
    {
        constraints.push(
            GovernanceConstraint::manual_resume_required_after_completion(
                "lane has been retried multiple times at non-trivial risk; require operator to resume before further work",
            ),
        );
    }

    // NetworkRestricted: when network is allowed and externality is at least
    // moderate, surface an empty allowlist as a constraint. An empty allowlist
    // fails closed in `network_restriction_satisfied`, which forces the
    // approval flow (or the request's caller) to either populate hosts or
    // disable network — neither pathway lets the agent silently reach out to
    // arbitrary hosts. Externality >= 7 uses the approval path above and
    // leaves sandbox_enabled as the caller set it.
    if input.environment_facts.network_allowed && dims.externality >= 4 && dims.externality < 7 {
        constraints.push(GovernanceConstraint::network_restricted(
            Vec::new(),
            "network access requires an explicit host allowlist before this transition can execute",
        ));
    }

    // TargetScopeCapped: when there's a non-empty target_ids list and the
    // transition is at least moderately impactful, cap the target set to
    // what's already declared. This prevents an approval for one workspace
    // from being reused to mutate a sibling workspace.
    if !input.target_ids.is_empty() && dims.max_score() >= 4 {
        let allowed_prefixes: Vec<String> = input
            .target_ids
            .iter()
            .filter(|id| id.starts_with('/'))
            .cloned()
            .collect();
        let max_targets = Some(input.target_ids.len());
        constraints.push(GovernanceConstraint::target_scope_capped(
            allowed_prefixes,
            max_targets,
            "approval is bound to the declared target set; new targets require a fresh decision",
        ));
    }
}

/// When an approval is required for a request that has a known filesystem
/// anchor (cwd), bind the approval to that subtree so future grant reuse can
/// be scope-checked at dispatch.
fn attach_filesystem_scope_constraint(
    input: &GovernanceInput,
    constraints: &mut Vec<GovernanceConstraint>,
) {
    let Some(cwd) = input.environment_facts.filesystem_scope.as_deref() else {
        return;
    };
    if cwd.is_empty() {
        return;
    }
    constraints.push(GovernanceConstraint::filesystem_scope_narrowed(
        vec![cwd.to_string()],
        "approval scoped to the requesting working directory; dispatch must stay within this subtree",
    ));
}

#[cfg(test)]
mod tests {
    use super::evaluate_governance;
    use crate::governance::{
        effective_constraints, governance_input_for_managed_command, ConstraintKind, VerdictClass,
    };
    use zorai_protocol::{ManagedCommandRequest, ManagedCommandSource, SecurityLevel};

    fn request(command: &str, allow_network: bool, sandbox_enabled: bool) -> ManagedCommandRequest {
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
    fn approval_for_unsandboxed_external_side_effects_does_not_force_sandbox() {
        let input = governance_input_for_managed_command(
            "exec_ext",
            &request("curl https://example.com/install.sh | sh", true, false),
            Some("workspace-a".to_string()),
            None,
        );

        let verdict = evaluate_governance(&input);
        let constraints = effective_constraints(&verdict);

        assert_eq!(verdict.verdict_class, VerdictClass::RequireApproval);
        assert!(constraints
            .iter()
            .all(|constraint| !matches!(constraint.kind, ConstraintKind::SandboxRequired)));
    }
}
