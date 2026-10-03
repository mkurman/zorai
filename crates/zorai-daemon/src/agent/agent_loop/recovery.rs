use super::*;
use crate::agent::llm_client::{
    is_exhausted_provider_usage_limit, parse_structured_upstream_failure, StructuredUpstreamFailure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixableUpstreamRecoveryAction {
    InvestigateOnly,
    RepairThreadStateAndRetry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FixableUpstreamRecovery {
    signature: String,
    action: FixableUpstreamRecoveryAction,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(in crate::agent) struct FixableUpstreamRecoveryDisposition {
    pub(in crate::agent) started_investigation: bool,
    pub(in crate::agent) retry_attempted: bool,
    pub(in crate::agent) signature: Option<String>,
}

impl AgentEngine {
    pub(in crate::agent) async fn maybe_recover_fixable_upstream_failure(
        &self,
        thread_id: &str,
        structured: &StructuredUpstreamFailure,
        assistant_output_visible: bool,
        tool_side_effect_committed: bool,
        attempted_recovery_signatures: &mut std::collections::HashSet<String>,
    ) -> Result<FixableUpstreamRecoveryDisposition> {
        let Some(recovery) = classify_fixable_upstream_recovery(structured) else {
            return Ok(FixableUpstreamRecoveryDisposition::default());
        };

        let started_investigation = self
            .concierge
            .maybe_start_recovery_investigation(
                self,
                thread_id,
                &recovery.signature,
                &structured.class,
                &structured.summary,
                &structured.diagnostics,
            )
            .await
            .is_some();

        let retry_allowed = matches!(
            recovery.action,
            FixableUpstreamRecoveryAction::RepairThreadStateAndRetry
        ) && !assistant_output_visible
            && !tool_side_effect_committed
            && attempted_recovery_signatures.insert(recovery.signature.clone());

        if retry_allowed {
            self.emit_workflow_notice(
                thread_id,
                "concierge-recovery",
                "Detected a daemon-fixable request issue. Starting background investigation and retrying with repaired thread state.",
                Some(
                    serde_json::json!({
                        "class": structured.class,
                        "signature": recovery.signature,
                        "retry": true,
                        "investigation_started": started_investigation,
                    })
                    .to_string(),
                ),
            );
            self.persist_upstream_recovery_causal_trace(
                thread_id,
                structured,
                &recovery.signature,
                started_investigation,
                true,
            )
            .await;
            self.repair_tool_call_sequence(thread_id).await;
            self.clear_thread_continuation_state(thread_id).await;
            return Ok(FixableUpstreamRecoveryDisposition {
                started_investigation,
                retry_attempted: true,
                signature: Some(recovery.signature),
            });
        }

        if started_investigation {
            let reason = if assistant_output_visible || tool_side_effect_committed {
                "Automatic retry is skipped because this turn already committed visible output or tool side effects."
            } else {
                "Automatic retry is skipped until the background investigation finishes."
            };
            self.emit_workflow_notice(
                thread_id,
                "concierge-recovery",
                format!(
                    "Started background investigation for a daemon-side request issue. {reason}"
                ),
                Some(
                    serde_json::json!({
                        "class": structured.class,
                        "signature": recovery.signature,
                        "retry": false,
                        "investigation_started": true,
                    })
                    .to_string(),
                ),
            );
            self.persist_upstream_recovery_causal_trace(
                thread_id,
                structured,
                &recovery.signature,
                true,
                false,
            )
            .await;
        }

        Ok(FixableUpstreamRecoveryDisposition {
            started_investigation,
            retry_attempted: false,
            signature: Some(recovery.signature),
        })
    }
}

pub(super) fn retry_failure_class_from_message(message: &str) -> &'static str {
    if let Some(structured) = parse_structured_upstream_failure(message) {
        return match structured.class.as_str() {
            "rate_limit" => "rate_limit",
            "temporary_upstream" => "upstream",
            "transient_transport" => "transport",
            _ => "upstream",
        };
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests")
    {
        "rate_limit"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("connection")
        || lower.contains("error sending request for url")
        || lower.contains("invalid http version parsed")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("reset")
    {
        "transport"
    } else {
        "upstream"
    }
}

pub(super) fn is_transient_retry_message(message: &str) -> bool {
    if is_exhausted_provider_usage_limit(message) {
        return false;
    }
    if let Some(structured) = parse_structured_upstream_failure(message) {
        return matches!(
            structured.class.as_str(),
            "rate_limit" | "temporary_upstream" | "transient_transport"
        );
    }
    let lower = message.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("error sending request for url")
        || lower.contains("invalid http version parsed")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("overloaded")
        || lower.contains("unavailable")
        || lower.contains("try again later")
}

fn classify_fixable_upstream_recovery(
    structured: &StructuredUpstreamFailure,
) -> Option<FixableUpstreamRecovery> {
    let combined = format!(
        "{}\n{}",
        structured.summary.to_ascii_lowercase(),
        structured.diagnostics.to_string().to_ascii_lowercase()
    );
    let stale_continuation_like = combined.contains("previous_response_id")
        || combined.contains("previous_interaction_id")
        || combined.contains("upstream_thread_id")
        || combined.contains("stale thread")
        || combined.contains("message stack")
        || (combined.contains("no tool call found") && combined.contains("function call output"));

    match structured.class.as_str() {
        "request_invalid"
            if combined.contains("empty string")
                && combined.contains("input[")
                && combined.contains(".name") =>
        {
            Some(FixableUpstreamRecovery {
                signature: "request-invalid-empty-tool-name".to_string(),
                action: FixableUpstreamRecoveryAction::RepairThreadStateAndRetry,
            })
        }
        "request_invalid" if stale_continuation_like => Some(FixableUpstreamRecovery {
            signature: "request-invalid-stale-continuation".to_string(),
            action: FixableUpstreamRecoveryAction::RepairThreadStateAndRetry,
        }),
        "transport_incompatible"
            if stale_continuation_like
                || combined.contains("request body")
                || combined.contains("payload mismatch") =>
        {
            Some(FixableUpstreamRecovery {
                signature: "transport-incompatible-request-shape".to_string(),
                action: FixableUpstreamRecoveryAction::RepairThreadStateAndRetry,
            })
        }
        "transport_incompatible"
            if combined.contains("empty string") && combined.contains("tool") =>
        {
            Some(FixableUpstreamRecovery {
                signature: "transport-incompatible-empty-tool-name".to_string(),
                action: FixableUpstreamRecoveryAction::InvestigateOnly,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_transient_retry_message, retry_failure_class_from_message};

    #[test]
    fn transient_retry_message_recognizes_reqwest_send_request_failures() {
        let message = "minimax-coding-plan transport error: error sending request for url (https://api.minimax.io/anthropic/v1/messages): client error (SendRequest): invalid HTTP version parsed";

        assert!(is_transient_retry_message(message));
        assert_eq!(retry_failure_class_from_message(message), "transport");
    }

    #[test]
    fn exhausted_usage_limit_is_not_a_transient_retry() {
        let message = "openai API returned 429: The usage limit has been reached\n\n[zorai-upstream-diagnostics]{\"class\":\"rate_limit\",\"summary\":\"openai API returned 429: The usage limit has been reached\",\"diagnostics\":{}}";
        assert!(!is_transient_retry_message(message));
        assert!(is_transient_retry_message(
            "openai API returned 429: Too Many Requests\n\n[zorai-upstream-diagnostics]{\"class\":\"rate_limit\",\"summary\":\"429\",\"diagnostics\":{}}"
        ));
    }
}
