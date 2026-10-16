use super::*;
use serde::{Deserialize, Serialize};

const MAX_COMPATIBLE_TOOL_COUNT: usize = 128;
const OBSERVED_PROVIDER_SIGNAL_TTL_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderAvailabilitySignal {
    InvalidCredentials,
    ExpiredSession,
    QuotaExhausted,
    BalanceExhausted,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KnownProviderAvailabilitySignal {
    signal: ProviderAvailabilitySignal,
    expires_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderPreflightReason {
    InvalidCredentials,
    ExpiredSession,
    ModelUnavailable,
    QuotaExhausted,
    BalanceExhausted,
    CircuitOpen,
    ToolLimitExceeded,
    SchemaUnsupported,
    ProviderConfigurationInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderPreflightDecision {
    Eligible,
    Rerouted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProviderCandidateRejection {
    pub provider: String,
    pub model: String,
    pub reasons: Vec<ProviderPreflightReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProviderRoutingRationale {
    pub version: u8,
    pub decision: ProviderPreflightDecision,
    pub requested_provider: String,
    pub requested_model: String,
    pub selected_provider: Option<String>,
    pub selected_model: Option<String>,
    pub reserved_fallback_provider: Option<String>,
    pub reserved_fallback_model: Option<String>,
    pub rejected_candidates: Vec<ProviderCandidateRejection>,
    pub checked_at: u64,
}

impl ProviderRoutingRationale {
    fn rejected_reason_codes(&self) -> String {
        self.rejected_candidates
            .iter()
            .flat_map(|candidate| candidate.reasons.iter())
            .map(|reason| reason.as_str().to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl ProviderPreflightReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::ExpiredSession => "expired_session",
            Self::ModelUnavailable => "model_unavailable",
            Self::QuotaExhausted => "quota_exhausted",
            Self::BalanceExhausted => "balance_exhausted",
            Self::CircuitOpen => "circuit_open",
            Self::ToolLimitExceeded => "tool_limit_exceeded",
            Self::SchemaUnsupported => "schema_unsupported",
            Self::ProviderConfigurationInvalid => "provider_configuration_invalid",
        }
    }
}

#[derive(Debug, Clone)]
struct PreflightRequirements {
    tool_count: usize,
    requires_schema: bool,
    transport_override: Option<ApiTransport>,
}

fn task_requirements(task: &AgentTask) -> PreflightRequirements {
    PreflightRequirements {
        tool_count: task
            .tool_whitelist
            .as_ref()
            .map(Vec::len)
            .or_else(|| {
                task.containment_scope
                    .as_deref()
                    .and_then(|value| value.strip_prefix("provider_preflight_tool_count:"))
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(0),
        requires_schema: task.containment_scope.as_deref()
            == Some("provider_preflight_requires_schema"),
        transport_override: task.override_api_transport,
    }
}

fn configured_model(config: &AgentConfig, provider: &str) -> String {
    if provider == config.provider && !config.model.trim().is_empty() {
        return config.model.trim().to_string();
    }
    config
        .providers
        .get(provider)
        .map(|candidate| candidate.model.trim())
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| {
            get_provider_definition(provider).map(|definition| definition.default_model.to_string())
        })
        .unwrap_or_default()
}

pub(crate) fn availability_signal_from_observed_failure(
    message: &str,
) -> Option<ProviderAvailabilitySignal> {
    let structured = crate::agent::llm_client::parse_structured_upstream_failure(message)?;
    let lower = format!("{}\n{}", structured.summary, structured.diagnostics).to_ascii_lowercase();
    if crate::agent::llm_client::mentions_provider_credit_exhaustion(&lower) {
        return Some(ProviderAvailabilitySignal::BalanceExhausted);
    }
    if crate::agent::llm_client::mentions_provider_quota_exhaustion(&lower) {
        return Some(ProviderAvailabilitySignal::QuotaExhausted);
    }
    if lower.contains("expired")
        && (lower.contains("oauth") || lower.contains("session") || lower.contains("token"))
    {
        return Some(ProviderAvailabilitySignal::ExpiredSession);
    }
    if lower.contains("invalid api key")
        || lower.contains("invalid credentials")
        || lower.contains("unauthorized")
    {
        return Some(ProviderAvailabilitySignal::InvalidCredentials);
    }
    None
}

fn model_is_known_compatible(provider: &str, model: &str) -> bool {
    if model.trim().is_empty() {
        return false;
    }
    if provider_allows_unlisted_models(provider) {
        return true;
    }
    get_provider_definition(provider).is_some_and(|definition| {
        definition.models.is_empty() || definition.models.iter().any(|entry| entry.id == model)
    })
}

impl AgentEngine {
    async fn preflight_candidate(
        &self,
        provider: &str,
        model: &str,
        requirements: &PreflightRequirements,
    ) -> Vec<ProviderPreflightReason> {
        let mut reasons = Vec::new();
        let config = self.config.read().await.clone();
        let resolved = resolve_provider_config_for(&config, provider, Some(model));
        let Ok(resolved) = resolved else {
            reasons.push(ProviderPreflightReason::ProviderConfigurationInvalid);
            return reasons;
        };

        let auth_state = self
            .get_provider_auth_states()
            .await
            .into_iter()
            .find(|state| state.provider_id == provider);
        if !auth_state.as_ref().is_some_and(|state| state.authenticated) {
            reasons.push(match resolved.auth_source {
                AuthSource::ChatgptSubscription | AuthSource::GithubCopilot => {
                    ProviderPreflightReason::ExpiredSession
                }
                AuthSource::ApiKey => ProviderPreflightReason::InvalidCredentials,
            });
        }

        if !model_is_known_compatible(provider, model) {
            reasons.push(ProviderPreflightReason::ModelUnavailable);
        }
        let exact_transport = requirements
            .transport_override
            .unwrap_or(resolved.api_transport);
        if !provider_supports_transport(provider, exact_transport) {
            reasons.push(ProviderPreflightReason::SchemaUnsupported);
        }
        if requirements.requires_schema && exact_transport == ApiTransport::NativeAssistant {
            reasons.push(ProviderPreflightReason::SchemaUnsupported);
        }
        if requirements.tool_count > MAX_COMPATIBLE_TOOL_COUNT {
            reasons.push(ProviderPreflightReason::ToolLimitExceeded);
        }

        if let Some(known) = self
            .provider_availability_signals
            .read()
            .await
            .get(provider)
            .copied()
        {
            if known.expires_at > now_millis() {
                reasons.push(match known.signal {
                    ProviderAvailabilitySignal::InvalidCredentials => {
                        ProviderPreflightReason::InvalidCredentials
                    }
                    ProviderAvailabilitySignal::ExpiredSession => {
                        ProviderPreflightReason::ExpiredSession
                    }
                    ProviderAvailabilitySignal::QuotaExhausted => {
                        ProviderPreflightReason::QuotaExhausted
                    }
                    ProviderAvailabilitySignal::BalanceExhausted => {
                        ProviderPreflightReason::BalanceExhausted
                    }
                });
            }
        }

        let breaker = self.circuit_breakers.get(provider).await;
        if !breaker.lock().await.can_execute(now_millis()) {
            reasons.push(ProviderPreflightReason::CircuitOpen);
        }
        reasons.sort();
        reasons.dedup();
        reasons
    }

    pub(crate) async fn provider_assignment_preflight(
        &self,
        task: &AgentTask,
    ) -> ProviderRoutingRationale {
        let config = self.config.read().await.clone();
        let requested_provider = task
            .override_provider
            .clone()
            .unwrap_or_else(|| config.provider.clone());
        let requested_model = task
            .override_model
            .clone()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| configured_model(&config, &requested_provider));
        let requirements = task_requirements(task);
        let requested_reasons = self
            .preflight_candidate(&requested_provider, &requested_model, &requirements)
            .await;

        let mut configured_candidates = config.providers.keys().cloned().collect::<Vec<_>>();
        if !configured_candidates
            .iter()
            .any(|candidate| candidate == &config.provider)
        {
            configured_candidates.push(config.provider.clone());
        }
        configured_candidates.sort();
        configured_candidates.dedup();
        configured_candidates.retain(|candidate| candidate != &requested_provider);

        let mut rejected_candidates = Vec::new();
        if !requested_reasons.is_empty() {
            rejected_candidates.push(ProviderCandidateRejection {
                provider: requested_provider.clone(),
                model: requested_model.clone(),
                reasons: requested_reasons,
            });
        }

        let mut compatible_fallbacks = Vec::new();
        for provider in configured_candidates {
            let model = configured_model(&config, &provider);
            let reasons = self
                .preflight_candidate(&provider, &model, &requirements)
                .await;
            if reasons.is_empty() {
                compatible_fallbacks.push((provider, model));
            } else if !rejected_candidates.is_empty() {
                rejected_candidates.push(ProviderCandidateRejection {
                    provider,
                    model,
                    reasons,
                });
            }
        }

        let requested_eligible = rejected_candidates
            .first()
            .is_none_or(|candidate| candidate.provider != requested_provider);
        let selected = if requested_eligible {
            Some((requested_provider.clone(), requested_model.clone()))
        } else {
            compatible_fallbacks.first().cloned()
        };
        let reserved_fallback = if requested_eligible {
            compatible_fallbacks.first().cloned()
        } else {
            compatible_fallbacks.get(1).cloned()
        };
        let decision = match (&selected, requested_eligible) {
            (Some(_), true) => ProviderPreflightDecision::Eligible,
            (Some(_), false) => ProviderPreflightDecision::Rerouted,
            (None, _) => ProviderPreflightDecision::Rejected,
        };

        ProviderRoutingRationale {
            version: 1,
            decision,
            requested_provider,
            requested_model,
            selected_provider: selected.as_ref().map(|value| value.0.clone()),
            selected_model: selected.as_ref().map(|value| value.1.clone()),
            reserved_fallback_provider: reserved_fallback.as_ref().map(|value| value.0.clone()),
            reserved_fallback_model: reserved_fallback.as_ref().map(|value| value.1.clone()),
            rejected_candidates,
            checked_at: now_millis(),
        }
    }

    pub(crate) async fn apply_provider_preflight_to_ready_tasks(&self) -> bool {
        let candidates = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .filter(|task| {
                    task.status == TaskStatus::Queued
                        || (task.status == TaskStatus::Blocked
                            && task.error.as_deref() == Some("provider_preflight_rejected"))
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut decisions = Vec::with_capacity(candidates.len());
        for task in candidates {
            decisions.push((
                task.id.clone(),
                self.provider_assignment_preflight(&task).await,
            ));
        }
        if decisions.is_empty() {
            return false;
        }

        let mut changed = false;
        let mut tasks = self.tasks.lock().await;
        for (task_id, rationale) in decisions {
            let Some(task) = tasks.iter_mut().find(|task| {
                task.id == task_id
                    && (task.status == TaskStatus::Queued
                        || (task.status == TaskStatus::Blocked
                            && task.error.as_deref() == Some("provider_preflight_rejected")))
            }) else {
                continue;
            };
            let details = serde_json::to_string(&rationale).expect("routing rationale serializes");
            task.logs
                .retain(|entry| entry.phase != "provider_preflight");
            match rationale.decision {
                ProviderPreflightDecision::Eligible | ProviderPreflightDecision::Rerouted => {
                    task.status = TaskStatus::Queued;
                    task.blocked_reason = None;
                    task.error = None;
                    task.override_provider = rationale.selected_provider.clone();
                    task.override_model = rationale.selected_model.clone();
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "provider_preflight",
                        "provider assignment preflight passed",
                        Some(details),
                    ));
                }
                ProviderPreflightDecision::Rejected => {
                    task.status = TaskStatus::Blocked;
                    task.progress = task.progress.min(99);
                    task.blocked_reason = Some(format!(
                        "no_compatible_provider:{}",
                        rationale.rejected_reason_codes()
                    ));
                    task.error = Some("provider_preflight_rejected".to_string());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Error,
                        "provider_preflight",
                        "provider assignment preflight rejected all configured candidates",
                        Some(details),
                    ));
                }
            }
            changed = true;
        }
        changed
    }

    #[cfg(test)]
    pub(crate) async fn set_provider_availability_signal_for_test(
        &self,
        provider: &str,
        signal: ProviderAvailabilitySignal,
    ) {
        self.record_provider_availability_signal(provider, signal)
            .await;
    }

    pub(crate) async fn record_provider_availability_signal(
        &self,
        provider: &str,
        signal: ProviderAvailabilitySignal,
    ) {
        self.provider_availability_signals.write().await.insert(
            provider.to_string(),
            KnownProviderAvailabilitySignal {
                signal,
                expires_at: now_millis().saturating_add(OBSERVED_PROVIDER_SIGNAL_TTL_MS),
            },
        );
    }
}
