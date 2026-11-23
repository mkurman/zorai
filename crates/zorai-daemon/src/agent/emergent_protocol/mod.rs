pub(crate) mod compressor;
pub(crate) mod decoder;
pub(crate) mod pattern_detector;
pub(crate) mod types;

use serde_json::json;
use zorai_protocol::{AgentDbMessage, AgentEventRow};

use crate::agent::engine::AgentEngine;
use crate::history::{EmergentProtocolRow, ProtocolStepRow};

use self::types::{
    ContextSignature, EmergentProtocolStore, ProtocolCandidate, ProtocolCandidateState,
    ProtocolCandidateStore, ProtocolDecodeOutcome, ProtocolDecodeOutcomeKind,
    ProtocolRegistryEntry, ProtocolStep, ProtocolUsageRecord,
};

const ACCEPTANCE_OBSERVATION_THRESHOLD: u32 = 3;
const SUPPRESSION_OBSERVATION_THRESHOLD: u32 = 2;

impl AgentEngine {
    pub(crate) async fn get_thread_protocol_candidate_store(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<ProtocolCandidateStore> {
        Ok(self
            .history
            .get_thread_protocol_candidates_state(thread_id)
            .await?
            .unwrap_or_default())
    }

    pub(crate) async fn analyze_emergent_protocol_for_thread(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Option<EmergentProtocolStore>> {
        let messages = self.history.list_recent_messages(thread_id, 12).await?;
        self.analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
    }

    pub(crate) async fn analyze_emergent_protocol_from_messages(
        &self,
        thread_id: &str,
        messages: &[AgentDbMessage],
    ) -> anyhow::Result<Option<EmergentProtocolStore>> {
        let detected = pattern_detector::detect_protocol_candidates(thread_id, messages);
        if detected.candidates.is_empty() {
            return Ok(None);
        }

        let mut durable_store = self.get_thread_protocol_candidate_store(thread_id).await?;
        merge_detected_candidates(&mut durable_store, detected.candidates.clone());
        durable_store.updated_at_ms = crate::agent::task_prompt::now_millis();
        self.history
            .upsert_thread_protocol_candidates_state(
                thread_id,
                &durable_store,
                durable_store.updated_at_ms,
            )
            .await?;

        let strongest = durable_store.candidates.first().cloned();
        if let Some(candidate) = strongest {
            let row = AgentEventRow {
                id: format!("emergent_protocol_{}", uuid::Uuid::new_v4()),
                category: "emergent_protocol".to_string(),
                kind: "candidate_detected".to_string(),
                pane_id: Some(thread_id.to_string()),
                workspace_id: None,
                surface_id: None,
                session_id: None,
                payload_json: serde_json::to_string(&json!({
                    "thread_id": thread_id,
                    "candidate": candidate,
                }))?,
                timestamp: crate::agent::task_prompt::now_millis() as i64,
            };
            self.history.upsert_agent_event(&row).await?;
        }

        Ok(Some(EmergentProtocolStore {
            candidates: durable_store.candidates.clone(),
        }))
    }

    pub(crate) async fn list_thread_protocol_registry_entries(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Vec<ProtocolRegistryEntry>> {
        let rows = self
            .history
            .list_emergent_protocols_for_thread(thread_id)
            .await?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(self.protocol_registry_entry_from_row(row).await?);
        }
        Ok(entries)
    }

    pub(crate) async fn lookup_thread_protocol_registry_entry(
        &self,
        thread_id: &str,
        token: &str,
    ) -> anyhow::Result<Option<ProtocolRegistryEntry>> {
        let Some(row) = self.history.get_emergent_protocol_by_token(token).await? else {
            return Ok(None);
        };
        if row.thread_id != thread_id {
            return Ok(None);
        }
        Ok(Some(self.protocol_registry_entry_from_row(row).await?))
    }

    pub(crate) async fn list_thread_protocol_proposals(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let store = self.get_thread_protocol_candidate_store(thread_id).await?;
        let proposals = store
            .candidates
            .into_iter()
            .filter(|candidate| candidate.state == ProtocolCandidateState::Proposed)
            .collect::<Vec<_>>();
        Ok(json!({
            "thread_id": thread_id,
            "proposal_count": proposals.len(),
            "proposals": proposals,
        }))
    }

    pub(crate) async fn respond_to_protocol_proposal(
        &self,
        thread_id: &str,
        candidate_id: &str,
        accept: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let mut store = self.get_thread_protocol_candidate_store(thread_id).await?;
        let now = crate::agent::task_prompt::now_millis();

        let Some(index) = store
            .candidates
            .iter()
            .position(|candidate| candidate.id == candidate_id)
        else {
            return Ok(json!({
                "thread_id": thread_id,
                "candidate_id": candidate_id,
                "status": "not_found",
            }));
        };

        if store.candidates[index].state != ProtocolCandidateState::Proposed {
            let candidate = store.candidates[index].clone();
            return Ok(json!({
                "thread_id": thread_id,
                "candidate_id": candidate_id,
                "status": "invalid_state",
                "candidate": candidate,
            }));
        }

        let candidate = if accept {
            store.candidates[index].state = ProtocolCandidateState::Accepted;
            let candidate = store.candidates[index].clone();
            let entry = self
                .activate_protocol_candidate(thread_id, &candidate, now)
                .await?;

            let row = AgentEventRow {
                id: format!("emergent_protocol_activation_{}", uuid::Uuid::new_v4()),
                category: "emergent_protocol".to_string(),
                kind: "protocol_activated".to_string(),
                pane_id: Some(thread_id.to_string()),
                workspace_id: None,
                surface_id: None,
                session_id: None,
                payload_json: serde_json::to_string(&json!({
                    "thread_id": thread_id,
                    "candidate_id": candidate.id,
                    "token": entry.token,
                    "protocol_id": entry.protocol_id,
                }))?,
                timestamp: now as i64,
            };
            self.history.upsert_agent_event(&row).await?;
            candidate
        } else {
            store.candidates[index].state = ProtocolCandidateState::Rejected;
            store.candidates[index].clone()
        };

        store.updated_at_ms = now;
        self.history
            .upsert_thread_protocol_candidates_state(thread_id, &store, store.updated_at_ms)
            .await?;

        let row = AgentEventRow {
            id: format!(
                "emergent_protocol_proposal_response_{}",
                uuid::Uuid::new_v4()
            ),
            category: "emergent_protocol".to_string(),
            kind: if accept {
                "proposal_accepted".to_string()
            } else {
                "proposal_rejected".to_string()
            },
            pane_id: Some(thread_id.to_string()),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            payload_json: serde_json::to_string(&json!({
                "thread_id": thread_id,
                "candidate": candidate,
            }))?,
            timestamp: now as i64,
        };
        self.history.upsert_agent_event(&row).await?;

        let entry = if accept {
            if let Some(row) = self
                .history
                .get_emergent_protocol_by_pattern(thread_id, &candidate.normalized_pattern)
                .await?
            {
                Some(self.protocol_registry_entry_from_row(row).await?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(json!({
            "thread_id": thread_id,
            "candidate_id": candidate_id,
            "status": if accept { "accepted" } else { "rejected" },
            "candidate": candidate,
            "entry": entry,
        }))
    }

    pub(crate) async fn reload_thread_protocol_registry(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let entries = self
            .list_thread_protocol_registry_entries(thread_id)
            .await?;
        Ok(json!({
            "thread_id": thread_id,
            "protocol_count": entries.len(),
            "protocols": entries,
        }))
    }

    pub(crate) async fn record_protocol_registry_usage(
        &self,
        thread_id: &str,
        token: &str,
        success: bool,
        fallback_reason: Option<String>,
        execution_time_ms: Option<u64>,
    ) -> anyhow::Result<Option<ProtocolRegistryEntry>> {
        let Some(mut entry) = self
            .lookup_thread_protocol_registry_entry(thread_id, token)
            .await?
        else {
            return Ok(None);
        };
        let used_at_ms = crate::agent::task_prompt::now_millis();
        let usage = ProtocolUsageRecord {
            id: format!("proto_usage_{}", uuid::Uuid::new_v4()),
            protocol_id: entry.protocol_id.clone(),
            used_at_ms,
            execution_time_ms,
            success,
            fallback_reason: fallback_reason.clone(),
        };

        self.history
            .insert_protocol_usage_log(&crate::history::ProtocolUsageLogRow {
                id: usage.id.clone(),
                protocol_id: usage.protocol_id.clone(),
                used_at: usage.used_at_ms,
                execution_time_ms: usage.execution_time_ms,
                success: usage.success,
                fallback_reason: usage.fallback_reason.clone(),
            })
            .await?;

        let previous_total = entry.usage_count as f64;
        let previous_successes = entry.success_rate * previous_total;
        let next_total = previous_total + 1.0;
        let next_successes = previous_successes + if success { 1.0 } else { 0.0 };
        entry.usage_count += 1;
        entry.last_used_ms = Some(used_at_ms);
        entry.success_rate = if next_total > 0.0 {
            (next_successes / next_total).clamp(0.0, 1.0)
        } else {
            entry.success_rate
        };

        self.history
            .update_emergent_protocol_usage_stats(
                &entry.protocol_id,
                used_at_ms,
                entry.usage_count as u64,
                entry.success_rate,
            )
            .await?;

        let row = AgentEventRow {
            id: format!("emergent_protocol_usage_{}", uuid::Uuid::new_v4()),
            category: "emergent_protocol".to_string(),
            kind: if success {
                "protocol_usage_recorded".to_string()
            } else {
                "protocol_fallback_recorded".to_string()
            },
            pane_id: Some(thread_id.to_string()),
            workspace_id: None,
            surface_id: None,
            session_id: None,
            payload_json: serde_json::to_string(&json!({
                "thread_id": thread_id,
                "protocol_id": entry.protocol_id,
                "token": entry.token,
                "success": success,
                "fallback_reason": fallback_reason,
                "usage_count": entry.usage_count,
                "success_rate": entry.success_rate,
            }))?,
            timestamp: used_at_ms as i64,
        };
        self.history.upsert_agent_event(&row).await?;

        Ok(Some(entry))
    }

    pub(crate) async fn decode_thread_protocol_token(
        &self,
        thread_id: &str,
        token: &str,
        current_role: Option<&str>,
        expected_target_role: Option<&str>,
        normalized_pattern: Option<&str>,
    ) -> anyhow::Result<Option<ProtocolDecodeOutcome>> {
        let Some(entry) = self
            .lookup_thread_protocol_registry_entry(thread_id, token)
            .await?
        else {
            return Ok(None);
        };

        let current_role = current_role
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("user");
        let target_role = expected_target_role
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| infer_counterparty_role(current_role));
        let normalized_pattern = normalized_pattern
            .map(crate::agent::emergent_protocol::compressor::compress_pattern_key)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| entry.context_signature.normalized_pattern.clone());
        let observed_signal_kind =
            crate::agent::emergent_protocol::decoder::classify_pattern_key(&normalized_pattern);

        let role_matches = current_role == entry.context_signature.source_role;
        let target_role_matches = target_role == entry.context_signature.target_role;
        let pattern_matches = normalized_pattern == entry.context_signature.normalized_pattern;
        let signal_kind_matches = observed_signal_kind
            .map(|kind| kind == entry.context_signature.signal_kind)
            .unwrap_or(false);
        let context_match =
            role_matches && target_role_matches && pattern_matches && signal_kind_matches;

        if !context_match {
            let mut mismatches = Vec::new();
            if !role_matches {
                mismatches.push(format!(
                    "role_mismatch:{}!= {}",
                    current_role, entry.context_signature.source_role
                ));
            }
            if !target_role_matches {
                mismatches.push(format!(
                    "target_role_mismatch:{}!= {}",
                    target_role, entry.context_signature.target_role
                ));
            }
            if !pattern_matches {
                mismatches.push(format!(
                    "pattern_mismatch:{}!= {}",
                    normalized_pattern, entry.context_signature.normalized_pattern
                ));
            }
            if !signal_kind_matches {
                mismatches.push(format!(
                    "signal_kind_mismatch:{:?}!={:?}",
                    observed_signal_kind, entry.context_signature.signal_kind
                ));
            }
            let fallback_reason = mismatches.join(";");
            let updated = self
                .record_protocol_registry_usage(
                    thread_id,
                    token,
                    false,
                    Some(fallback_reason.clone()),
                    None,
                )
                .await?
                .unwrap_or(entry.clone());
            return Ok(Some(ProtocolDecodeOutcome {
                outcome: ProtocolDecodeOutcomeKind::Fallback,
                token: token.to_string(),
                protocol_id: updated.protocol_id.clone(),
                thread_id: thread_id.to_string(),
                context_match: false,
                fallback_reason: Some(fallback_reason),
                entry: updated,
                expanded_steps: Vec::new(),
            }));
        }

        let updated = self
            .record_protocol_registry_usage(thread_id, token, true, None, None)
            .await?
            .unwrap_or(entry.clone());
        Ok(Some(ProtocolDecodeOutcome {
            outcome: ProtocolDecodeOutcomeKind::Match,
            token: token.to_string(),
            protocol_id: updated.protocol_id.clone(),
            thread_id: thread_id.to_string(),
            context_match: true,
            fallback_reason: None,
            expanded_steps: updated.steps.clone(),
            entry: updated,
        }))
    }

    pub(crate) async fn build_protocol_prompt_context(
        &self,
        thread_id: &str,
        raw_content: &str,
    ) -> anyhow::Result<Option<String>> {
        let token = raw_content.trim();
        if token.is_empty() || token.contains(char::is_whitespace) {
            return Ok(None);
        }

        let Some(decoded) = self
            .decode_thread_protocol_token(thread_id, token, Some("user"), Some("assistant"), None)
            .await?
        else {
            return Ok(None);
        };
        if decoded.outcome != ProtocolDecodeOutcomeKind::Match {
            return Ok(None);
        }

        let mut lines = vec![format!(
            "- Decoded emergent protocol token `{}` into a reusable coordination intent.",
            token
        )];
        for step in decoded.expanded_steps.iter().take(3) {
            let tool = step
                .tool
                .as_deref()
                .map(|tool| format!(" via `{tool}`"))
                .unwrap_or_default();
            lines.push(format!(
                "- Step {}: {}{}",
                step.step_index + 1,
                step.intent,
                tool
            ));
        }
        Ok(Some(format!(
            "## Emergent Protocol Decode\n{}\n",
            lines.join("\n")
        )))
    }

    async fn protocol_registry_entry_from_row(
        &self,
        row: EmergentProtocolRow,
    ) -> anyhow::Result<ProtocolRegistryEntry> {
        let context_signature: ContextSignature =
            serde_json::from_str(&row.context_signature_json)?;
        let steps = self
            .history
            .list_protocol_steps(&row.protocol_id)
            .await?
            .into_iter()
            .map(|step| ProtocolStep {
                step_index: step.step_index as u32,
                intent: step.intent,
                tool: step.tool_name,
                args_template: serde_json::from_str(&step.args_template_json)
                    .unwrap_or_else(|_| serde_json::json!({})),
            })
            .collect();
        Ok(ProtocolRegistryEntry {
            protocol_id: row.protocol_id,
            token: row.token,
            description: row.description,
            agent_a: row.agent_a,
            agent_b: row.agent_b,
            thread_id: row.thread_id,
            normalized_pattern: row.normalized_pattern,
            trigger_phrase: context_signature.trigger_phrase.clone(),
            signal_kind: crate::agent::emergent_protocol::types::ProtocolSignalKind::from_str(
                &row.signal_kind,
            )
            .unwrap_or(
                crate::agent::emergent_protocol::types::ProtocolSignalKind::RepeatedShorthand,
            ),
            context_signature,
            steps,
            created_at_ms: row.created_at,
            activated_at_ms: row.activated_at,
            last_used_ms: row.last_used_at,
            usage_count: row.usage_count as u32,
            success_rate: row.success_rate,
            source_candidate_id: row.source_candidate_id,
        })
    }

    async fn activate_protocol_candidate(
        &self,
        thread_id: &str,
        candidate: &ProtocolCandidate,
        now: u64,
    ) -> anyhow::Result<ProtocolRegistryEntry> {
        if let Some(row) = self
            .history
            .get_emergent_protocol_by_pattern(thread_id, &candidate.normalized_pattern)
            .await?
        {
            return self.protocol_registry_entry_from_row(row).await;
        }

        let token = crate::agent::emergent_protocol::compressor::stable_protocol_token(
            &candidate.normalized_pattern,
            thread_id,
        );
        let context_signature = build_context_signature(thread_id, candidate);
        let entry = ProtocolRegistryEntry {
            protocol_id: format!("proto_reg_{}", uuid::Uuid::new_v4()),
            token: token.clone(),
            description: format!(
                "Accepted emergent protocol for '{}' in thread {}",
                candidate.normalized_pattern, thread_id
            ),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: candidate.normalized_pattern.clone(),
            trigger_phrase: candidate.trigger_phrase.clone(),
            signal_kind: candidate.kind,
            context_signature,
            steps: vec![ProtocolStep {
                step_index: 0,
                intent: format!(
                    "expand '{}' as recurring coordination cue '{}'",
                    token, candidate.trigger_phrase
                ),
                tool: None,
                args_template: serde_json::json!({
                    "normalized_pattern": candidate.normalized_pattern,
                    "trigger_phrase": candidate.trigger_phrase,
                }),
            }],
            created_at_ms: candidate.first_seen_at_ms,
            activated_at_ms: now,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: Some(candidate.id.clone()),
        };
        self.persist_protocol_registry_entry(&entry).await?;
        Ok(entry)
    }

    async fn persist_protocol_registry_entry(
        &self,
        entry: &ProtocolRegistryEntry,
    ) -> anyhow::Result<()> {
        let row = EmergentProtocolRow {
            protocol_id: entry.protocol_id.clone(),
            token: entry.token.clone(),
            description: entry.description.clone(),
            agent_a: entry.agent_a.clone(),
            agent_b: entry.agent_b.clone(),
            thread_id: entry.thread_id.clone(),
            normalized_pattern: entry.normalized_pattern.clone(),
            signal_kind: entry.signal_kind.as_str().to_string(),
            context_signature_json: serde_json::to_string(&entry.context_signature)?,
            created_at: entry.created_at_ms,
            activated_at: entry.activated_at_ms,
            last_used_at: entry.last_used_ms,
            usage_count: entry.usage_count as u64,
            success_rate: entry.success_rate,
            source_candidate_id: entry.source_candidate_id.clone(),
        };
        self.history.upsert_emergent_protocol(&row).await?;
        let steps = entry
            .steps
            .iter()
            .map(|step| ProtocolStepRow {
                protocol_id: entry.protocol_id.clone(),
                step_index: step.step_index as u64,
                intent: step.intent.clone(),
                tool_name: step.tool.clone(),
                args_template_json: serde_json::to_string(&step.args_template)
                    .unwrap_or_else(|_| "{}".to_string()),
            })
            .collect::<Vec<_>>();
        self.history
            .replace_protocol_steps(&entry.protocol_id, &steps)
            .await?;
        Ok(())
    }

    pub(crate) async fn get_protocol_usage_log_payload(
        &self,
        protocol_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let usage = self.history.list_protocol_usage_log(protocol_id).await?;
        let protocol_row = self
            .history
            .get_emergent_protocol_by_id(protocol_id)
            .await?;
        let protocol_summary = protocol_row.map(|row| {
            json!({
                "protocol_id": row.protocol_id,
                "token": row.token,
                "description": row.description,
                "thread_id": row.thread_id,
                "normalized_pattern": row.normalized_pattern,
                "signal_kind": row.signal_kind,
                "usage_count": row.usage_count,
                "success_rate": row.success_rate,
                "last_used_at": row.last_used_at,
            })
        });
        let entries = usage
            .into_iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "protocol_id": entry.protocol_id,
                    "used_at": entry.used_at,
                    "execution_time_ms": entry.execution_time_ms,
                    "success": entry.success,
                    "fallback_reason": entry.fallback_reason,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "protocol_id": protocol_id,
            "protocol": protocol_summary,
            "entries": entries,
        }))
    }
}

fn build_context_signature(thread_id: &str, candidate: &ProtocolCandidate) -> ContextSignature {
    let source_role = candidate
        .observations
        .last()
        .map(|observation| observation.role.clone())
        .unwrap_or_else(|| "user".to_string());
    let target_role = infer_counterparty_role(&source_role);
    ContextSignature {
        thread_id: thread_id.to_string(),
        normalized_pattern: candidate.normalized_pattern.clone(),
        trigger_phrase: candidate.trigger_phrase.clone(),
        signal_kind: candidate.kind,
        source_role,
        target_role,
    }
}

fn infer_counterparty_role(source_role: &str) -> String {
    if source_role == "assistant" {
        "user".to_string()
    } else {
        "assistant".to_string()
    }
}

fn merge_detected_candidates(
    store: &mut ProtocolCandidateStore,
    detected_candidates: Vec<ProtocolCandidate>,
) {
    for detected in detected_candidates {
        if let Some(existing) = store
            .candidates
            .iter_mut()
            .find(|candidate| candidate.normalized_pattern == detected.normalized_pattern)
        {
            existing.trigger_phrase = detected.trigger_phrase.clone();
            existing.kind = detected.kind;
            existing.confidence = existing.confidence.max(detected.confidence);
            existing.observation_count = existing.observation_count.max(detected.observation_count);
            existing.first_seen_at_ms = existing.first_seen_at_ms.min(detected.first_seen_at_ms);
            existing.last_seen_at_ms = existing.last_seen_at_ms.max(detected.last_seen_at_ms);
            existing.observations = detected.observations.clone();
            existing.state = match existing.state {
                ProtocolCandidateState::Accepted => ProtocolCandidateState::Accepted,
                ProtocolCandidateState::Rejected
                    if detected.observation_count >= ACCEPTANCE_OBSERVATION_THRESHOLD
                        && detected.confidence >= 0.20 =>
                {
                    classify_candidate_state(existing.observation_count, existing.confidence)
                }
                ProtocolCandidateState::Rejected => ProtocolCandidateState::Rejected,
                _ => classify_candidate_state(existing.observation_count, existing.confidence),
            };
        } else {
            let mut candidate = detected;
            candidate.state =
                classify_candidate_state(candidate.observation_count, candidate.confidence);
            store.candidates.push(candidate);
        }
    }

    store.candidates.sort_by(|a, b| {
        rank_state(b.state)
            .cmp(&rank_state(a.state))
            .then_with(|| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| b.observation_count.cmp(&a.observation_count))
    });
}

fn classify_candidate_state(observation_count: u32, confidence: f64) -> ProtocolCandidateState {
    if observation_count >= ACCEPTANCE_OBSERVATION_THRESHOLD && confidence >= 0.20 {
        ProtocolCandidateState::Proposed
    } else if observation_count < SUPPRESSION_OBSERVATION_THRESHOLD && confidence < 0.20 {
        ProtocolCandidateState::Rejected
    } else {
        ProtocolCandidateState::Candidate
    }
}

fn rank_state(state: ProtocolCandidateState) -> u8 {
    match state {
        ProtocolCandidateState::Accepted => 5,
        ProtocolCandidateState::Proposed => 4,
        ProtocolCandidateState::Candidate => 3,
        ProtocolCandidateState::Observing => 2,
        ProtocolCandidateState::Rejected => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ToolCall, ToolFunction};
    use crate::agent::{types::AgentConfig, AgentEngine};
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    fn msg(
        id: &str,
        thread_id: &str,
        created_at: i64,
        role: &str,
        content: &str,
    ) -> AgentDbMessage {
        AgentDbMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: id.to_string(),
            thread_id: thread_id.to_string(),
            created_at,
            role: role.to_string(),
            content: content.to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        }
    }

    fn msg_with_tool_calls(
        id: &str,
        thread_id: &str,
        created_at: i64,
        role: &str,
        content: &str,
        tool_names: &[&str],
    ) -> AgentDbMessage {
        let tool_calls = tool_names
            .iter()
            .enumerate()
            .map(|(index, name)| ToolCall {
                id: format!("call_{id}_{index}"),
                function: ToolFunction {
                    name: (*name).to_string(),
                    arguments: "{}".to_string(),
                },
                weles_review: None,
            })
            .collect::<Vec<_>>();

        AgentDbMessage {
            reasoning_tokens: None,
            thinking_ms: None,
            id: id.to_string(),
            thread_id: thread_id.to_string(),
            created_at,
            role: role.to_string(),
            content: content.to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: Some(
                serde_json::to_string(&tool_calls).expect("tool calls should serialize"),
            ),
            metadata_json: None,
        }
    }

    async fn accept_proposal(
        engine: &AgentEngine,
        thread_id: &str,
        normalized_pattern: &str,
    ) -> serde_json::Value {
        let store = engine
            .get_thread_protocol_candidate_store(thread_id)
            .await
            .expect("candidate store should load");
        let candidate_id = store
            .candidates
            .iter()
            .find(|candidate| {
                candidate.state == ProtocolCandidateState::Proposed
                    && candidate.normalized_pattern == normalized_pattern
            })
            .map(|candidate| candidate.id.clone())
            .expect("proposed candidate should exist");

        engine
            .respond_to_protocol_proposal(thread_id, &candidate_id, true)
            .await
            .expect("proposal acceptance should succeed")
    }

    #[tokio::test]
    async fn candidate_gets_proposed_after_sufficient_observation() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-accepted";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");

        assert!(analyzed.candidates.iter().any(|candidate| candidate.state
            == ProtocolCandidateState::Proposed
            && candidate.normalized_pattern == "continue"));

        let stored = engine
            .get_thread_protocol_candidate_store(thread_id)
            .await
            .expect("stored protocol candidates should load");
        assert!(stored.candidates.iter().any(|candidate| candidate.state
            == ProtocolCandidateState::Proposed
            && candidate.normalized_pattern == "continue"));
    }

    #[tokio::test]
    async fn repeated_multi_step_tool_sequence_becomes_proposed_protocol_candidate() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-tool-sequence";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "please inspect and patch"),
            msg_with_tool_calls(
                "m2",
                thread_id,
                2,
                "assistant",
                "running tool sequence",
                &[
                    zorai_protocol::tool_names::SEARCH_FILES,
                    zorai_protocol::tool_names::READ_FILE,
                    zorai_protocol::tool_names::APPLY_PATCH,
                ],
            ),
            msg("m3", thread_id, 3, "user", "again"),
            msg_with_tool_calls(
                "m4",
                thread_id,
                4,
                "assistant",
                "running tool sequence",
                &[
                    zorai_protocol::tool_names::SEARCH_FILES,
                    zorai_protocol::tool_names::READ_FILE,
                    zorai_protocol::tool_names::APPLY_PATCH,
                ],
            ),
            msg("m5", thread_id, 5, "user", "one more time"),
            msg_with_tool_calls(
                "m6",
                thread_id,
                6,
                "assistant",
                "running tool sequence",
                &[
                    zorai_protocol::tool_names::SEARCH_FILES,
                    zorai_protocol::tool_names::READ_FILE,
                    zorai_protocol::tool_names::APPLY_PATCH,
                ],
            ),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");

        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "search_files -> read_file -> apply_patch"
        }));

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn weak_candidate_gets_suppressed_before_acceptance() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-suppressed";

        let weak = ProtocolCandidate {
            id: "proto-weak".to_string(),
            thread_id: thread_id.to_string(),
            kind: crate::agent::emergent_protocol::types::ProtocolSignalKind::RepeatedShorthand,
            trigger_phrase: "k".to_string(),
            normalized_pattern: "k".to_string(),
            state: ProtocolCandidateState::Candidate,
            confidence: 0.05,
            observation_count: 1,
            first_seen_at_ms: 1,
            last_seen_at_ms: 1,
            observations: vec![],
        };

        let mut store = ProtocolCandidateStore::default();
        merge_detected_candidates(&mut store, vec![weak]);

        assert!(store.candidates.iter().any(|candidate| candidate.state
            == ProtocolCandidateState::Rejected
            && candidate.normalized_pattern == "k"));

        engine
            .history
            .upsert_thread_protocol_candidates_state(thread_id, &store, 10)
            .await
            .expect("suppressed store should persist");

        let stored = engine
            .get_thread_protocol_candidate_store(thread_id)
            .await
            .expect("suppressed protocol candidates should load");
        assert!(stored.candidates.iter().any(|candidate| candidate.state
            == ProtocolCandidateState::Rejected
            && candidate.normalized_pattern == "k"));
    }
    #[tokio::test]
    async fn rejected_candidate_can_reenter_proposed_state_after_stronger_future_evidence() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-reproposal";

        let weak = ProtocolCandidate {
            id: "proto_rejected_then_reobserved".to_string(),
            thread_id: thread_id.to_string(),
            kind: types::ProtocolSignalKind::RepeatedContinuationCue,
            trigger_phrase: "continue".to_string(),
            normalized_pattern: "continue".to_string(),
            state: ProtocolCandidateState::Rejected,
            confidence: 0.05,
            observation_count: 1,
            first_seen_at_ms: 1,
            last_seen_at_ms: 1,
            observations: vec![],
        };

        let mut store = ProtocolCandidateStore::default();
        store.candidates.push(weak);
        engine
            .history
            .upsert_thread_protocol_candidates_state(thread_id, &store, 10)
            .await
            .expect("initial rejected store should persist");

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");

        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.normalized_pattern == "continue"
                && candidate.state == ProtocolCandidateState::Proposed
        }));
    }

    #[tokio::test]
    async fn accepted_proposal_promotes_into_registry_entry() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-registry";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");
        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        }));

        let pre_registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        assert!(pre_registry.is_empty());

        let response = accept_proposal(&engine, thread_id, "continue").await;
        assert_eq!(response["status"].as_str(), Some("accepted"));

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");

        assert_eq!(registry.len(), 1);
        let entry = &registry[0];
        assert_eq!(entry.thread_id, thread_id);
        assert_eq!(entry.normalized_pattern, "continue");
        assert!(entry.token.starts_with("@proto_"));
        assert!(entry
            .source_candidate_id
            .as_deref()
            .is_some_and(|value| value.starts_with("proto_")));
    }
    #[tokio::test]
    async fn registry_lookup_can_record_usage_and_fallback() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-usage";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");
        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        }));
        let response = accept_proposal(&engine, thread_id, "continue").await;
        assert_eq!(response["status"].as_str(), Some("accepted"));

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        let token = registry[0].token.clone();
        let protocol_id = registry[0].protocol_id.clone();

        let looked_up = engine
            .lookup_thread_protocol_registry_entry(thread_id, &token)
            .await
            .expect("lookup should succeed")
            .expect("entry should exist");
        assert_eq!(looked_up.protocol_id, protocol_id);
        assert_eq!(looked_up.usage_count, 0);

        let updated = engine
            .record_protocol_registry_usage(
                thread_id,
                &token,
                false,
                Some("context_mismatch".to_string()),
                Some(7),
            )
            .await
            .expect("usage recording should succeed")
            .expect("entry should still exist");
        assert_eq!(updated.usage_count, 1);
        assert_eq!(updated.success_rate, 0.0);

        let usage_payload = engine
            .get_protocol_usage_log_payload(&protocol_id)
            .await
            .expect("usage payload should load");
        assert_eq!(
            usage_payload["protocol"]["token"].as_str(),
            Some(token.as_str())
        );
        assert_eq!(
            usage_payload["protocol"]["protocol_id"].as_str(),
            Some(protocol_id.as_str())
        );
        assert!(usage_payload["protocol"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("Accepted emergent protocol")));
        assert_eq!(usage_payload["protocol"]["usage_count"].as_u64(), Some(1));
        assert_eq!(
            usage_payload["protocol"]["success_rate"].as_f64(),
            Some(0.0)
        );
        let entries = usage_payload["entries"]
            .as_array()
            .expect("usage entries should be an array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["success"].as_bool(), Some(false));
        assert_eq!(
            entries[0]["fallback_reason"].as_str(),
            Some("context_mismatch")
        );

        let reloaded = engine
            .reload_thread_protocol_registry(thread_id)
            .await
            .expect("reload should succeed");
        assert_eq!(reloaded["protocol_count"].as_u64(), Some(1));
    }
    #[tokio::test]
    async fn decode_returns_expanded_steps_on_context_match() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-match";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");
        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        }));
        accept_proposal(&engine, thread_id, "continue").await;

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        let token = registry[0].token.clone();

        let decoded = engine
            .decode_thread_protocol_token(thread_id, &token, Some("user"), None, Some("continue"))
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Match);
        assert_eq!(decoded.expanded_steps.len(), 1);
        assert!(decoded.expanded_steps[0].intent.contains("expand"));
    }
    #[tokio::test]
    async fn decode_returns_structured_fallback_on_context_mismatch() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-fallback";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");
        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        }));
        accept_proposal(&engine, thread_id, "continue").await;

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        let token = registry[0].token.clone();
        let protocol_id = registry[0].protocol_id.clone();

        let decoded = engine
            .decode_thread_protocol_token(
                thread_id,
                &token,
                Some("assistant"),
                None,
                Some("different"),
            )
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(!decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Fallback);
        assert!(decoded
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("mismatch")));

        let usage_payload = engine
            .get_protocol_usage_log_payload(&protocol_id)
            .await
            .expect("usage payload should load");
        let entries = usage_payload["entries"]
            .as_array()
            .expect("usage entries should be an array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["success"].as_bool(), Some(false));
    }

    #[tokio::test]
    async fn rejected_proposal_does_not_activate_registry_entry() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-rejected-proposal";

        let messages = vec![
            msg("m1", thread_id, 1, "user", "continue"),
            msg("m2", thread_id, 2, "assistant", "working"),
            msg("m3", thread_id, 3, "user", "continue"),
            msg("m4", thread_id, 4, "assistant", "still working"),
            msg("m5", thread_id, 5, "user", "continue"),
        ];

        let analyzed = engine
            .analyze_emergent_protocol_from_messages(thread_id, &messages)
            .await
            .expect("analysis should succeed")
            .expect("candidate store should be returned");
        assert!(analyzed.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Proposed
                && candidate.normalized_pattern == "continue"
        }));

        let store = engine
            .get_thread_protocol_candidate_store(thread_id)
            .await
            .expect("candidate store should load");
        let candidate_id = store
            .candidates
            .iter()
            .find(|candidate| {
                candidate.state == ProtocolCandidateState::Proposed
                    && candidate.normalized_pattern == "continue"
            })
            .map(|candidate| candidate.id.clone())
            .expect("proposed candidate should exist");

        let response = engine
            .respond_to_protocol_proposal(thread_id, &candidate_id, false)
            .await
            .expect("proposal rejection should succeed");
        assert_eq!(response["status"].as_str(), Some("rejected"));
        assert_eq!(response["entry"], serde_json::Value::Null);

        let registry = engine
            .list_thread_protocol_registry_entries(thread_id)
            .await
            .expect("registry should load");
        assert!(registry.is_empty());

        let stored = engine
            .get_thread_protocol_candidate_store(thread_id)
            .await
            .expect("candidate store should load");
        assert!(stored.candidates.iter().any(|candidate| {
            candidate.state == ProtocolCandidateState::Rejected
                && candidate.normalized_pattern == "continue"
        }));
    }

    #[tokio::test]
    async fn decode_returns_fallback_on_signal_kind_mismatch_even_when_pattern_is_present() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-signal-kind-fallback";

        let entry = ProtocolRegistryEntry {
            protocol_id: "proto_manual_signal_kind".to_string(),
            token: "@proto_signalkind".to_string(),
            description: "Manual protocol for signal-kind validation".to_string(),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: "continue".to_string(),
            trigger_phrase: "continue".to_string(),
            signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
            context_signature: types::ContextSignature {
                thread_id: thread_id.to_string(),
                normalized_pattern: "continue".to_string(),
                trigger_phrase: "continue".to_string(),
                signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
                source_role: "user".to_string(),
                target_role: "assistant".to_string(),
            },
            steps: vec![types::ProtocolStep {
                step_index: 0,
                intent: "expand continue cue".to_string(),
                tool: None,
                args_template: serde_json::json!({}),
            }],
            created_at_ms: 1,
            activated_at_ms: 1,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: None,
        };

        engine
            .persist_protocol_registry_entry(&entry)
            .await
            .expect("entry should persist");

        let decoded = engine
            .decode_thread_protocol_token(
                thread_id,
                "@proto_signalkind",
                Some("user"),
                None,
                Some("ok"),
            )
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(!decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Fallback);
        assert!(decoded
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("signal_kind_mismatch")));
    }

    #[tokio::test]
    async fn decode_matches_when_signal_kind_and_pattern_both_match() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-signal-kind-match";

        let entry = ProtocolRegistryEntry {
            protocol_id: "proto_manual_signal_kind_match".to_string(),
            token: "@proto_signalkind_match".to_string(),
            description: "Manual protocol for signal-kind match validation".to_string(),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: "ok".to_string(),
            trigger_phrase: "ok".to_string(),
            signal_kind: types::ProtocolSignalKind::RepeatedAffirmation,
            context_signature: types::ContextSignature {
                thread_id: thread_id.to_string(),
                normalized_pattern: "ok".to_string(),
                trigger_phrase: "ok".to_string(),
                signal_kind: types::ProtocolSignalKind::RepeatedAffirmation,
                source_role: "user".to_string(),
                target_role: "assistant".to_string(),
            },
            steps: vec![types::ProtocolStep {
                step_index: 0,
                intent: "expand ok affirmation".to_string(),
                tool: None,
                args_template: serde_json::json!({}),
            }],
            created_at_ms: 1,
            activated_at_ms: 1,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: None,
        };

        engine
            .persist_protocol_registry_entry(&entry)
            .await
            .expect("entry should persist");

        let decoded = engine
            .decode_thread_protocol_token(
                thread_id,
                "@proto_signalkind_match",
                Some("user"),
                None,
                Some("ok"),
            )
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Match);
        assert_eq!(decoded.expanded_steps.len(), 1);
    }

    #[tokio::test]
    async fn decode_returns_fallback_on_target_role_mismatch_even_when_other_context_matches() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-target-role-fallback";

        let entry = ProtocolRegistryEntry {
            protocol_id: "proto_manual_target_role".to_string(),
            token: "@proto_targetrole".to_string(),
            description: "Manual protocol for target-role validation".to_string(),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: "continue".to_string(),
            trigger_phrase: "continue".to_string(),
            signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
            context_signature: types::ContextSignature {
                thread_id: thread_id.to_string(),
                normalized_pattern: "continue".to_string(),
                trigger_phrase: "continue".to_string(),
                signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
                source_role: "user".to_string(),
                target_role: "assistant".to_string(),
            },
            steps: vec![types::ProtocolStep {
                step_index: 0,
                intent: "expand continue cue".to_string(),
                tool: None,
                args_template: serde_json::json!({}),
            }],
            created_at_ms: 1,
            activated_at_ms: 1,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: None,
        };

        engine
            .persist_protocol_registry_entry(&entry)
            .await
            .expect("entry should persist");

        let decoded = engine
            .decode_thread_protocol_token(
                thread_id,
                "@proto_targetrole",
                Some("user"),
                Some("user"),
                Some("continue"),
            )
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(!decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Fallback);
        assert!(decoded
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("target_role_mismatch")));
    }

    #[tokio::test]
    async fn decode_matches_when_target_role_matches_context_signature() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-decode-target-role-match";

        let entry = ProtocolRegistryEntry {
            protocol_id: "proto_manual_target_role_match".to_string(),
            token: "@proto_targetrole_match".to_string(),
            description: "Manual protocol for target-role match validation".to_string(),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: "continue".to_string(),
            trigger_phrase: "continue".to_string(),
            signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
            context_signature: types::ContextSignature {
                thread_id: thread_id.to_string(),
                normalized_pattern: "continue".to_string(),
                trigger_phrase: "continue".to_string(),
                signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
                source_role: "user".to_string(),
                target_role: "assistant".to_string(),
            },
            steps: vec![types::ProtocolStep {
                step_index: 0,
                intent: "expand continue cue".to_string(),
                tool: None,
                args_template: serde_json::json!({}),
            }],
            created_at_ms: 1,
            activated_at_ms: 1,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: None,
        };

        engine
            .persist_protocol_registry_entry(&entry)
            .await
            .expect("entry should persist");

        let decoded = engine
            .decode_thread_protocol_token(
                thread_id,
                "@proto_targetrole_match",
                Some("user"),
                Some("assistant"),
                Some("continue"),
            )
            .await
            .expect("decode should succeed")
            .expect("entry should decode");

        assert!(decoded.context_match);
        assert_eq!(decoded.outcome, types::ProtocolDecodeOutcomeKind::Match);
        assert_eq!(decoded.expanded_steps.len(), 1);
    }

    #[tokio::test]
    async fn build_protocol_prompt_context_surfaces_decoded_steps_for_live_turns() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-emergent-prompt-context";

        let entry = ProtocolRegistryEntry {
            protocol_id: "proto_manual_prompt_context".to_string(),
            token: "@proto_prompt".to_string(),
            description: "Manual protocol for prompt context".to_string(),
            agent_a: "user".to_string(),
            agent_b: "assistant".to_string(),
            thread_id: thread_id.to_string(),
            normalized_pattern: "continue".to_string(),
            trigger_phrase: "continue".to_string(),
            signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
            context_signature: types::ContextSignature {
                thread_id: thread_id.to_string(),
                normalized_pattern: "continue".to_string(),
                trigger_phrase: "continue".to_string(),
                signal_kind: types::ProtocolSignalKind::RepeatedContinuationCue,
                source_role: "user".to_string(),
                target_role: "assistant".to_string(),
            },
            steps: vec![types::ProtocolStep {
                step_index: 0,
                intent: "expand continue cue".to_string(),
                tool: None,
                args_template: serde_json::json!({}),
            }],
            created_at_ms: 1,
            activated_at_ms: 1,
            last_used_ms: None,
            usage_count: 0,
            success_rate: 1.0,
            source_candidate_id: None,
        };

        engine
            .persist_protocol_registry_entry(&entry)
            .await
            .expect("entry should persist");

        let context = engine
            .build_protocol_prompt_context(thread_id, "@proto_prompt")
            .await
            .expect("prompt context should build")
            .expect("prompt context should exist");

        assert!(context.contains("Emergent Protocol Decode"));
        assert!(context.contains("expand continue cue"));
    }
}
