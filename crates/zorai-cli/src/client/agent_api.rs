use anyhow::Result;
use futures::StreamExt;
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use zorai_protocol::{ClientMessage, DaemonMessage, OperationStatusSnapshot};

use super::connection::{connect, roundtrip, roundtrip_until};
use futures::SinkExt;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentThreadMessageRecord {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentThreadRecord {
    pub id: String,
    pub agent_name: Option<String>,
    pub title: String,
    pub messages: Vec<AgentThreadMessageRecord>,
    pub pinned: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunStepRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub position: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub success_criteria: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunEventRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub todo_snapshot: Vec<serde_json::Value>,
}

fn deserialize_goal_binding<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Object(map) if map.len() == 1 => {
            let (kind, payload) = map.into_iter().next().expect("validated length");
            let payload = payload
                .as_str()
                .ok_or_else(|| D::Error::custom("goal binding payload must be a string"))?;
            Ok(format!("{kind}:{payload}"))
        }
        other => Err(D::Error::custom(format!(
            "unsupported goal binding payload: {other}"
        ))),
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalEvidenceRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub captured_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalProofCheckRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub resolved_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunReportRecord {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<AgentGoalEvidenceRecord>,
    #[serde(default)]
    pub proof_checks: Vec<AgentGoalProofCheckRecord>,
    #[serde(default)]
    pub generated_at: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunModelUsageRecord {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub request_count: u64,
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalResumeDecisionRecord {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub reason_code: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub details: Vec<String>,
    #[serde(default)]
    pub decided_at: Option<u64>,
    #[serde(default)]
    pub projection_state: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalDeliveryUnitRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, deserialize_with = "deserialize_goal_binding")]
    pub execution_binding: String,
    #[serde(default, deserialize_with = "deserialize_goal_binding")]
    pub verification_binding: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub proof_checks: Vec<AgentGoalProofCheckRecord>,
    #[serde(default)]
    pub evidence: Vec<AgentGoalEvidenceRecord>,
    #[serde(default)]
    pub report: Option<AgentGoalRunReportRecord>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunDossierRecord {
    #[serde(default)]
    pub units: Vec<AgentGoalDeliveryUnitRecord>,
    #[serde(default)]
    pub projection_state: String,
    #[serde(default)]
    pub latest_resume_decision: Option<AgentGoalResumeDecisionRecord>,
    #[serde(default)]
    pub report: Option<AgentGoalRunReportRecord>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub projection_error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentGoalRunRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub client_request_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub current_step_index: usize,
    #[serde(default)]
    pub current_step_title: Option<String>,
    #[serde(default)]
    pub current_step_kind: Option<String>,
    #[serde(default)]
    pub replan_count: u32,
    #[serde(default)]
    pub max_replans: u32,
    #[serde(default)]
    pub plan_summary: Option<String>,
    #[serde(default)]
    pub reflection_summary: Option<String>,
    #[serde(default)]
    pub memory_updates: Vec<String>,
    #[serde(default)]
    pub generated_skill_path: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub failure_cause: Option<String>,
    #[serde(default)]
    pub stopped_reason: Option<String>,
    #[serde(default)]
    pub child_task_ids: Vec<String>,
    #[serde(default)]
    pub child_task_count: u32,
    #[serde(default)]
    pub approval_count: u32,
    #[serde(default)]
    pub awaiting_approval_id: Option<String>,
    #[serde(default)]
    pub policy_fingerprint: Option<String>,
    #[serde(default)]
    pub approval_expires_at: Option<u64>,
    #[serde(default)]
    pub containment_scope: Option<String>,
    #[serde(default)]
    pub compensation_status: Option<String>,
    #[serde(default)]
    pub compensation_summary: Option<String>,
    #[serde(default)]
    pub active_task_id: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub steps: Vec<AgentGoalRunStepRecord>,
    #[serde(default)]
    pub events: Vec<AgentGoalRunEventRecord>,
    #[serde(default)]
    pub dossier: Option<AgentGoalRunDossierRecord>,
    #[serde(default)]
    pub total_prompt_tokens: u64,
    #[serde(default)]
    pub total_completion_tokens: u64,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub model_usage: Vec<AgentGoalRunModelUsageRecord>,
    #[serde(default)]
    pub autonomy_level: String,
    #[serde(default)]
    pub loaded_step_start: usize,
    #[serde(default)]
    pub loaded_step_end: usize,
    #[serde(default)]
    pub total_step_count: usize,
    #[serde(default)]
    pub loaded_event_start: usize,
    #[serde(default)]
    pub loaded_event_end: usize,
    #[serde(default)]
    pub total_event_count: usize,
}

/// Status response fields from the daemon.
pub struct AgentStatusSnapshot {
    pub tier: String,
    pub activity: String,
    pub active_thread_id: Option<String>,
    #[allow(dead_code)]
    pub active_goal_run_id: Option<String>,
    pub active_goal_run_title: Option<String>,
    pub provider_health_json: String,
    pub gateway_statuses_json: String,
    pub recent_actions_json: String,
}

pub struct DirectMessageResponse {
    pub target: String,
    pub thread_id: String,
    pub response: String,
    pub session_id: Option<String>,
    pub provider_final_result_json: Option<String>,
}

pub struct DeleteThreadResponse {
    pub thread_id: String,
    pub deleted: bool,
}

pub struct ThreadControlResponse {
    pub thread_id: String,
    pub action: String,
    pub ok: bool,
}

pub struct GoalControlResponse {
    pub goal_run_id: String,
    pub action: String,
    pub ok: bool,
}

pub struct DeleteGoalRunResponse {
    pub goal_run_id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptInspectionSection {
    pub id: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptToolInspection {
    pub name: String,
    pub serialized: String,
    pub serialized_chars: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptInspection {
    pub agent_id: String,
    pub agent_name: String,
    pub provider_id: String,
    pub model: String,
    pub sections: Vec<AgentPromptInspectionSection>,
    pub final_prompt: String,
    #[serde(default)]
    pub tool_count: Option<usize>,
    #[serde(default)]
    pub tools_serialized_chars: Option<usize>,
    #[serde(default)]
    pub tools: Vec<AgentPromptToolInspection>,
}

pub async fn send_direct_message(
    target: &str,
    thread_id: Option<String>,
    content: String,
    session_id: Option<String>,
) -> Result<DirectMessageResponse> {
    match roundtrip(ClientMessage::AgentDirectMessage {
        target: target.to_string(),
        thread_id,
        content,
        session_id,
    })
    .await?
    {
        DaemonMessage::AgentDirectMessageResponse {
            target,
            thread_id,
            response,
            session_id,
            provider_final_result_json,
        } => Ok(DirectMessageResponse {
            target,
            thread_id,
            response,
            session_id,
            provider_final_result_json,
        }),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_status_query() -> Result<AgentStatusSnapshot> {
    match roundtrip(ClientMessage::AgentStatusQuery).await? {
        DaemonMessage::AgentStatusResponse {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            ..
        } => Ok(AgentStatusSnapshot {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
        }),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_thread_list_query(limit: usize, offset: usize) -> Result<Vec<AgentThreadRecord>> {
    match roundtrip(ClientMessage::AgentListThreads {
        limit: Some(limit),
        offset: Some(offset),
        include_internal: false,
        agent_filter: None,
    })
    .await?
    {
        DaemonMessage::AgentThreadList { threads_json } => Ok(serde_json::from_str(&threads_json)?),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_thread_get_query(thread_id: String) -> Result<Option<AgentThreadRecord>> {
    let mut framed = connect().await?;
    let requested_thread_id = thread_id.clone();
    framed
        .send(ClientMessage::AgentGetThread {
            thread_id,
            message_limit: None,
            message_offset: None,
            collapse_tool_calls: false,
        })
        .await?;

    let mut thread_detail_bytes = Vec::new();
    loop {
        let response = framed
            .next()
            .await
            .ok_or_else(super::connection::closed_connection_error)??;
        match response {
            DaemonMessage::AgentThreadDetail { thread_json } => {
                return Ok(serde_json::from_str(&thread_json)?);
            }
            DaemonMessage::AgentThreadDetailChunk {
                thread_id,
                thread_json_chunk,
                done,
            } => {
                if thread_id != requested_thread_id {
                    thread_detail_bytes.clear();
                    anyhow::bail!(
                        "received chunk for unexpected thread: expected {}, got {}",
                        requested_thread_id,
                        thread_id
                    );
                }
                thread_detail_bytes.extend(thread_json_chunk);
                if done {
                    let thread_json = String::from_utf8(thread_detail_bytes)?;
                    return Ok(serde_json::from_str(&thread_json)?);
                }
            }
            DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
            other => anyhow::bail!("unexpected response: {other:?}"),
        }
    }
}

pub async fn send_thread_delete(thread_id: String) -> Result<DeleteThreadResponse> {
    let mut framed = connect().await?;
    framed
        .send(ClientMessage::AgentDeleteThread {
            thread_id: thread_id.clone(),
        })
        .await?;
    let resp = futures::StreamExt::next(&mut framed)
        .await
        .ok_or_else(super::connection::closed_connection_error)??;
    match resp {
        DaemonMessage::AgentThreadDeleted { thread_id, deleted } => {
            Ok(DeleteThreadResponse { thread_id, deleted })
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_thread_control(thread_id: String, action: &str) -> Result<ThreadControlResponse> {
    let request = match action {
        "stop" => ClientMessage::AgentStopStream {
            thread_id: thread_id.clone(),
        },
        "resume" => ClientMessage::AgentRetryStreamNow {
            thread_id: thread_id.clone(),
        },
        other => anyhow::bail!("unsupported thread action: {other}"),
    };

    match roundtrip(request).await? {
        DaemonMessage::AgentThreadControlled {
            thread_id,
            action,
            ok,
        } => Ok(ThreadControlResponse {
            thread_id,
            action,
            ok,
        }),
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_goal_list_query(limit: usize, offset: usize) -> Result<Vec<AgentGoalRunRecord>> {
    match roundtrip(ClientMessage::AgentListGoalRuns {
        limit: Some(limit),
        offset: Some(offset),
    })
    .await?
    {
        DaemonMessage::AgentGoalRunList { goal_runs_json } => {
            Ok(serde_json::from_str(&goal_runs_json)?)
        }
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_goal_get_query(goal_run_id: String) -> Result<Option<AgentGoalRunRecord>> {
    match roundtrip(ClientMessage::AgentGetGoalRun { goal_run_id }).await? {
        DaemonMessage::AgentGoalRunDetail { goal_run_json } => {
            Ok(serde_json::from_str(&goal_run_json)?)
        }
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_goal_control(goal_run_id: String, action: &str) -> Result<GoalControlResponse> {
    send_goal_control_with_step(goal_run_id, action, None, None).await
}

pub async fn send_goal_control_with_step(
    goal_run_id: String,
    action: &str,
    step_index: Option<usize>,
    payload_json: Option<String>,
) -> Result<GoalControlResponse> {
    let daemon_action = match action {
        "stop" => "stop",
        "resume" => "resume",
        "pause" => "pause",
        "cancel" => "cancel",
        "accept" => "accept",
        "soft_reject" => "soft_reject",
        "hard_reject" => "hard_reject",
        other => anyhow::bail!("unsupported goal action: {other}"),
    };

    match roundtrip(ClientMessage::AgentControlGoalRun {
        goal_run_id: goal_run_id.clone(),
        action: daemon_action.to_string(),
        step_index,
        payload_json,
    })
    .await?
    {
        DaemonMessage::AgentGoalRunControlled { goal_run_id, ok } => Ok(GoalControlResponse {
            goal_run_id,
            action: action.to_string(),
            ok,
        }),
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_goal_delete(goal_run_id: String) -> Result<DeleteGoalRunResponse> {
    match roundtrip(ClientMessage::AgentDeleteGoalRun {
        goal_run_id: goal_run_id.clone(),
    })
    .await?
    {
        DaemonMessage::AgentGoalRunDeleted {
            goal_run_id,
            deleted,
        } => Ok(DeleteGoalRunResponse {
            goal_run_id,
            deleted,
        }),
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_prompt_query(agent_id: Option<String>) -> Result<AgentPromptInspection> {
    match roundtrip(ClientMessage::AgentInspectPrompt {
        agent_id,
        client_surface: Some(zorai_protocol::ClientSurface::Tui),
    })
    .await?
    {
        DaemonMessage::AgentPromptInspection { prompt_json } => {
            Ok(serde_json::from_str(&prompt_json)?)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_operation_status_query(operation_id: String) -> Result<OperationStatusSnapshot> {
    parse_operation_status_response(
        roundtrip(ClientMessage::AgentGetOperationStatus { operation_id }).await?,
    )
}

pub async fn send_text_to_speech(args: serde_json::Value) -> Result<String> {
    parse_speech_response(
        roundtrip(ClientMessage::AgentTextToSpeech {
            args_json: args.to_string(),
        })
        .await?,
    )
}

pub async fn send_speech_to_text(args: serde_json::Value) -> Result<String> {
    parse_speech_response(
        roundtrip(ClientMessage::AgentSpeechToText {
            args_json: args.to_string(),
        })
        .await?,
    )
}

pub async fn send_generate_image(args: serde_json::Value) -> Result<String> {
    parse_speech_response(
        roundtrip(ClientMessage::AgentGenerateImage {
            args_json: args.to_string(),
        })
        .await?,
    )
}

pub async fn send_generate_video(args: serde_json::Value) -> Result<String> {
    parse_speech_response(
        roundtrip(ClientMessage::AgentGenerateVideo {
            args_json: args.to_string(),
        })
        .await?,
    )
}

pub async fn send_analyze_image(args: serde_json::Value) -> Result<String> {
    parse_speech_response(
        roundtrip(ClientMessage::AgentAnalyzeImage {
            args_json: args.to_string(),
        })
        .await?,
    )
}

pub(crate) fn parse_speech_response(msg: DaemonMessage) -> Result<String> {
    match msg {
        DaemonMessage::AgentTextToSpeechResult { content }
        | DaemonMessage::AgentSpeechToTextResult { content }
        | DaemonMessage::AgentGenerateImageResult { content }
        | DaemonMessage::AgentGenerateVideoResult { content }
        | DaemonMessage::AgentAnalyzeImageResult { content } => Ok(content),
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_get() -> Result<serde_json::Value> {
    match roundtrip(ClientMessage::AgentGetConfig).await? {
        DaemonMessage::AgentConfigResponse { config_json } => {
            let config: serde_json::Value = serde_json::from_str(&config_json)?;
            Ok(config)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_set(key_path: String, value_json: String) -> Result<()> {
    parse_config_set_response(
        roundtrip(ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        })
        .await?,
    )
}

pub async fn send_external_runtime_migration_status() -> Result<serde_json::Value> {
    send_external_runtime_migration_query(ClientMessage::AgentExternalRuntimeMigrationStatus).await
}

pub async fn send_external_runtime_migration_preview(
    runtime: String,
    config_path: Option<String>,
) -> Result<serde_json::Value> {
    send_external_runtime_migration_query(ClientMessage::AgentExternalRuntimeMigrationPreview {
        runtime,
        config_path,
    })
    .await
}

pub async fn send_external_runtime_migration_apply(
    runtime: String,
    config_path: Option<String>,
    conflict_policy: String,
) -> Result<serde_json::Value> {
    send_external_runtime_migration_query(ClientMessage::AgentExternalRuntimeMigrationApply {
        runtime,
        config_path,
        conflict_policy,
    })
    .await
}

pub async fn send_external_runtime_migration_report(
    runtime: Option<String>,
    limit: Option<usize>,
) -> Result<serde_json::Value> {
    send_external_runtime_migration_query(ClientMessage::AgentExternalRuntimeMigrationReport {
        runtime,
        limit,
    })
    .await
}

pub async fn send_external_runtime_migration_shadow_run(
    runtime: String,
) -> Result<serde_json::Value> {
    send_external_runtime_migration_query(ClientMessage::AgentExternalRuntimeMigrationShadowRun {
        runtime,
    })
    .await
}

async fn send_external_runtime_migration_query(
    message: ClientMessage,
) -> Result<serde_json::Value> {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        roundtrip_until(message, parse_external_runtime_migration_terminal_response),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "timed out waiting for daemon migration response; restart zorai-daemon so it matches this CLI"
        )
    })?
}

pub(crate) fn parse_external_runtime_migration_terminal_response(
    msg: DaemonMessage,
) -> Option<Result<serde_json::Value>> {
    match msg {
        DaemonMessage::AgentExternalRuntimeMigrationResult { .. }
        | DaemonMessage::Error { .. }
        | DaemonMessage::AgentError { .. } => Some(parse_external_runtime_migration_response(msg)),
        _ => None,
    }
}

pub(crate) fn parse_external_runtime_migration_response(
    msg: DaemonMessage,
) -> Result<serde_json::Value> {
    match msg {
        DaemonMessage::AgentExternalRuntimeMigrationResult { result_json } => {
            Ok(serde_json::from_str(&result_json)?)
        }
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub(crate) fn parse_config_set_response(msg: DaemonMessage) -> Result<()> {
    match msg {
        DaemonMessage::OperationAccepted { .. } | DaemonMessage::AgentConfigResponse { .. } => {
            Ok(())
        }
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub(crate) fn parse_operation_status_response(
    msg: DaemonMessage,
) -> Result<OperationStatusSnapshot> {
    match msg {
        DaemonMessage::OperationStatus { snapshot } => Ok(snapshot),
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_audit_query(
    action_types: Option<Vec<String>>,
    since: Option<u64>,
    limit: Option<usize>,
) -> Result<Vec<zorai_protocol::AuditEntryPublic>> {
    match roundtrip(ClientMessage::AuditQuery {
        action_types,
        since,
        limit,
    })
    .await?
    {
        DaemonMessage::AuditList { entries_json } => {
            let entries: Vec<zorai_protocol::AuditEntryPublic> =
                serde_json::from_str(&entries_json)?;
            Ok(entries)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
