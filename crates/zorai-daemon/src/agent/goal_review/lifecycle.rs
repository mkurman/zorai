use super::super::*;
use super::{review_state_key, GoalReviewRecord};

impl AgentEngine {
    pub(super) async fn complete_goal_from_supervisor(
        &self,
        goal_run_id: &str,
        explanation: &str,
    ) -> Result<GoalRun> {
        let now = now_millis();
        let (updated, worker_task_id) = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing during accept");
            };
            let worker_task_id = goal_run.active_task_id.clone();
            goal_run.status = GoalRunStatus::Completed;
            goal_run.completed_at = Some(now);
            goal_run.updated_at = now;
            goal_run.pending_review_report = None;
            goal_run.last_error = None;
            goal_run.failure_cause = None;
            goal_run.duration_ms = goal_run
                .started_at
                .zip(goal_run.completed_at)
                .map(|(started_at, completed_at)| completed_at.saturating_sub(started_at));
            goal_run.events.push(make_goal_run_event(
                "review",
                "owner accepted goal",
                (!explanation.is_empty()).then(|| explanation.to_string()),
            ));
            (goal_run.clone(), worker_task_id)
        };
        if let Some(task_id) = worker_task_id {
            let _ = self.cancel_task(&task_id).await;
        }
        self.persist_goal_runs().await;
        self.quiesce_goal_execution_tree(&updated, true).await;
        self.notify_goal_worker_of_verdict(
            &updated,
            format!("Supervisor accepted this goal. The goal is complete; stop working on it.\n\n{explanation}"),
        )
        .await;
        self.unpin_goal_owner_thread(&updated).await;
        self.emit_goal_run_update(&updated, Some("Goal completed".into()));
        Ok(updated)
    }

    pub(super) async fn soft_reject_goal_worker(
        &self,
        goal_run_id: &str,
        explanation: &str,
    ) -> Result<GoalRun> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during soft reject")?;
        let worker_task_id = snapshot
            .active_task_id
            .clone()
            .ok_or_else(|| anyhow::anyhow!("goal {goal_run_id} has no worker to resume"))?;
        let Some(mut worker) = self.task_by_id_for_dispatcher(&worker_task_id).await else {
            anyhow::bail!("goal worker {worker_task_id} missing during soft reject");
        };
        let worker_thread_id = worker
            .thread_id
            .clone()
            .or_else(|| snapshot.thread_id.clone());
        if let Some(thread_id) = worker_thread_id.as_deref() {
            self.append_system_thread_message(
                thread_id,
                format!(
                    "Supervisor soft-rejected this goal. Keep working on the same thread.\n\n{explanation}"
                ),
            )
            .await;
        }
        worker.status = TaskStatus::Queued;
        worker.blocked_reason = None;
        worker.completed_at = None;
        worker.error = None;
        worker.last_error = None;
        worker.next_retry_at = None;
        worker.logs.push(make_task_log_entry(
            worker.retry_count,
            TaskLogLevel::Info,
            "goal_review",
            "supervisor soft-rejected; worker continues",
            Some(explanation.to_string()),
        ));
        self.upsert_live_task(&worker).await;

        let now = now_millis();
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing after soft reject");
            };
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now;
            goal_run.pending_review_report = None;
            goal_run.events.push(make_goal_run_event(
                "review",
                "owner soft-rejected; worker continues",
                Some(explanation.to_string()),
            ));
            goal_run.clone()
        };
        self.persist_goal_runs().await;
        self.emit_goal_run_update(
            &updated,
            Some("Goal worker continuing after soft reject".into()),
        );
        Ok(updated)
    }

    pub(super) async fn hard_reject_goal_run(&self, goal_run_id: &str, explanation: &str) {
        self.fail_goal_run(goal_run_id, explanation, "hard_reject", None)
            .await;
        if let Some(updated) = self.get_goal_run(goal_run_id).await {
            self.notify_goal_worker_of_verdict(
                &updated,
                format!("Supervisor hard-rejected this goal. The goal is stopped; do not continue it.\n\n{explanation}"),
            )
            .await;
            self.unpin_goal_owner_thread(&updated).await;
        }
    }

    async fn notify_goal_worker_of_verdict(&self, goal_run: &GoalRun, content: String) {
        let Some(worker_thread_id) = goal_run.thread_id.as_deref() else {
            return;
        };
        if goal_run.supervision_thread_id.as_deref() == Some(worker_thread_id) {
            return;
        }
        if !self
            .append_system_thread_message(worker_thread_id, content.trim_end())
            .await
        {
            tracing::warn!(
                goal_run_id = %goal_run.id,
                thread_id = %worker_thread_id,
                "could not deliver supervisor verdict to goal worker thread"
            );
        }
    }

    pub(in crate::agent) async fn fail_goal_run(
        &self,
        goal_run_id: &str,
        error: &str,
        phase: &str,
        thread_id: Option<String>,
    ) {
        let now = now_millis();
        let maybe_updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter_mut()
                .find(|item| item.id == goal_run_id)
                .map(|goal_run| {
                    super::super::goal_run_apply_thread_routing(goal_run, thread_id.clone());
                    goal_run.status = GoalRunStatus::Failed;
                    goal_run.completed_at = Some(now);
                    goal_run.updated_at = now;
                    goal_run.last_error = Some(error.to_string());
                    goal_run.failure_cause = Some(error.to_string());
                    goal_run.pending_review_report = None;
                    goal_run.awaiting_approval_id = None;
                    goal_run.active_task_id = None;
                    goal_run.duration_ms = goal_run
                        .started_at
                        .zip(goal_run.completed_at)
                        .map(|(started_at, completed_at)| completed_at.saturating_sub(started_at));
                    goal_run.events.push(make_goal_run_event(
                        phase,
                        "goal run failed",
                        Some(error.to_string()),
                    ));
                    goal_run.clone()
                })
        };
        let Some(updated) = maybe_updated else {
            return;
        };
        self.persist_goal_runs().await;
        self.quiesce_goal_execution_tree(&updated, true).await;
        for task_id in self.goal_related_task_ids(&updated).await {
            let _ = self.cancel_task(&task_id).await;
        }
        self.cancel_goal_wakeups(goal_run_id).await;
        self.unpin_goal_owner_thread(&updated).await;
        self.emit_goal_run_update(&updated, Some(format!("Goal failed: {error}")));
    }

    pub(in crate::agent) async fn pin_goal_owner_thread(&self, goal_run: &GoalRun) {
        let Some(owner_thread_id) = goal_run.supervision_thread_id.as_deref() else {
            return;
        };
        if goal_run.thread_id.as_deref() == Some(owner_thread_id) {
            return;
        }
        self.set_thread_pinned(owner_thread_id, true).await;
    }

    pub(in crate::agent) async fn unpin_goal_owner_thread(&self, goal_run: &GoalRun) {
        let Some(owner_thread_id) = goal_run.supervision_thread_id.as_deref() else {
            return;
        };
        self.set_thread_pinned(owner_thread_id, false).await;
    }

    async fn set_thread_pinned(&self, thread_id: &str, pinned: bool) {
        self.ensure_thread_messages_loaded(thread_id).await;
        let title = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| thread.title.clone())
                .unwrap_or_else(|| thread_id.to_string())
        };
        self.ensure_thread_identity(thread_id, &title, pinned).await;
    }

    pub(super) async fn wake_goal_owner_for_review(&self, goal_run: &GoalRun, report: &str) {
        let Some(owner_thread_id) = goal_run.supervision_thread_id.as_deref() else {
            return;
        };
        if goal_run.thread_id.as_deref() == Some(owner_thread_id) {
            return;
        }
        let content = format!(
            "Goal `{}` worker requested review and is paused until you verdict.\n\n\
             Goal: {}\n\nReport:\n{}\n\n\
             Call `submit_goal_review` with goal_run_id `{}` and verdict `accept`, \
             `soft_reject`, or `hard_reject`. Soft reject requires why the work is incomplete. \
             Hard reject stops the goal, the worker thread, and unpins this supervision.",
            goal_run.id, goal_run.goal, report, goal_run.id
        );
        let _ = self
            .append_system_thread_message(owner_thread_id, content)
            .await;
        let agent_id = self
            .agent_scope_id_for_turn(Some(owner_thread_id), None)
            .await;
        self.enqueue_visible_thread_continuation(
            owner_thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id: None,
                preferred_session_hint: None,
                llm_user_content: format!(
                    "Review goal {} and call submit_goal_review with accept, soft_reject, or hard_reject.",
                    goal_run.id
                ),
                queued_at_ms: now_millis(),
                force_compaction: false,
                rerun_participant_observers_after_turn: false,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;
    }

    pub(super) async fn load_goal_review_record(
        &self,
        goal_run_id: &str,
    ) -> Result<Option<GoalReviewRecord>> {
        let Some(value) = self
            .history
            .get_consolidation_state(&review_state_key(goal_run_id))
            .await?
        else {
            return Ok(None);
        };
        Ok(serde_json::from_str(&value).ok())
    }

    pub(in crate::agent) async fn upsert_live_task(&self, updated: &AgentTask) {
        {
            let mut tasks = self.tasks.lock().await;
            if let Some(task) = tasks.iter_mut().find(|task| task.id == updated.id) {
                *task = updated.clone();
            } else {
                tasks.push_back(updated.clone());
            }
        }
        let _ = self.history.upsert_agent_task(updated).await;
        self.persist_tasks().await;
        self.emit_task_update(updated, Some(status_message(updated).into()));
    }

    pub(super) async fn requeue_goal_worker_task(&self, task: &AgentTask) {
        let mut updated = task.clone();
        if is_task_terminal_status(updated.status) || updated.status == TaskStatus::Blocked {
            updated.status = TaskStatus::Queued;
            updated.completed_at = None;
            updated.blocked_reason = None;
            self.upsert_live_task(&updated).await;
        }
    }
}
