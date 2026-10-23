use crate::client::get_string_lossy::get_string;
use crate::client::{ClientEvent, DaemonClient};
use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc;
use zorai_protocol::ClientMessage;

impl DaemonClient {
    fn is_internal_agent_thread(thread_id: Option<&str>, title: Option<&str>) -> bool {
        let normalized_id = thread_id.unwrap_or_default().trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("dm:") || normalized_title.starts_with("internal dm")
    }

    pub(crate) fn is_hidden_agent_thread(thread_id: Option<&str>, title: Option<&str>) -> bool {
        let normalized_id = thread_id.unwrap_or_default().trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("handoff:")
            || normalized_title.starts_with("handoff ")
            || normalized_title == "weles"
            || normalized_title.starts_with("weles ")
    }

    pub(crate) fn parse_weles_review(event: &Value) -> Option<crate::client::WelesReviewMetaVm> {
        let review = event.get("weles_review")?;
        let verdict = review.get("verdict").and_then(Value::as_str)?.to_string();
        let reasons = review
            .get("reasons")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(crate::client::WelesReviewMetaVm {
            weles_reviewed: review
                .get("weles_reviewed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            verdict,
            reasons,
            audit_id: get_string(review, "audit_id"),
            security_override_mode: get_string(review, "security_override_mode"),
        })
    }

    pub(crate) async fn dispatch_agent_event(event: Value, event_tx: &mpsc::Sender<ClientEvent>) {
        let Some(kind) = event
            .get("type")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
        else {
            return;
        };

        Self::dispatch_match_arms(&kind, event, event_tx).await;
    }

    pub(crate) fn send(&self, request: ClientMessage) -> Result<()> {
        zorai_protocol::validate_client_message_size(&request)?;
        self.request_tx.send(request)?;
        Ok(())
    }

    pub fn refresh(&self) -> Result<()> {
        self.refresh_threads_for_agent(None)
    }

    /// Request an unbounded thread list, optionally narrowed to a specific
    /// agent. The TUI caches the result and refreshes on demand (shift+r) or
    /// when a `ThreadCreated` event for the same filter arrives.
    pub fn refresh_threads_for_agent(&self, agent_filter: Option<String>) -> Result<()> {
        const TUI_THREAD_LIST_LIMIT: usize = 250;
        self.send(ClientMessage::AgentListThreads {
            limit: Some(TUI_THREAD_LIST_LIMIT),
            offset: Some(0),
            include_internal: true,
            agent_filter,
        })
    }

    pub fn search_threads(&self, query: String) -> Result<()> {
        self.send(ClientMessage::AgentSearchThreads { query, limit: None })
    }

    pub fn get_config(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetConfig)
    }

    pub fn get_mlflow_tracing_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetMlflowTracingStatus)
    }

    pub fn test_mlflow_tracing_connection(&self) -> Result<()> {
        self.send(ClientMessage::AgentTestMlflowTracingConnection)
    }

    pub fn send_mlflow_tracing_test_trace(&self) -> Result<()> {
        self.send(ClientMessage::AgentSendMlflowTracingTestTrace)
    }

    pub fn refresh_services(&self) -> Result<()> {
        for request in [
            ClientMessage::AgentListTasks,
            ClientMessage::AgentListGoalRuns {
                limit: None,
                offset: None,
            },
            ClientMessage::AgentHeartbeatGetItems,
        ] {
            self.send(request)?;
        }
        Ok(())
    }

    pub fn list_tasks(&self) -> Result<()> {
        self.send(ClientMessage::AgentListTasks)
    }

    pub fn request_goal_run(&self, goal_run_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetGoalRun {
            goal_run_id: goal_run_id.into(),
        })
    }

    pub fn request_goal_run_page(
        &self,
        goal_run_id: impl Into<String>,
        step_offset: Option<usize>,
        step_limit: Option<usize>,
        event_offset: Option<usize>,
        event_limit: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentGetGoalRunPage {
            goal_run_id: goal_run_id.into(),
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        })
    }

    pub fn start_goal_run(
        &self,
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
        launch_assignments: Vec<crate::state::task::GoalAgentAssignment>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartGoalRun {
            goal,
            title: None,
            thread_id,
            session_id,
            priority: None,
            client_request_id: None,
            launch_assignments: launch_assignments
                .into_iter()
                .map(|assignment| zorai_protocol::GoalAgentAssignment {
                    role_id: assignment.role_id,
                    enabled: assignment.enabled,
                    provider: assignment.provider,
                    model: assignment.model,
                    reasoning_effort: assignment.reasoning_effort,
                    inherit_from_main: assignment.inherit_from_main,
                })
                .collect(),
            autonomy_level: None,
            client_surface: Some(zorai_protocol::ClientSurface::Tui),
            target_agent_id: None,
            requires_approval: true,
        })
    }

    pub fn explain_action(&self, action_id: String, step_index: Option<usize>) -> Result<()> {
        self.send(ClientMessage::AgentExplainAction {
            action_id,
            step_index,
        })
    }

    pub fn start_divergent_session(
        &self,
        problem_statement: String,
        thread_id: String,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartDivergentSession {
            problem_statement,
            thread_id,
            goal_run_id,
            custom_framings_json: None,
        })
    }

    pub fn get_divergent_session(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentGetDivergentSession { session_id })
    }

    pub fn request_thread(
        &self,
        thread_id: impl Into<String>,
        message_limit: Option<usize>,
        message_offset: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentGetThread {
            thread_id: thread_id.into(),
            message_limit,
            message_offset,
            collapse_tool_calls: false,
        })
    }

    pub fn request_todos(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetTodos {
            thread_id: thread_id.into(),
        })
    }

    pub fn request_work_context(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetWorkContext {
            thread_id: thread_id.into(),
        })
    }

    pub fn list_notifications(&self) -> Result<()> {
        self.send(ClientMessage::ListAgentEvents {
            category: Some("notification".to_string()),
            pane_id: None,
            limit: Some(500),
        })
    }

    pub fn upsert_notification(
        &self,
        notification: zorai_protocol::InboxNotification,
    ) -> Result<()> {
        let event_row = zorai_protocol::AgentEventRow {
            id: notification.id.clone(),
            category: "notification".to_string(),
            kind: notification.kind.clone(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            session_id: None,
            payload_json: serde_json::to_string(&notification)?,
            timestamp: notification.updated_at,
        };
        self.send(ClientMessage::UpsertAgentEvent {
            event_json: serde_json::to_string(&event_row)?,
        })
    }

    pub fn mark_all_notifications_read(&self) -> Result<()> {
        self.send(ClientMessage::MarkAllNotificationsRead)
    }

    pub fn archive_read_notifications(&self) -> Result<()> {
        self.send(ClientMessage::ArchiveReadNotifications)
    }

    pub fn request_git_diff(
        &self,
        repo_path: impl Into<String>,
        file_path: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::GetGitDiff {
            repo_path: repo_path.into(),
            file_path,
        })
    }

    pub fn enqueue_prompt(
        &self,
        thread_id: String,
        prompt_id: String,
        content: String,
        content_blocks_json: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentEnqueuePrompt {
            thread_id,
            content,
            content_blocks_json,
            prompt_id: Some(prompt_id),
        })
    }

    pub fn list_prompt_queue(&self, thread_id: Option<String>) -> Result<()> {
        self.send(ClientMessage::AgentListPromptQueue { thread_id })
    }

    pub fn update_queued_prompt(
        &self,
        thread_id: String,
        prompt_id: String,
        content: String,
        content_blocks_json: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentUpdateQueuedPrompt {
            thread_id,
            prompt_id,
            content,
            content_blocks_json,
        })
    }

    pub fn cancel_queued_prompt(&self, thread_id: String, prompt_id: String) -> Result<()> {
        self.send(ClientMessage::AgentCancelQueuedPrompt {
            thread_id,
            prompt_id,
        })
    }

    pub fn send_queued_prompt_now(&self, thread_id: String, prompt_id: String) -> Result<()> {
        self.send(ClientMessage::AgentSendQueuedPromptNow {
            thread_id,
            prompt_id,
        })
    }
}
