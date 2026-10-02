use super::*;

use crate::governance::{
    apply_constraints_to_request, can_honor_constraints, effective_constraints,
    evaluate_governance, governance_input_for_managed_command, ConstraintKind, GovernanceInput,
    GovernanceVerdict, RiskClass, TransitionKind, VerdictClass,
};
use crate::history::{ApprovalRecordRow, AuditEntryRow, GovernanceEvaluationRow};
use serde_json::json;
use zorai_protocol::{ApprovalPayload, SecurityLevel};

fn transition_kind_str(kind: &TransitionKind) -> &'static str {
    match kind {
        TransitionKind::RunAdmission => "run_admission",
        TransitionKind::LaneAdmission => "lane_admission",
        TransitionKind::StageAdvance => "stage_advance",
        TransitionKind::LaneRetry => "lane_retry",
        TransitionKind::ResumeFromBlocked => "resume_from_blocked",
        TransitionKind::CompensationEntry => "compensation_entry",
        TransitionKind::FinalDisposition => "final_disposition",
        TransitionKind::ManagedCommandDispatch => "managed_command_dispatch",
        TransitionKind::ApprovalReuseCheck => "approval_reuse_check",
    }
}

fn risk_class_str(risk_class: &RiskClass) -> &'static str {
    match risk_class {
        RiskClass::Low => "low",
        RiskClass::Medium => "medium",
        RiskClass::High => "high",
        RiskClass::Critical => "critical",
    }
}

fn verdict_class_str(verdict_class: &VerdictClass) -> &'static str {
    match verdict_class {
        VerdictClass::Allow => "allow",
        VerdictClass::AllowWithConstraints => "allow_with_constraints",
        VerdictClass::RequireApproval => "require_approval",
        VerdictClass::Defer => "defer",
        VerdictClass::Deny => "deny",
        VerdictClass::HaltAndIsolate => "halt_and_isolate",
        VerdictClass::AllowOnlyWithCompensationPlan => "compensation_plan_required",
    }
}

fn constraint_kind_str(kind: &ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::SandboxRequired => "sandbox_required",
        ConstraintKind::NetworkDenied => "network_denied",
        ConstraintKind::NetworkRestricted => "network_restricted",
        ConstraintKind::FilesystemScopeNarrowed => "filesystem_scope_narrowed",
        ConstraintKind::TargetScopeCapped => "target_scope_capped",
        ConstraintKind::SerialOnlyExecution => "serial_only_execution",
        ConstraintKind::RetriesDisabled => "retries_disabled",
        ConstraintKind::RetriesRequireFreshCheckpoint => "retries_require_fresh_checkpoint",
        ConstraintKind::ArtifactRetentionElevated => "artifact_retention_elevated",
        ConstraintKind::ManualResumeRequiredAfterCompletion => {
            "manual_resume_required_after_completion"
        }
    }
}

fn approval_resolution_str(decision: ApprovalDecision) -> &'static str {
    match decision {
        ApprovalDecision::ApproveOnce => "approved_once",
        ApprovalDecision::ApproveSession => "approved_session",
        ApprovalDecision::Deny => "denied",
    }
}

fn blast_radius_summary(input: &GovernanceInput) -> String {
    format!(
        "{} (lane: {}, stage: {})",
        input.blast_radius.run_scope, input.blast_radius.lane_scope, input.blast_radius.stage_scope
    )
}

fn approval_payload_from_verdict(
    execution_id: &str,
    request: &ManagedCommandRequest,
    workspace_id: Option<String>,
    input: &GovernanceInput,
    verdict: &GovernanceVerdict,
    expires_at: Option<u64>,
) -> ApprovalPayload {
    let constraints = effective_constraints(verdict);
    let mut reasons = verdict.rationale.clone();
    reasons.extend(constraints.iter().filter_map(|constraint| {
        constraint.rationale.as_ref().map(|rationale| {
            format!(
                "constraint {}: {}",
                constraint_kind_str(&constraint.kind),
                rationale
            )
        })
    }));

    if reasons.is_empty() {
        reasons.push(format!(
            "governance classified this transition as {} risk",
            risk_class_str(&verdict.risk_class)
        ));
    }

    ApprovalPayload {
        approval_id: format!("apr_{}", Uuid::new_v4()),
        execution_id: execution_id.to_string(),
        command: request.command.clone(),
        rationale: request.rationale.clone(),
        risk_level: risk_class_str(&verdict.risk_class).to_string(),
        blast_radius: blast_radius_summary(input),
        reasons,
        workspace_id,
        allow_network: request.allow_network,
        transition_kind: Some(transition_kind_str(&input.transition_kind).to_string()),
        policy_fingerprint: Some(verdict.policy_fingerprint.clone()),
        expires_at,
        constraints: constraints
            .iter()
            .map(|constraint| constraint_kind_str(&constraint.kind).to_string())
            .collect(),
        scope_summary: verdict
            .approval_requirement
            .as_ref()
            .map(|requirement| requirement.scope_summary.clone())
            .or_else(|| Some(blast_radius_summary(input))),
    }
}

fn governance_rejection_message(verdict: &GovernanceVerdict) -> String {
    let verdict_label = verdict_class_str(&verdict.verdict_class);

    let reason = if verdict.rationale.is_empty() {
        format!("governance returned {verdict_label} for this transition")
    } else {
        verdict.rationale.join("; ")
    };

    format!(
        "managed command blocked by governance ({verdict_label}, {} risk): {reason}",
        risk_class_str(&verdict.risk_class),
    )
}

fn should_persist_governance_trace(
    verdict: &GovernanceVerdict,
    constraints: &[crate::governance::GovernanceConstraint],
) -> bool {
    !matches!(verdict.verdict_class, VerdictClass::Allow)
        || !constraints.is_empty()
        || !matches!(verdict.risk_class, RiskClass::Low)
}

async fn persist_managed_command_governance_trace(
    history: &crate::history::HistoryStore,
    session_id: SessionId,
    request: &ManagedCommandRequest,
    input: &GovernanceInput,
    verdict: &GovernanceVerdict,
    constraints: &[crate::governance::GovernanceConstraint],
    can_honor: bool,
) -> Result<()> {
    if !should_persist_governance_trace(verdict, constraints) {
        return Ok(());
    }

    let created_at = crate::history::now_ts();
    let constraint_labels = constraints
        .iter()
        .map(|constraint| constraint_kind_str(&constraint.kind).to_string())
        .collect::<Vec<_>>();
    let rationale_text = if verdict.rationale.is_empty() {
        format!(
            "governance returned {} for this transition",
            verdict_class_str(&verdict.verdict_class)
        )
    } else {
        verdict.rationale.join("; ")
    };

    let reasoning = format!(
        "Governance classified managed command `{}` as {} with {} risk.",
        request.command,
        verdict_class_str(&verdict.verdict_class),
        risk_class_str(&verdict.risk_class),
    );
    let selected = crate::agent::learning::traces::DecisionOption {
        option_type: "governance_evaluation".to_string(),
        reasoning: if constraint_labels.is_empty() {
            reasoning.clone()
        } else {
            format!(
                "{} Triggered constraints: {}.",
                reasoning,
                constraint_labels.join(", ")
            )
        },
        rejection_reason: None,
        estimated_success_prob: Some(match verdict.verdict_class {
            VerdictClass::Allow => 0.9,
            VerdictClass::AllowWithConstraints => 0.72,
            VerdictClass::RequireApproval => 0.45,
            VerdictClass::Defer => 0.2,
            VerdictClass::Deny
            | VerdictClass::HaltAndIsolate
            | VerdictClass::AllowOnlyWithCompensationPlan => 0.1,
        }),
        arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(&format!(
            "{}|{}|{}|{}",
            request.command, request.rationale, session_id, verdict.policy_fingerprint
        ))),
    };

    let mut rejected_options = Vec::new();
    if !matches!(verdict.verdict_class, VerdictClass::Allow) || !constraint_labels.is_empty() {
        rejected_options.push(crate::agent::learning::traces::DecisionOption {
            option_type: "unconstrained_dispatch".to_string(),
            reasoning:
                "Execute the managed command immediately without governance gating or constraint enforcement."
                    .to_string(),
            rejection_reason: Some(rationale_text.clone()),
            estimated_success_prob: Some(0.15),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(&format!(
                "immediate|{}|{}",
                request.command, session_id
            ))),
        });
    }

    let mut causal_factors = vec![crate::agent::learning::traces::CausalFactor {
        factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
        description: format!(
            "governance verdict {} with {} risk for {}",
            verdict_class_str(&verdict.verdict_class),
            risk_class_str(&verdict.risk_class),
            transition_kind_str(&input.transition_kind)
        ),
        weight: 0.85,
    }];
    causal_factors.push(crate::agent::learning::traces::CausalFactor {
        factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
        description: format!(
            "policy fingerprint {} and stage {}",
            &verdict.policy_fingerprint,
            input.stage_id.as_deref().unwrap_or("unknown")
        ),
        weight: 0.55,
    });
    causal_factors.push(crate::agent::learning::traces::CausalFactor {
        factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
        description: format!(
            "provenance completeness: {:?}",
            input.provenance_status.completeness
        )
        .to_lowercase(),
        weight: 0.45,
    });
    if !constraint_labels.is_empty() {
        causal_factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
            description: format!("triggered constraints: {}", constraint_labels.join(", ")),
            weight: 0.65,
        });
    }
    if !can_honor {
        causal_factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
            description: "current runtime could not honor computed governance constraints"
                .to_string(),
            weight: 0.7,
        });
    }

    let outcome = match verdict.verdict_class {
        VerdictClass::Allow | VerdictClass::AllowWithConstraints if can_honor => {
            crate::agent::learning::traces::CausalTraceOutcome::Success
        }
        VerdictClass::RequireApproval => {
            crate::agent::learning::traces::CausalTraceOutcome::Unresolved
        }
        _ => crate::agent::learning::traces::CausalTraceOutcome::Failure {
            reason: if can_honor {
                rationale_text.clone()
            } else {
                format!(
                    "{}; runtime could not honor computed constraints",
                    rationale_text
                )
            },
        },
    };

    let trace = crate::agent::learning::traces::CausalTrace {
        trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        task_id: input.task_id.clone(),
        decision_type: crate::agent::learning::traces::DecisionType::GovernanceEvaluation,
        selected,
        rejected_options,
        context_hash: crate::agent::learning::traces::hash_context_blob(&format!(
            "{}|{}|{}|{}|{}",
            input.requested_action_summary,
            input.intent_summary,
            session_id,
            verdict.policy_fingerprint,
            verdict_class_str(&verdict.verdict_class)
        )),
        causal_factors,
        outcome,
        model_used: None,
        created_at,
    };

    let selected_json = serde_json::to_string(&trace.selected)?;
    let rejected_json = serde_json::to_string(&trace.rejected_options)?;
    let factors_json = serde_json::to_string(&trace.causal_factors)?;
    let outcome_json = serde_json::to_string(&trace.outcome)?;
    history
        .insert_causal_trace(
            &trace.trace_id,
            trace.thread_id.as_deref(),
            trace.goal_run_id.as_deref(),
            trace.task_id.as_deref(),
            "governance_evaluation",
            trace.decision_type.family_label(),
            &selected_json,
            &rejected_json,
            &trace.context_hash,
            &factors_json,
            &outcome_json,
            trace.model_used.as_deref(),
            trace.created_at,
        )
        .await?;

    let raw_data_json = serde_json::json!({
        "session_id": session_id.to_string(),
        "command": request.command,
        "rationale": request.rationale,
        "transition_kind": transition_kind_str(&input.transition_kind),
        "stage_id": input.stage_id,
        "lane_ids": input.lane_ids,
        "target_ids": input.target_ids,
        "verdict_class": verdict_class_str(&verdict.verdict_class),
        "risk_class": risk_class_str(&verdict.risk_class),
        "policy_fingerprint": verdict.policy_fingerprint,
        "constraints": constraint_labels,
        "rationale_list": verdict.rationale,
        "provenance_completeness": format!("{:?}", input.provenance_status.completeness).to_lowercase(),
        "missing_provenance_evidence": input.provenance_status.missing_evidence,
        "can_honor_constraints": can_honor,
    });
    let confidence_val = trace.selected.estimated_success_prob;
    history
        .insert_action_audit(&AuditEntryRow {
            id: format!("audit-governance-{}", trace.trace_id),
            timestamp: trace.created_at as i64,
            action_type: "governance_evaluation".to_string(),
            summary: format!(
                "Governance evaluated managed command dispatch as {} ({})",
                verdict_class_str(&verdict.verdict_class),
                risk_class_str(&verdict.risk_class)
            ),
            explanation: Some(trace.selected.reasoning.clone()),
            confidence: confidence_val,
            confidence_band: confidence_val
                .map(|prob| crate::agent::confidence_band(prob).as_str().to_string()),
            causal_trace_id: Some(trace.trace_id.clone()),
            thread_id: trace.thread_id.clone(),
            goal_run_id: trace.goal_run_id.clone(),
            task_id: trace.task_id.clone(),
            raw_data_json: Some(raw_data_json.to_string()),
        })
        .await?;

    Ok(())
}

async fn queue_with_snapshot(
    snapshots: &crate::snapshot::SnapshotStore,
    session: &Arc<Mutex<PtySession>>,
    workspace_id: Option<String>,
    session_id: SessionId,
    execution_id: String,
    request: ManagedCommandRequest,
    snapshot_reason: &str,
) -> Result<(usize, Option<zorai_protocol::SnapshotInfo>)> {
    let snapshot = {
        let session = session.lock().await;
        snapshots
            .create_snapshot(
                workspace_id,
                Some(session_id),
                request.cwd.as_deref().or_else(|| session.cwd()),
                Some(&request.command),
                snapshot_reason,
            )
            .await?
    };
    let position =
        session
            .lock()
            .await
            .queue_managed_command(execution_id, request, snapshot.clone())?;
    Ok((position, snapshot))
}

impl SessionManager {
    async fn prune_expired_session_grants(&self, session_id: SessionId) {
        let now = crate::history::now_ts();
        let mut grants = self.session_approval_grants.write().await;
        if let Some(entries) = grants.get_mut(&session_id) {
            entries.retain(|grant| match grant.expires_at {
                Some(expires_at) => expires_at > now,
                None => true,
            });
            if entries.is_empty() {
                grants.remove(&session_id);
            }
        }
    }

    async fn find_valid_session_grant(
        &self,
        session_id: SessionId,
        policy_fingerprint: &str,
    ) -> Option<SessionApprovalGrant> {
        self.prune_expired_session_grants(session_id).await;
        self.session_approval_grants
            .read()
            .await
            .get(&session_id)
            .and_then(|grants| {
                grants
                    .iter()
                    .find(|grant| grant.policy_fingerprint == policy_fingerprint)
                    .cloned()
            })
    }

    async fn store_session_grant(
        &self,
        session_id: SessionId,
        _approval_id: &str,
        policy_fingerprint: &str,
        expires_at: Option<u64>,
    ) {
        self.prune_expired_session_grants(session_id).await;
        let mut grants = self.session_approval_grants.write().await;
        let entries = grants.entry(session_id).or_default();
        entries.retain(|grant| grant.policy_fingerprint != policy_fingerprint);
        entries.push(SessionApprovalGrant {
            approval_id: _approval_id.to_string(),
            policy_fingerprint: policy_fingerprint.to_string(),
            expires_at,
        });
    }

    pub async fn spawn(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        workspace_id: Option<String>,
        env: Option<Vec<(String, String)>>,
        cols: u16,
        rows: u16,
    ) -> Result<(SessionId, broadcast::Receiver<DaemonMessage>)> {
        let id = Uuid::new_v4();
        let shell_for_audit = shell.clone();
        let cwd_for_audit = cwd.clone();
        let workspace_for_audit = workspace_id.clone();
        let session = PtySession::spawn(
            id,
            shell,
            cwd,
            workspace_id,
            env,
            cols,
            rows,
            (*self.history).clone(),
            self.pty_channel_capacity,
        )?;
        let rx = session.subscribe();

        self.sessions
            .write()
            .await
            .insert(id, Arc::new(Mutex::new(session)));

        self.persist_state().await;

        crate::governance::record_transition_audit(
            &self.history,
            crate::governance::TransitionKind::LaneAdmission,
            crate::governance::TransitionAuditIds {
                run_id: Some(id.to_string()),
                ..Default::default()
            },
            serde_json::json!({
                "session_id": id.to_string(),
                "shell": shell_for_audit,
                "cwd": cwd_for_audit,
                "workspace_id": workspace_for_audit,
                "cols": cols,
                "rows": rows,
            }),
            "admitted",
        )
        .await;

        tracing::info!(%id, "session spawned");
        Ok((id, rx))
    }

    pub async fn clone_session(
        &self,
        source_id: SessionId,
        workspace_id: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
        replay_scrollback: bool,
        cwd_override: Option<String>,
    ) -> Result<(
        SessionId,
        broadcast::Receiver<DaemonMessage>,
        Option<String>,
    )> {
        let source = self
            .sessions
            .read()
            .await
            .get(&source_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {source_id}"))?;

        let (
            shell,
            cwd,
            source_workspace_id,
            source_cols,
            source_rows,
            replay_bytes,
            src_active_cmd,
        ) = {
            let source = source.lock().await;
            (
                source.shell().map(ToOwned::to_owned),
                cwd_override.or_else(|| source.resolved_cwd()),
                source.workspace_id().map(ToOwned::to_owned),
                source.cols(),
                source.rows(),
                if replay_scrollback {
                    source.scrollback(None)
                } else {
                    Vec::new()
                },
                source.active_command(),
            )
        };

        let target_workspace_id = workspace_id.or(source_workspace_id);
        let (id, rx) = self
            .spawn(
                shell,
                cwd,
                target_workspace_id,
                None,
                cols.unwrap_or(source_cols),
                rows.unwrap_or(source_rows),
            )
            .await?;

        if replay_scrollback && !replay_bytes.is_empty() {
            let sanitized = crate::pty_session::sanitize_scrollback_for_replay(&replay_bytes);
            if let Some(cloned_session) = self.sessions.read().await.get(&id).cloned() {
                cloned_session.lock().await.preload_output(&sanitized);
            }
        }

        tracing::info!(%source_id, %id, active_command = ?src_active_cmd, "session cloned");
        Ok((id, rx, src_active_cmd))
    }

    pub async fn write_input(&self, id: SessionId, data: &[u8]) -> Result<()> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.lock().await.write(data)?;
        Ok(())
    }

    pub async fn resize(&self, id: SessionId, cols: u16, rows: u16) -> Result<()> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        session.lock().await.resize(cols, rows)?;
        Ok(())
    }

    pub async fn kill(&self, id: SessionId) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&id)
        };

        if let Some(session) = session {
            session.lock().await.kill()?;
            tracing::info!(%id, "session killed");
        }

        self.persist_state().await;
        Ok(())
    }

    pub async fn subscribe(
        &self,
        id: SessionId,
    ) -> Result<(broadcast::Receiver<DaemonMessage>, bool)> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?
            .clone();
        let s = session.lock().await;
        let rx = s.subscribe();
        let alive = !s.is_dead();
        Ok((rx, alive))
    }

    pub fn read_workspace_topology(&self) -> Option<WorkspaceTopology> {
        let path = match zorai_protocol::ensure_zorai_data_dir() {
            Ok(dir) => dir.join("workspace-topology.json"),
            Err(_) => return None,
        };
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub(crate) async fn session_activity_snapshots(&self) -> Vec<SessionActivitySnapshot> {
        let pending_sessions: std::collections::HashSet<SessionId> = self
            .pending_approvals
            .read()
            .await
            .values()
            .map(|pending| pending.session_id)
            .collect();
        let sessions: Vec<(SessionId, Arc<Mutex<PtySession>>)> = self
            .sessions
            .read()
            .await
            .iter()
            .map(|(id, session)| (*id, session.clone()))
            .collect();

        let mut snapshots = Vec::with_capacity(sessions.len());
        for (id, session) in sessions {
            let session = session.lock().await;
            if session.is_dead() {
                continue;
            }
            let has_active_command = session.active_command().is_some();
            let busy =
                has_active_command || session.has_managed_work() || pending_sessions.contains(&id);
            snapshots.push(SessionActivitySnapshot {
                id,
                workspace_id: session.workspace_id().map(ToOwned::to_owned),
                last_activity_at_ms: session.last_activity_at_ms(),
                has_active_command,
                busy,
            });
        }
        snapshots
    }

    #[cfg(test)]
    pub async fn set_session_last_activity_for_test(
        &self,
        id: SessionId,
        activity_at_ms: u64,
    ) -> bool {
        let Some(session) = self.sessions.read().await.get(&id).cloned() else {
            return false;
        };
        session
            .lock()
            .await
            .set_last_activity_at_ms_for_test(activity_at_ms);
        true
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        self.list_filtered(None).await
    }

    pub async fn list_workspace(&self, workspace_id: &str) -> Vec<SessionInfo> {
        self.list_filtered(Some(workspace_id)).await
    }

    async fn list_filtered(&self, workspace_filter: Option<&str>) -> Vec<SessionInfo> {
        let sessions: Vec<(SessionId, Arc<Mutex<PtySession>>)> = self
            .sessions
            .read()
            .await
            .iter()
            .map(|(id, session)| (*id, session.clone()))
            .collect();

        let mut infos = Vec::with_capacity(sessions.len());
        for (id, session) in sessions {
            let s = session.lock().await;
            if s.is_dead() {
                continue;
            }
            let workspace_id = s.workspace_id().map(ToOwned::to_owned);

            if let Some(filter) = workspace_filter {
                if workspace_id.as_deref() != Some(filter) {
                    continue;
                }
            }

            infos.push(SessionInfo {
                id,
                title: s.title().map(|t| t.to_owned()),
                cwd: s.resolved_cwd().or_else(|| s.cwd().map(|c| c.to_owned())),
                cols: s.cols(),
                rows: s.rows(),
                created_at: s.created_at(),
                workspace_id,
                exit_code: None,
                is_alive: !s.is_dead(),
                active_command: s.active_command(),
            });
        }
        infos
    }

    pub async fn get_scrollback(&self, id: SessionId, max_lines: Option<usize>) -> Result<Vec<u8>> {
        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        let data = session.lock().await.scrollback(max_lines);
        Ok(data)
    }

    pub async fn get_analysis_text(
        &self,
        id: SessionId,
        max_lines: Option<usize>,
    ) -> Result<String> {
        let raw = self.get_scrollback(id, max_lines).await?;
        let stripped = strip_ansi_escapes::strip(&raw);
        Ok(String::from_utf8_lossy(&stripped).into_owned())
    }

    pub async fn get_background_task_status(
        &self,
        background_task_id: &str,
    ) -> Result<Option<BackgroundTaskStatus>> {
        let sessions: Vec<(SessionId, Arc<Mutex<PtySession>>)> = self
            .sessions
            .read()
            .await
            .iter()
            .map(|(id, session)| (*id, session.clone()))
            .collect();

        for (session_id, session) in sessions {
            let live_status = session
                .lock()
                .await
                .managed_command_status(background_task_id);
            if let Some(live_status) = live_status {
                let state = match live_status.state {
                    crate::pty_session::ManagedCommandLiveState::Queued => {
                        BackgroundTaskState::Queued
                    }
                    crate::pty_session::ManagedCommandLiveState::Running => {
                        BackgroundTaskState::Running
                    }
                };
                return Ok(Some(BackgroundTaskStatus {
                    background_task_id: background_task_id.to_string(),
                    kind: "managed_command".to_string(),
                    state,
                    session_id: Some(session_id.to_string()),
                    position: Some(live_status.position),
                    command: Some(live_status.command),
                    exit_code: None,
                    duration_ms: None,
                    snapshot_path: live_status.snapshot_path,
                }));
            }
        }

        if let Some(finished) = self.history.get_managed_finish(background_task_id).await? {
            let state = if finished.exit_code == Some(0) {
                BackgroundTaskState::Completed
            } else {
                BackgroundTaskState::Failed
            };
            return Ok(Some(BackgroundTaskStatus {
                background_task_id: background_task_id.to_string(),
                kind: "managed_command".to_string(),
                state,
                session_id: None,
                position: None,
                command: Some(finished.command),
                exit_code: finished.exit_code,
                duration_ms: finished.duration_ms,
                snapshot_path: finished.snapshot_path,
            }));
        }

        Ok(None)
    }

    pub async fn cancel_queued_managed_command(&self, execution_id: &str) -> bool {
        let sessions: Vec<Arc<Mutex<PtySession>>> =
            self.sessions.read().await.values().cloned().collect();
        for session in sessions {
            if session
                .lock()
                .await
                .cancel_queued_managed_command(execution_id)
            {
                return true;
            }
        }
        false
    }

    pub async fn reap_dead(&self) {
        let mut sessions = self.sessions.write().await;
        let mut dead: Vec<SessionId> = Vec::new();
        for (id, session) in sessions.iter() {
            if session.lock().await.is_dead() {
                dead.push(*id);
            }
        }
        for id in &dead {
            sessions.remove(id);
            tracing::info!(%id, "reaped dead session");
        }

        drop(sessions);
        self.persist_state().await;
    }

    pub async fn execute_managed_command(
        &self,
        id: SessionId,
        request: ManagedCommandRequest,
    ) -> Result<DaemonMessage> {
        validate_command(&request.command, request.language_hint.as_deref())?;

        let session = self
            .sessions
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {id}"))?;
        if session.lock().await.is_dead() {
            anyhow::bail!("session is not alive: {id}");
        }

        let workspace_id = session.lock().await.workspace_id().map(ToOwned::to_owned);
        let execution_id = format!("exec_{}", Uuid::new_v4());
        if matches!(request.security_level, SecurityLevel::Yolo) {
            let (position, snapshot) = queue_with_snapshot(
                &self.snapshots,
                &session,
                workspace_id.clone(),
                id,
                execution_id.clone(),
                request,
                "yolo pre-execution checkpoint",
            )
            .await?;
            return Ok(DaemonMessage::ManagedCommandQueued {
                id,
                execution_id,
                position,
                snapshot,
            });
        }
        let governance_input = governance_input_for_managed_command(
            &execution_id,
            &request,
            workspace_id.clone(),
            Some(id.to_string()),
        );
        let verdict = evaluate_governance(&governance_input);
        let constraints = effective_constraints(&verdict);
        let can_honor = can_honor_constraints(&constraints, &request);

        self.history
            .insert_governance_evaluation(&GovernanceEvaluationRow {
                id: format!("gov_{}", Uuid::new_v4()),
                run_id: governance_input.run_id.clone(),
                task_id: governance_input.task_id.clone(),
                goal_run_id: governance_input.goal_run_id.clone(),
                thread_id: governance_input.thread_id.clone(),
                transition_kind: transition_kind_str(&governance_input.transition_kind).to_string(),
                input_json: serde_json::to_string(&governance_input)?,
                verdict_json: serde_json::to_string(&verdict)?,
                policy_fingerprint: verdict.policy_fingerprint.clone(),
                created_at: crate::history::now_ts(),
            })
            .await?;

        persist_managed_command_governance_trace(
            self.history.as_ref(),
            id,
            &request,
            &governance_input,
            &verdict,
            &constraints,
            can_honor,
        )
        .await?;

        if !can_honor {
            return Ok(DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: Some(execution_id),
                message: format!(
                    "managed command blocked because current runtime cannot honor governance constraints: {}",
                    constraints
                        .iter()
                        .map(|constraint| constraint_kind_str(&constraint.kind))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
        let mut constrained_request = request.clone();
        apply_constraints_to_request(&mut constrained_request, &constraints);

        match verdict.verdict_class {
            VerdictClass::Allow | VerdictClass::AllowWithConstraints => {
                let (position, snapshot) = queue_with_snapshot(
                    &self.snapshots,
                    &session,
                    workspace_id.clone(),
                    id,
                    execution_id.clone(),
                    constrained_request,
                    "pre-execution checkpoint",
                )
                .await?;
                Ok(DaemonMessage::ManagedCommandQueued {
                    id,
                    execution_id,
                    position,
                    snapshot,
                })
            }
            VerdictClass::RequireApproval => {
                let requested_at = crate::history::now_ts();
                let expires_at = verdict
                    .approval_requirement
                    .as_ref()
                    .and_then(|requirement| requirement.expires_at)
                    .or_else(|| {
                        verdict
                            .freshness_window_secs
                            .map(|window| requested_at + window)
                    });

                if let Some(grant) = self
                    .find_valid_session_grant(id, &verdict.policy_fingerprint)
                    .await
                {
                    let reuse_verdict = json!({
                        "reused_approval_id": grant.approval_id,
                        "grant_expires_at": grant.expires_at,
                        "policy_fingerprint": verdict.policy_fingerprint,
                        "checked_at": crate::history::now_ts(),
                    });
                    self.history
                        .insert_governance_evaluation(&GovernanceEvaluationRow {
                            id: format!("gov_{}", Uuid::new_v4()),
                            run_id: governance_input.run_id.clone(),
                            task_id: governance_input.task_id.clone(),
                            goal_run_id: governance_input.goal_run_id.clone(),
                            thread_id: governance_input.thread_id.clone(),
                            transition_kind: transition_kind_str(
                                &TransitionKind::ApprovalReuseCheck,
                            )
                            .to_string(),
                            input_json: serde_json::to_string(&governance_input)?,
                            verdict_json: reuse_verdict.to_string(),
                            policy_fingerprint: verdict.policy_fingerprint.clone(),
                            created_at: crate::history::now_ts(),
                        })
                        .await?;

                    let (position, snapshot) = queue_with_snapshot(
                        &self.snapshots,
                        &session,
                        workspace_id.clone(),
                        id,
                        execution_id.clone(),
                        constrained_request,
                        "session-approved pre-execution checkpoint",
                    )
                    .await?;
                    return Ok(DaemonMessage::ManagedCommandQueued {
                        id,
                        execution_id,
                        position,
                        snapshot,
                    });
                }

                let approval = approval_payload_from_verdict(
                    &execution_id,
                    &request,
                    workspace_id.clone(),
                    &governance_input,
                    &verdict,
                    expires_at,
                );

                self.history
                    .insert_approval_record(&ApprovalRecordRow {
                        approval_id: approval.approval_id.clone(),
                        run_id: governance_input.run_id.clone(),
                        task_id: governance_input.task_id.clone(),
                        goal_run_id: governance_input.goal_run_id.clone(),
                        thread_id: governance_input.thread_id.clone(),
                        transition_kind: transition_kind_str(&governance_input.transition_kind)
                            .to_string(),
                        stage_id: governance_input.stage_id.clone(),
                        scope_summary: verdict
                            .approval_requirement
                            .as_ref()
                            .map(|requirement| requirement.scope_summary.clone())
                            .or_else(|| Some(blast_radius_summary(&governance_input))),
                        target_scope_json: json!({
                            "lane_ids": &governance_input.lane_ids,
                            "target_ids": &governance_input.target_ids,
                        })
                        .to_string(),
                        constraints_json: serde_json::to_string(&constraints)?,
                        risk_class: risk_class_str(&verdict.risk_class).to_string(),
                        rationale_json: serde_json::to_string(&verdict.rationale)?,
                        policy_fingerprint: verdict.policy_fingerprint.clone(),
                        requested_at,
                        resolved_at: None,
                        expires_at,
                        resolution: None,
                        invalidated_at: None,
                        invalidation_reason: None,
                    })
                    .await?;

                self.history
                    .insert_approval_inbox_entry(&crate::history::ApprovalInboxEntry {
                        approval_id: approval.approval_id.clone(),
                        session_id: id.to_string(),
                        workspace_id: workspace_id.clone(),
                        execution_id: execution_id.clone(),
                        request: request.clone(),
                        approval: approval.clone(),
                        policy_fingerprint: verdict.policy_fingerprint.clone(),
                        constraints: constraints.clone(),
                        transition_kind: governance_input.transition_kind.clone(),
                        requested_at,
                        expires_at,
                        gateway_surface: None,
                        gateway_channel: None,
                        gateway_thread: None,
                        rendered_prompt: None,
                    })
                    .await?;

                self.pending_approvals.write().await.insert(
                    approval.approval_id.clone(),
                    PendingApproval {
                        session_id: id,
                        workspace_id,
                        execution_id,
                        request,
                        approval: approval.clone(),
                        policy_fingerprint: verdict.policy_fingerprint.clone(),
                        constraints: constraints.clone(),
                        transition_kind: governance_input.transition_kind.clone(),
                        requested_at,
                        expires_at,
                    },
                );
                Ok(DaemonMessage::ApprovalRequired { id, approval })
            }
            VerdictClass::Defer
            | VerdictClass::Deny
            | VerdictClass::HaltAndIsolate
            | VerdictClass::AllowOnlyWithCompensationPlan => {
                Ok(DaemonMessage::ManagedCommandRejected {
                    id,
                    execution_id: Some(execution_id),
                    message: governance_rejection_message(&verdict),
                })
            }
        }
    }

    pub async fn resolve_approval(
        &self,
        id: SessionId,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<Vec<DaemonMessage>> {
        let pending = self
            .pending_approvals
            .read()
            .await
            .get(approval_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("approval not found: {approval_id}"))?;

        let now = crate::history::now_ts();
        if pending
            .expires_at
            .map(|expires_at| now > expires_at)
            .unwrap_or(false)
        {
            self.pending_approvals.write().await.remove(approval_id);
            let _ = self.history.remove_inbox_entry(approval_id).await;
            self.history
                .invalidate_approval_record(approval_id, "approval expired before resolution", now)
                .await?;
            anyhow::bail!("approval is stale: approval window expired before resolution");
        }

        let mut fresh_input = governance_input_for_managed_command(
            &pending.execution_id,
            &pending.request,
            pending.workspace_id.clone(),
            Some(pending.session_id.to_string()),
        );
        fresh_input.transition_kind = pending.transition_kind.clone();
        let fresh_verdict = evaluate_governance(&fresh_input);
        if fresh_verdict.policy_fingerprint != pending.policy_fingerprint {
            self.pending_approvals.write().await.remove(approval_id);
            let _ = self.history.remove_inbox_entry(approval_id).await;
            self.history
                .invalidate_approval_record(
                    approval_id,
                    "approval invalidated because governance conditions changed",
                    now,
                )
                .await?;
            anyhow::bail!("approval is stale: governance conditions changed since it was issued");
        }

        let pending = self
            .pending_approvals
            .write()
            .await
            .remove(approval_id)
            .ok_or_else(|| anyhow::anyhow!("approval not found: {approval_id}"))?;

        self.history.remove_inbox_entry(approval_id).await?;

        self.history
            .resolve_approval_record(approval_id, approval_resolution_str(decision), now)
            .await?;

        let mut responses = vec![DaemonMessage::ApprovalResolved {
            id,
            approval_id: approval_id.to_string(),
            decision,
        }];

        if matches!(decision, ApprovalDecision::Deny) {
            responses.push(DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: Some(pending.execution_id),
                message: "execution denied by operator".to_string(),
            });
            return Ok(responses);
        }

        let session = self
            .sessions
            .read()
            .await
            .get(&pending.session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", pending.session_id))?;

        let mut request = pending.request.clone();
        if !can_honor_constraints(&pending.constraints, &request) {
            anyhow::bail!(
                "managed command blocked because current runtime can no longer honor governance constraints"
            );
        }
        apply_constraints_to_request(&mut request, &pending.constraints);

        if matches!(decision, ApprovalDecision::ApproveSession) {
            self.store_session_grant(
                pending.session_id,
                approval_id,
                &pending.policy_fingerprint,
                pending.expires_at,
            )
            .await;
        }

        let (position, snapshot) = queue_with_snapshot(
            &self.snapshots,
            &session,
            pending.workspace_id.clone(),
            pending.session_id,
            pending.execution_id.clone(),
            request,
            "approved pre-execution checkpoint",
        )
        .await?;
        responses.push(DaemonMessage::ManagedCommandQueued {
            id,
            execution_id: pending.execution_id,
            position,
            snapshot,
        });
        Ok(responses)
    }

    pub async fn resolve_approval_by_id(
        &self,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<Vec<DaemonMessage>> {
        let session_id = self
            .pending_approvals
            .read()
            .await
            .get(approval_id)
            .map(|pending| pending.session_id)
            .ok_or_else(|| anyhow::anyhow!("approval not found: {approval_id}"))?;

        self.resolve_approval(session_id, approval_id, decision)
            .await
    }

    pub async fn list_pending_approvals(&self) -> Vec<ApprovalPayload> {
        self.pending_approvals
            .read()
            .await
            .values()
            .map(|pending| pending.approval.clone())
            .collect()
    }

    pub async fn hydrate_pending_approvals(&self) -> Result<()> {
        let entries = self.history.list_pending_inbox_entries().await?;
        let mut pending = self.pending_approvals.write().await;
        pending.clear();

        for entry in entries {
            let session_id = match uuid::Uuid::parse_str(&entry.session_id) {
                Ok(session_id) => session_id,
                Err(_) => {
                    self.history.remove_inbox_entry(&entry.approval_id).await?;
                    continue;
                }
            };

            pending.insert(
                entry.approval_id.clone(),
                PendingApproval {
                    session_id,
                    workspace_id: entry.workspace_id,
                    execution_id: entry.execution_id,
                    request: entry.request,
                    approval: entry.approval,
                    policy_fingerprint: entry.policy_fingerprint,
                    constraints: entry.constraints,
                    transition_kind: entry.transition_kind,
                    requested_at: entry.requested_at,
                    expires_at: entry.expires_at,
                },
            );
        }

        Ok(())
    }

    pub fn verify_telemetry_integrity(&self) -> Result<Vec<TelemetryLedgerStatus>> {
        let results = self.history.verify_worm_integrity()?;
        Ok(results
            .into_iter()
            .map(|r| TelemetryLedgerStatus {
                kind: r.kind,
                total_entries: r.total_entries,
                valid: r.valid,
                first_invalid_seq: r.first_invalid_seq,
                message: r.message,
            })
            .collect())
    }

    async fn snapshot_saved_sessions(&self) -> Vec<SavedSession> {
        let sessions: Vec<Arc<Mutex<PtySession>>> =
            self.sessions.read().await.values().cloned().collect();

        let mut saved = Vec::with_capacity(sessions.len());
        for session in sessions {
            let session = session.lock().await;
            saved.push(SavedSession {
                id: session.id().to_string(),
                shell: session.shell().map(ToOwned::to_owned),
                cwd: session.cwd().map(ToOwned::to_owned),
                workspace_id: session.workspace_id().map(ToOwned::to_owned),
                cols: session.cols(),
                rows: session.rows(),
            });
        }

        saved
    }

    async fn persist_state(&self) {
        let previous_sessions = self.snapshot_saved_sessions().await;
        let state = DaemonState { previous_sessions };
        if let Err(error) = save_state(&self.state_path, &state) {
            tracing::error!(error = %error, path = %self.state_path.display(), "failed to persist daemon state");
        }
    }
}
