use super::*;

fn parse_workflow_notice_details(details: Option<&str>) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(details?).ok()
}

pub(super) fn auto_compaction_reload_window(
    details: Option<&str>,
) -> Option<(usize, usize, usize, usize)> {
    let parsed = parse_workflow_notice_details(details)?;
    let split_at = parsed.get("split_at")?.as_u64()? as usize;
    let total_message_count = parsed.get("total_message_count")?.as_u64()? as usize;
    Some((
        total_message_count.saturating_sub(split_at).max(1),
        0,
        split_at,
        total_message_count,
    ))
}

pub(super) struct CompactionTokenSnapshot {
    pub(super) post_compaction_total_tokens: u64,
    pub(super) post_compaction_window_start: usize,
    pub(super) post_compaction_window_end: usize,
}

pub(super) struct CompactionArtifactNotice {
    pub(super) id: String,
    pub(super) content: String,
    pub(super) payload: Option<String>,
    pub(super) strategy: Option<String>,
}

pub(super) fn compaction_artifact_from_notice(
    details: Option<&str>,
) -> Option<CompactionArtifactNotice> {
    let parsed = parse_workflow_notice_details(details)?;
    let content = parsed
        .get("artifact_content")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let id = parsed
        .get("artifact_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("compaction-artifact")
        .to_string();
    let payload = parsed
        .get("artifact_payload")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let strategy = parsed
        .get("artifact_strategy")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    Some(CompactionArtifactNotice {
        id,
        content,
        payload,
        strategy,
    })
}

pub(super) fn compaction_token_snapshot(details: Option<&str>) -> Option<CompactionTokenSnapshot> {
    let parsed = parse_workflow_notice_details(details)?;
    let post_compaction_total_tokens = parsed.get("post_compaction_total_tokens")?.as_u64()?;
    let post_compaction_window_start =
        parsed.get("post_compaction_window_start")?.as_u64()? as usize;
    let post_compaction_window_end = parsed.get("post_compaction_window_end")?.as_u64()? as usize;
    Some(CompactionTokenSnapshot {
        post_compaction_total_tokens,
        post_compaction_window_start,
        post_compaction_window_end,
    })
}

pub(super) fn normalized_skill_workflow_notice(
    kind: &str,
    message: &str,
    details: Option<&str>,
) -> Option<(String, String, Option<String>)> {
    let parsed = parse_workflow_notice_details(details);
    let recommended_skill = parsed
        .as_ref()
        .and_then(|value| value.get("recommended_skill"))
        .and_then(|value| value.as_str());
    let confidence_tier = parsed
        .as_ref()
        .and_then(|value| value.get("confidence_tier"))
        .and_then(|value| value.as_str());
    let recommended_action = parsed
        .as_ref()
        .and_then(|value| value.get("recommended_action"))
        .and_then(|value| value.as_str());
    let skip_rationale = parsed
        .as_ref()
        .and_then(|value| value.get("skip_rationale"))
        .and_then(|value| value.as_str());

    match kind {
        "skill-preflight" => {
            let normalized_kind = if confidence_tier == Some("strong") {
                "skill-discovery-required"
            } else {
                "skill-discovery-recommended"
            };
            let status = [
                if normalized_kind == "skill-discovery-required" {
                    Some("Skill gate required".to_string())
                } else {
                    Some("Skill guidance ready".to_string())
                },
                recommended_skill.map(|value| format!("skill={value}")),
                confidence_tier.map(|value| format!("confidence={value}")),
                recommended_action.map(|value| format!("next={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            let activity = if normalized_kind == "skill-discovery-required" {
                Some("skill gate".to_string())
            } else {
                Some("skill review".to_string())
            };
            Some((normalized_kind.to_string(), status, activity))
        }
        "skill-gate" => {
            let status = [
                Some("Skill gate blocked progress".to_string()),
                recommended_skill.map(|value| format!("skill={value}")),
                recommended_action.map(|value| format!("next={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((
                "skill-discovery-required".to_string(),
                status,
                Some("skill gate".to_string()),
            ))
        }
        "skill-discovery-skipped" => {
            let status = [
                Some("Skill recommendation skipped".to_string()),
                recommended_skill.map(|value| format!("skill={value}")),
                skip_rationale.map(|value| format!("why={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((kind.to_string(), status, None))
        }
        "skill-community-scout" => {
            let candidates = parsed
                .as_ref()
                .and_then(|value| value.get("candidates"))
                .and_then(|value| value.as_array())
                .map(|value| value.len());
            let timeout = parsed
                .as_ref()
                .and_then(|value| value.get("community_preapprove_timeout_secs"))
                .and_then(|value| value.as_u64());
            let status = [
                Some("Community scout update".to_string()),
                candidates.map(|value| format!("candidates={value}")),
                timeout.map(|value| format!("timeout={}s", value)),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((kind.to_string(), status, Some("skill scout".to_string())))
        }
        "skill-discovery-required" | "skill-discovery-recommended" => Some((
            kind.to_string(),
            message.to_string(),
            Some(if kind == "skill-discovery-required" {
                "skill gate".to_string()
            } else {
                "skill review".to_string()
            }),
        )),
        "manual-compaction" | "auto-compaction" => {
            let lower = message.to_ascii_lowercase();
            let still_compacting = !lower.contains("applied")
                && !lower.contains("skipped")
                && !lower.contains("fail")
                && !lower.contains("compacted");
            Some((
                kind.to_string(),
                message.to_string(),
                still_compacting.then(|| "compacting".to_string()),
            ))
        }
        _ => None,
    }
}

#[path = "events_activity_parts/participant_playground_target_to_handle_operator_model_summary_event.rs"]
mod participant_playground_target_to_handle_operator_model_summary_event;

#[path = "events_activity_parts/handle_operator_model_reset_event_to_handle_divergent_session_event.rs"]
mod handle_operator_model_reset_event_to_handle_divergent_session_event;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_compaction_workflow_notice_routes_to_compacting_activity() {
        // Why this matters: render_status_bar ignores status_line entirely,
        // so the only visible feedback channel for compaction is the per-thread
        // agent_activity spinner. Compaction kinds must therefore yield an
        // activity label from this normalizer or the user sees nothing during
        // the daemon round-trip.
        let result = normalized_skill_workflow_notice(
            "manual-compaction",
            "Manual compaction starting...",
            None,
        );
        let (_kind, _status, activity) = result.expect(
            "manual-compaction must produce an agent_activity so the spinner stays visible",
        );
        assert_eq!(activity.as_deref(), Some("compacting"));
    }

    #[test]
    fn applied_compaction_notice_clears_the_compacting_activity() {
        let result = normalized_skill_workflow_notice(
            "manual-compaction",
            "Manual compaction applied using heuristic. Pre-compaction context",
            None,
        );
        let (_kind, _status, activity) =
            result.expect("applied compaction stays a workflow notice");
        assert!(activity.is_none());
    }

    #[test]
    fn auto_compaction_workflow_notice_routes_to_compacting_activity() {
        let result = normalized_skill_workflow_notice(
            "auto-compaction",
            "Auto compaction applied using heuristic.",
            Some("{\"split_at\":20,\"total_message_count\":121}"),
        );
        let (_kind, _status, activity) = result
            .expect("auto-compaction must also surface the spinner; otherwise users see a hang");
        assert_eq!(activity.as_deref(), Some("compacting"));
    }

    #[test]
    fn unrecognized_workflow_notice_returns_none() {
        // Sanity: don't accidentally promote arbitrary kinds to the spinner.
        assert!(normalized_skill_workflow_notice("some-other-kind", "msg", None).is_none());
    }
}

pub(super) fn parse_collaboration_sessions(
    value: serde_json::Value,
) -> Option<Vec<CollaborationSessionVm>> {
    let items = value.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|session| {
                let id = session.get("id")?.as_str()?.to_string();
                let disagreement_values = session
                    .get("disagreements")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let disagreements = session
                    .get("disagreements")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|disagreement| {
                                Some(CollaborationDisagreementVm {
                                    id: disagreement.get("id")?.as_str()?.to_string(),
                                    topic: disagreement
                                        .get("topic")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("disagreement")
                                        .to_string(),
                                    positions: disagreement
                                        .get("positions")
                                        .and_then(serde_json::Value::as_array)
                                        .map(|positions| {
                                            positions
                                                .iter()
                                                .filter_map(|position| {
                                                    position.as_str().map(ToOwned::to_owned)
                                                })
                                                .collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default(),
                                    vote_count: disagreement
                                        .get("votes")
                                        .and_then(serde_json::Value::as_array)
                                        .map(|votes| votes.len())
                                        .unwrap_or(0),
                                    resolution: disagreement
                                        .get("resolution")
                                        .and_then(serde_json::Value::as_str)
                                        .map(ToOwned::to_owned),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let escalation = disagreement_values.iter().find_map(|disagreement| {
                    let resolution = disagreement
                        .get("resolution")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("pending");
                    let confidence_gap = disagreement
                        .get("confidence_gap")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(1.0);
                    if resolution == "escalated"
                        || (resolution == "pending" && confidence_gap < 0.15)
                    {
                        Some(CollaborationEscalationVm {
                            from_level: "L1".to_string(),
                            to_level: if resolution == "escalated" {
                                "L2".to_string()
                            } else {
                                "L1".to_string()
                            },
                            reason: disagreement
                                .get("topic")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("subagent disagreement requires attention")
                                .to_string(),
                            attempts: 1,
                        })
                    } else {
                        None
                    }
                });
                Some(CollaborationSessionVm {
                    id,
                    parent_task_id: session
                        .get("parent_task_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    parent_thread_id: session
                        .get("parent_thread_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    agent_count: session
                        .get("agents")
                        .and_then(serde_json::Value::as_array)
                        .map(|agents| agents.len())
                        .unwrap_or(0),
                    disagreement_count: disagreements.len(),
                    consensus_summary: session
                        .get("consensus")
                        .and_then(|consensus| consensus.get("summary"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    escalation,
                    disagreements,
                })
            })
            .collect(),
    )
}
