use crate::client::get_string_lossy::{get_string, get_string_lossy};
use crate::client::ClientEvent;
use crate::client::DaemonClient;
use crate::wire::*;
use serde_json::Value;
use tokio::sync::mpsc;

impl DaemonClient {
    pub(crate) async fn dispatch_match_arms(
        kind: &str,
        event: Value,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) {
        match kind {
            "thread_created" => {
                let title =
                    get_string(&event, "title").unwrap_or_else(|| "New Conversation".to_string());
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let agent_name = get_string(&event, "agent_name");
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), Some(title.as_str())) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ThreadCreated {
                        thread_id,
                        title,
                        agent_name,
                    })
                    .await;
            }
            "thread_title_updated" => {
                let title = get_string(&event, "title").unwrap_or_default();
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if title.trim().is_empty()
                    || Self::is_hidden_agent_thread(Some(thread_id.as_str()), Some(title.as_str()))
                {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ThreadTitleUpdated { thread_id, title })
                    .await;
            }
            "next_reply_suggestion" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let content = get_string(&event, "content").unwrap_or_default();
                if content.trim().is_empty()
                    || Self::is_hidden_agent_thread(Some(thread_id.as_str()), None)
                {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::NextReplySuggestion { thread_id, content })
                    .await;
            }
            "thread_reload_required" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ThreadReloadRequired { thread_id })
                    .await;
            }
            "context_window_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ContextWindowUpdate {
                        thread_id,
                        active_context_window_start: event
                            .get("active_context_window_start")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok())
                            .unwrap_or(0),
                        active_context_window_end: event
                            .get("active_context_window_end")
                            .and_then(Value::as_u64)
                            .and_then(|value| usize::try_from(value).ok())
                            .unwrap_or(0),
                        active_context_window_tokens: event
                            .get("active_context_window_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    })
                    .await;
            }
            "participant_suggestion" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                if let Some(suggestion) = event
                    .get("suggestion")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<ThreadParticipantSuggestion>(raw).ok())
                {
                    let _ = event_tx
                        .send(ClientEvent::ParticipantSuggestion {
                            thread_id,
                            suggestion,
                        })
                        .await;
                }
            }
            "delta" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::Delta {
                        thread_id,
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "reasoning" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::Reasoning {
                        thread_id,
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "tool_call" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ToolCall {
                        thread_id,
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        arguments: get_string_lossy(&event, "arguments"),
                        weles_review: Self::parse_weles_review(&event),
                        message_id: get_string(&event, "message_id"),
                    })
                    .await;
            }
            "tool_result" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let _ = event_tx
                    .send(ClientEvent::ToolResult {
                        thread_id,
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        content: get_string_lossy(&event, "content"),
                        is_error: event
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        weles_review: Self::parse_weles_review(&event),
                        message_id: get_string(&event, "message_id"),
                    })
                    .await;
            }
            "approval_required" => {
                let reasons = event
                    .get("reasons")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                item.as_str()
                                    .map(ToOwned::to_owned)
                                    .unwrap_or_else(|| item.to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::ApprovalRequired {
                        approval_id: get_string(&event, "approval_id").unwrap_or_default(),
                        command: get_string_lossy(&event, "command"),
                        rationale: get_string(&event, "rationale"),
                        reasons,
                        risk_level: get_string(&event, "risk_level")
                            .unwrap_or_else(|| "medium".to_string()),
                        blast_radius: get_string(&event, "blast_radius")
                            .unwrap_or_else(|| "tool execution".to_string()),
                    })
                    .await;
            }
            "done" | "turn_interrupted" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                if Self::is_hidden_agent_thread(Some(thread_id.as_str()), None) {
                    return;
                }
                let done = if kind == "turn_interrupted" {
                    ClientEvent::Done {
                        thread_id,
                        input_tokens: 0,
                        output_tokens: 0,
                        cost: None,
                        provider: None,
                        model: None,
                        tps: None,
                        generation_ms: None,
                        reasoning: None,
                        provider_final_result_json: None,
                        message_id: None,
                    }
                } else {
                    ClientEvent::Done {
                        thread_id,
                        input_tokens: event
                            .get("input_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        output_tokens: event
                            .get("output_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        cost: event.get("cost").and_then(Value::as_f64),
                        provider: get_string(&event, "provider"),
                        model: get_string(&event, "model"),
                        tps: event.get("tps").and_then(Value::as_f64),
                        generation_ms: event.get("generation_ms").and_then(Value::as_u64),
                        reasoning: get_string(&event, "reasoning"),
                        provider_final_result_json: event
                            .get("provider_final_result")
                            .and_then(|value| serde_json::to_string(value).ok()),
                        message_id: get_string(&event, "message_id"),
                    }
                };
                let _ = event_tx.send(done).await;
            }
            "error" => {
                let _ = event_tx
                    .send(ClientEvent::Error(
                        get_string(&event, "message")
                            .unwrap_or_else(|| "Unknown agent error".to_string()),
                    ))
                    .await;
            }
            "workflow_notice" => {
                let _ = event_tx
                    .send(ClientEvent::WorkflowNotice {
                        thread_id: get_string(&event, "thread_id"),
                        kind: get_string(&event, "kind").unwrap_or_default(),
                        message: get_string(&event, "message").unwrap_or_default(),
                        details: get_string(&event, "details"),
                    })
                    .await;
            }
            "operator_question" => {
                let options = event
                    .get("options")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::OperatorQuestion {
                        question_id: get_string(&event, "question_id").unwrap_or_default(),
                        content: get_string(&event, "content").unwrap_or_default(),
                        options,
                        session_id: get_string(&event, "session_id"),
                        thread_id: get_string(&event, "thread_id"),
                    })
                    .await;
            }
            "operator_question_resolved" => {
                let _ = event_tx
                    .send(ClientEvent::OperatorQuestionResolved {
                        question_id: get_string(&event, "question_id").unwrap_or_default(),
                        answer: get_string(&event, "answer").unwrap_or_default(),
                    })
                    .await;
            }
            "notification_inbox_upsert" => {
                if let Some(notification) = event.get("notification").cloned().and_then(|raw| {
                    serde_json::from_value::<zorai_protocol::InboxNotification>(raw).ok()
                }) {
                    let _ = event_tx
                        .send(ClientEvent::NotificationUpsert(notification))
                        .await;
                }
            }
            "workspace_task_update" => {
                if let Some(task) = event.get("task").cloned().and_then(|raw| {
                    serde_json::from_value::<zorai_protocol::WorkspaceTask>(raw).ok()
                }) {
                    let _ = event_tx.send(ClientEvent::WorkspaceTaskUpdated(task)).await;
                }
            }
            "workspace_settings_update" => {
                if let Some(settings) = event.get("settings").cloned().and_then(|raw| {
                    serde_json::from_value::<zorai_protocol::WorkspaceSettings>(raw).ok()
                }) {
                    let _ = event_tx
                        .send(ClientEvent::WorkspaceSettings(settings))
                        .await;
                }
            }
            "workspace_task_deleted" => {
                let task_id = get_string(&event, "task_id").unwrap_or_default();
                let deleted_at = event.get("deleted_at").and_then(Value::as_u64);
                let _ = event_tx
                    .send(ClientEvent::WorkspaceTaskDeleted {
                        task_id,
                        deleted_at,
                    })
                    .await;
            }
            "workspace_notice_update" => {
                if let Some(notice) = event.get("notice").cloned().and_then(|raw| {
                    serde_json::from_value::<zorai_protocol::WorkspaceNotice>(raw).ok()
                }) {
                    let _ = event_tx
                        .send(ClientEvent::WorkspaceNoticeUpdated(notice))
                        .await;
                }
            }
            "weles_health_update" => {
                let _ = event_tx
                    .send(ClientEvent::WelesHealthUpdate {
                        state: get_string(&event, "state").unwrap_or_else(|| "healthy".to_string()),
                        reason: get_string(&event, "reason"),
                        checked_at: event.get("checked_at").and_then(Value::as_u64).unwrap_or(0),
                    })
                    .await;
            }
            "retry_status" => {
                let _ = event_tx
                    .send(ClientEvent::RetryStatus {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        phase: get_string(&event, "phase")
                            .unwrap_or_else(|| "retrying".to_string()),
                        attempt: event.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32,
                        max_retries: event
                            .get("max_retries")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32,
                        delay_ms: event.get("delay_ms").and_then(Value::as_u64).unwrap_or(0),
                        failure_class: get_string(&event, "failure_class")
                            .unwrap_or_else(|| "transient".to_string()),
                        message: get_string(&event, "message").unwrap_or_default(),
                    })
                    .await;
            }
            "anticipatory_update" => {
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<AnticipatoryItem>>(raw).ok())
                    .unwrap_or_default();
                let _ = event_tx.send(ClientEvent::AnticipatoryItems(items)).await;
            }
            "task_update" => {
                let task = event
                    .get("task")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<AgentTask>(raw).ok())
                    .unwrap_or_else(|| AgentTask {
                        id: get_string(&event, "task_id").unwrap_or_default(),
                        title: get_string(&event, "message")
                            .unwrap_or_else(|| "Task update".to_string()),
                        description: String::new(),
                        thread_id: None,
                        parent_task_id: None,
                        parent_thread_id: None,
                        created_at: 0,
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<TaskStatus>(raw).ok()),
                        progress: event.get("progress").and_then(Value::as_u64).unwrap_or(0) as u8,
                        session_id: None,
                        goal_run_id: None,
                        goal_step_title: None,
                        command: None,
                        awaiting_approval_id: None,
                        blocked_reason: get_string(&event, "message"),
                        sub_agent_def_id: None,
                    });
                let _ = event_tx.send(ClientEvent::TaskUpdate(task)).await;
            }
            "goal_run_update" => {
                let goal_run = event
                    .get("goal_run")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<GoalRun>(raw).ok())
                    .unwrap_or_else(|| GoalRun {
                        id: get_string(&event, "goal_run_id").unwrap_or_default(),
                        title: "Goal run update".to_string(),
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<GoalRunStatus>(raw).ok()),
                        current_step_index: event
                            .get("current_step_index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as usize,
                        last_error: None,
                        sparse_update: true,
                        ..GoalRun::default()
                    });
                let _ = event_tx.send(ClientEvent::GoalRunUpdate(goal_run)).await;
            }
            "todo_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let goal_run_id = get_string(&event, "goal_run_id");
                let step_index = event
                    .get("step_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok());
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<crate::wire::TodoItem>>(raw).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|mut todo| {
                        if todo.step_index.is_none() {
                            todo.step_index = step_index;
                        }
                        todo
                    })
                    .collect();
                let _ = event_tx
                    .send(ClientEvent::ThreadTodos {
                        thread_id,
                        goal_run_id,
                        step_index,
                        items,
                    })
                    .await;
            }
            "work_context_update" => {
                if let Some(context) = event
                    .get("context")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<ThreadWorkContext>(raw).ok())
                {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
            }
            "concierge_welcome" => {
                let content = event
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let actions = event
                    .get("actions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                Some(crate::state::ConciergeActionVm {
                                    label: a.get("label")?.as_str()?.to_string(),
                                    action_type: a.get("action_type")?.as_str()?.to_string(),
                                    thread_id: a
                                        .get("thread_id")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::ConciergeWelcome { content, actions })
                    .await;
            }
            "heartbeat_digest" => {
                let items: Vec<(u8, String, String, String)> = event
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                Some((
                                    item.get("priority")?.as_u64()? as u8,
                                    item.get("check_type")?.as_str()?.to_string(),
                                    item.get("title")?.as_str()?.to_string(),
                                    item.get("suggestion")?.as_str()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let _ = event_tx
                    .send(ClientEvent::HeartbeatDigest {
                        cycle_id: get_string(&event, "cycle_id").unwrap_or_default(),
                        actionable: event
                            .get("actionable")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        digest: get_string(&event, "digest").unwrap_or_default(),
                        items,
                        checked_at: event.get("checked_at").and_then(Value::as_u64).unwrap_or(0),
                        explanation,
                    })
                    .await;
            }
            "audit_action" => {
                let id = get_string(&event, "id").unwrap_or_default();
                let timestamp = event.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
                let action_type = get_string(&event, "action_type").unwrap_or_default();
                let summary = get_string(&event, "summary").unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let confidence = event.get("confidence").and_then(Value::as_f64);
                let confidence_band = get_string(&event, "confidence_band");
                let causal_trace_id = get_string(&event, "causal_trace_id");
                let thread_id = get_string(&event, "thread_id");
                let _ = event_tx
                    .send(ClientEvent::AuditEntry {
                        id,
                        timestamp,
                        action_type,
                        summary,
                        explanation,
                        confidence,
                        confidence_band,
                        causal_trace_id,
                        thread_id,
                    })
                    .await;
            }
            "escalation_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let from_level = get_string(&event, "from_level").unwrap_or_default();
                let to_level = get_string(&event, "to_level").unwrap_or_default();
                let reason = get_string(&event, "reason").unwrap_or_default();
                let attempts = event.get("attempts").and_then(Value::as_u64).unwrap_or(0) as u32;
                let audit_id = get_string(&event, "audit_id");
                let _ = event_tx
                    .send(ClientEvent::EscalationUpdate {
                        thread_id,
                        from_level,
                        to_level,
                        reason,
                        attempts,
                        audit_id,
                    })
                    .await;
            }
            "gateway_status" => {
                let platform = get_string(&event, "platform").unwrap_or_default();
                let status = get_string(&event, "status").unwrap_or_default();
                let last_error = get_string(&event, "last_error");
                let consecutive_failures = event
                    .get("consecutive_failures")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                let _ = event_tx
                    .send(ClientEvent::GatewayStatus {
                        platform,
                        status,
                        last_error,
                        consecutive_failures,
                    })
                    .await;
            }
            "prompt_queue_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let prompts = event
                    .get("prompts")
                    .cloned()
                    .and_then(|raw| {
                        serde_json::from_value::<Vec<zorai_protocol::QueuedPromptRecord>>(raw).ok()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::PromptQueue {
                        thread_id: if thread_id.is_empty() {
                            None
                        } else {
                            Some(thread_id)
                        },
                        prompts,
                    })
                    .await;
            }
            "message_feedback_updated" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let message_id = get_string(&event, "message_id").unwrap_or_default();
                let reaction = event
                    .get("reaction")
                    .and_then(Value::as_str)
                    .and_then(|value| match value {
                        "up" => Some(zorai_protocol::Reaction::Up),
                        "down" => Some(zorai_protocol::Reaction::Down),
                        _ => None,
                    });
                let _ = event_tx
                    .send(ClientEvent::MessageFeedbackUpdated {
                        thread_id,
                        message_id,
                        reaction,
                    })
                    .await;
            }
            "tier_changed" | "tier-changed" => {
                let data = event.get("data").cloned().unwrap_or_else(|| event.clone());
                let new_tier = data
                    .get("new_tier")
                    .or_else(|| data.get("newTier"))
                    .and_then(Value::as_str)
                    .unwrap_or("newcomer")
                    .to_string();
                let _ = event_tx.send(ClientEvent::TierChanged { new_tier }).await;
            }
            _ => {}
        }
    }
}
