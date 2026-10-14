use super::*;

#[derive(Debug, Clone)]
pub(in crate::agent) struct AgentWakeup {
    pub(in crate::agent) id: String,
    pub(in crate::agent) thread_id: String,
    pub(in crate::agent) message: String,
    pub(in crate::agent) interval_ms: u64,
    pub(in crate::agent) next_fire_at: u64,
    pub(in crate::agent) repetitions_remaining: Option<u64>,
    pub(in crate::agent) wakeup_kind: String,
    pub(in crate::agent) goal_run_id: Option<String>,
    pub(in crate::agent) reason: Option<String>,
    pub(in crate::agent) created_at: u64,
}

impl AgentWakeup {
    fn to_row(&self) -> crate::history::AgentWakeupRow {
        crate::history::AgentWakeupRow {
            id: self.id.clone(),
            thread_id: self.thread_id.clone(),
            message: self.message.clone(),
            interval_ms: self.interval_ms,
            next_fire_at: self.next_fire_at,
            repetitions_remaining: self.repetitions_remaining,
            wakeup_kind: self.wakeup_kind.clone(),
            goal_run_id: self.goal_run_id.clone(),
            reason: self.reason.clone(),
            created_at: self.created_at,
        }
    }
}

impl AgentEngine {
    #[cfg(test)]
    pub(in crate::agent) async fn schedule_wakeup(
        &self,
        thread_id: &str,
        delay_ms: u64,
        repetitions: u64,
        message: &str,
    ) -> AgentWakeup {
        self.schedule_wakeup_with_context(
            thread_id,
            delay_ms,
            repetitions,
            message,
            "generic",
            None,
            None,
        )
        .await
        .expect("generic wakeup configuration is always valid")
    }

    pub(in crate::agent) async fn schedule_wakeup_with_context(
        &self,
        thread_id: &str,
        delay_ms: u64,
        repetitions: u64,
        message: &str,
        wakeup_kind: &str,
        goal_run_id: Option<&str>,
        reason: Option<&str>,
    ) -> Result<AgentWakeup> {
        if !matches!(wakeup_kind, "generic" | "goal_supervision") {
            anyhow::bail!("unsupported wakeup kind '{wakeup_kind}'");
        }
        if wakeup_kind == "goal_supervision" {
            if goal_run_id.is_none() {
                anyhow::bail!("goal supervision requires goal ownership");
            }
            if repetitions != 1 {
                anyhow::bail!(
                    "goal supervision must schedule exactly one wakeup and be reassessed after the triggered turn"
                );
            }
            let goal_run_id = goal_run_id.expect("goal ownership checked above");
            match self.get_goal_run(goal_run_id).await {
                Some(goal) if goal.status.is_terminal() => {
                    anyhow::bail!("cannot supervise terminal goal {goal_run_id}");
                }
                Some(_) => {}
                None => anyhow::bail!("cannot supervise missing goal {goal_run_id}"),
            }
        }
        let now = now_millis();
        let interval_ms = delay_ms.max(1);
        let wakeup = AgentWakeup {
            id: format!("wakeup_{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.to_string(),
            message: message.to_string(),
            interval_ms,
            next_fire_at: now.saturating_add(interval_ms),
            repetitions_remaining: (repetitions != 0).then_some(repetitions),
            wakeup_kind: wakeup_kind.to_string(),
            goal_run_id: goal_run_id.map(str::to_string),
            reason: reason
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            created_at: now,
        };
        self.history.upsert_agent_wakeup(&wakeup.to_row()).await?;
        self.timer_wakeups
            .lock()
            .await
            .insert(wakeup.id.clone(), wakeup.clone());
        Ok(wakeup)
    }

    pub(in crate::agent) async fn cancel_goal_wakeups(&self, goal_run_id: &str) -> usize {
        let removed = {
            let mut wakeups = self.timer_wakeups.lock().await;
            let ids = wakeups
                .values()
                .filter(|wakeup| wakeup.goal_run_id.as_deref() == Some(goal_run_id))
                .map(|wakeup| wakeup.id.clone())
                .collect::<Vec<_>>();
            let removed = ids.len();
            for id in ids {
                wakeups.remove(&id);
            }
            removed
        };
        if let Err(error) = self
            .history
            .delete_agent_wakeups_for_goal(goal_run_id)
            .await
        {
            tracing::warn!(goal_run_id, error = %error, "failed to delete persisted goal wakeups");
        }
        removed
    }

    pub(in crate::agent) async fn cancel_wakeups_for_threads(
        &self,
        thread_ids: &std::collections::HashSet<String>,
    ) -> usize {
        let ids = {
            let wakeups = self.timer_wakeups.lock().await;
            wakeups
                .values()
                .filter(|wakeup| thread_ids.contains(&wakeup.thread_id))
                .map(|wakeup| wakeup.id.clone())
                .collect::<Vec<_>>()
        };
        for id in &ids {
            self.timer_wakeups.lock().await.remove(id);
            if let Err(error) = self.history.delete_agent_wakeup(id).await {
                tracing::warn!(wakeup_id = %id, %error, "failed to delete descendant wakeup");
            }
        }
        ids.len()
    }

    pub(in crate::agent) async fn cancel_wakeup(&self, wakeup_id: &str) -> bool {
        let wakeup_id = wakeup_id.trim();
        let removed = self.timer_wakeups.lock().await.remove(wakeup_id).is_some();
        if let Err(error) = self.history.delete_agent_wakeup(wakeup_id).await {
            tracing::warn!(error = %error, "failed to delete persisted wakeup");
        }
        removed
    }

    #[cfg(test)]
    pub(in crate::agent) async fn pending_wakeup_count(&self) -> usize {
        self.timer_wakeups.lock().await.len()
    }

    pub(in crate::agent) async fn supervise_timer_wakeups(&self) -> Result<()> {
        let now = now_millis();
        let live_threads: std::collections::HashSet<String> =
            self.threads.read().await.keys().cloned().collect();
        let due_goal_wakeups = {
            let wakeups = self.timer_wakeups.lock().await;
            wakeups
                .values()
                .filter(|wakeup| {
                    wakeup.next_fire_at <= now && wakeup.wakeup_kind == "goal_supervision"
                })
                .filter_map(|wakeup| {
                    wakeup
                        .goal_run_id
                        .clone()
                        .map(|goal_run_id| (wakeup.id.clone(), goal_run_id))
                })
                .collect::<Vec<_>>()
        };
        let mut terminal_goal_wakeup_ids = std::collections::HashSet::new();
        let mut goal_status_by_wakeup_id = std::collections::HashMap::new();
        for (wakeup_id, goal_run_id) in due_goal_wakeups {
            match self.get_goal_run(&goal_run_id).await {
                Some(goal) if goal.status.is_terminal() => {
                    terminal_goal_wakeup_ids.insert(wakeup_id);
                }
                Some(goal) => {
                    goal_status_by_wakeup_id.insert(wakeup_id, format!("{:?}", goal.status));
                }
                None => {
                    terminal_goal_wakeup_ids.insert(wakeup_id);
                }
            }
        }
        let mut fired_by_thread: std::collections::BTreeMap<
            String,
            Vec<(String, String, Option<String>)>,
        > = std::collections::BTreeMap::new();
        let mut to_delete: Vec<AgentWakeup> = Vec::new();
        let mut to_upsert: Vec<AgentWakeup> = Vec::new();
        {
            let mut wakeups = self.timer_wakeups.lock().await;
            let due_ids = wakeups
                .values()
                .filter(|wakeup| wakeup.next_fire_at <= now)
                .map(|wakeup| wakeup.id.clone())
                .collect::<Vec<_>>();
            for id in due_ids {
                if terminal_goal_wakeup_ids.contains(&id) {
                    if let Some(removed) = wakeups.remove(&id) {
                        to_delete.push(removed);
                    }
                    continue;
                }
                let Some(wakeup) = wakeups.get_mut(&id) else {
                    continue;
                };
                if !live_threads.contains(&wakeup.thread_id) {
                    if let Some(removed) = wakeups.remove(&id) {
                        to_delete.push(removed);
                    }
                    continue;
                }
                let message = if wakeup.wakeup_kind == "goal_supervision" {
                    format!(
                        "Goal supervision check for {}. Scheduler-observed status: {}. Re-read the current goal status before acting because it may have changed. If the goal is terminal or no further supervision is useful, stop and do not schedule another wakeup. If continued supervision is justified, schedule exactly one finite follow-up after explaining why.\n\n{}",
                        wakeup.goal_run_id.as_deref().unwrap_or("unknown goal"),
                        goal_status_by_wakeup_id
                            .get(&id)
                            .map(String::as_str)
                            .unwrap_or("unknown"),
                        wakeup.message
                    )
                } else {
                    wakeup.message.clone()
                };
                let mut message = match wakeup.reason.as_deref() {
                    Some(reason) => format!("{message}\nScheduled because: {reason}"),
                    None => message,
                };
                let is_last =
                    matches!(wakeup.repetitions_remaining, Some(remaining) if remaining <= 1);
                if !is_last {
                    message.push_str(&format!(
                        "\nCancel this scheduled wakeup with cancel_wakeup(wakeup_id=\"{id}\") if you no longer need it."
                    ));
                }
                fired_by_thread
                    .entry(wakeup.thread_id.clone())
                    .or_default()
                    .push((id.clone(), message, wakeup.goal_run_id.clone()));
                if is_last {
                    if let Some(removed) = wakeups.remove(&id) {
                        to_delete.push(removed);
                    }
                } else {
                    if let Some(remaining) = wakeup.repetitions_remaining {
                        wakeup.repetitions_remaining = Some(remaining - 1);
                    }
                    wakeup.next_fire_at = now.saturating_add(wakeup.interval_ms);
                    to_upsert.push(wakeup.clone());
                }
            }
        }

        let mut failed_deletes = std::collections::HashSet::new();
        for wakeup in to_delete {
            if let Err(error) = self.history.delete_agent_wakeup(&wakeup.id).await {
                tracing::warn!(error = %error, "failed to delete fired wakeup");
                failed_deletes.insert(wakeup.id.clone());
                self.timer_wakeups
                    .lock()
                    .await
                    .insert(wakeup.id.clone(), wakeup);
            }
        }
        for wakeup in to_upsert {
            if let Err(error) = self.history.upsert_agent_wakeup(&wakeup.to_row()).await {
                tracing::warn!(error = %error, "failed to persist rescheduled wakeup");
            }
        }

        for (thread_id, messages) in fired_by_thread {
            // Keep the goal-runs lock through continuation enqueue so a terminal transition
            // cannot race between the final status check and delivery of supervision work.
            let goal_runs = self.goal_runs.lock().await;
            let messages = messages
                .into_iter()
                .filter_map(|(wakeup_id, message, goal_run_id)| {
                    if failed_deletes.contains(&wakeup_id) {
                        return None;
                    }
                    match goal_run_id {
                        Some(goal_run_id)
                            if goal_runs.iter().any(|goal| {
                                goal.id == goal_run_id && !goal.status.is_terminal()
                            }) =>
                        {
                            Some((message, Some(goal_run_id)))
                        }
                        Some(_) => None,
                        None => Some((message, None)),
                    }
                })
                .collect::<Vec<_>>();
            drop(goal_runs);
            if messages.is_empty() {
                continue;
            }

            let mut routed_messages =
                std::collections::BTreeMap::<(String, String), Vec<String>>::new();
            for (message, goal_run_id) in messages {
                let route = match goal_run_id {
                    Some(goal_run_id) => {
                        match self.resolve_goal_supervision_route(&goal_run_id).await {
                            Ok(route) => (route.thread_id, route.agent_id),
                            Err(error) => {
                                tracing::warn!(
                                    goal_run_id,
                                    error = %error,
                                    "goal supervision wakeup has no resolvable owner route"
                                );
                                continue;
                            }
                        }
                    }
                    None => {
                        let agent_id = self
                            .active_agent_id_for_thread(&thread_id)
                            .await
                            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
                        (thread_id.clone(), agent_id)
                    }
                };
                routed_messages.entry(route).or_default().push(message);
            }

            for ((route_thread_id, agent_id), messages) in routed_messages {
                let body = if messages.len() == 1 {
                    messages.into_iter().next().unwrap_or_default()
                } else {
                    messages
                        .iter()
                        .enumerate()
                        .map(|(index, message)| format!("{}. {message}", index + 1))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let content = format!("[Scheduled wakeup] {body}");
                self.enqueue_visible_thread_continuation(
                    &route_thread_id,
                    DeferredVisibleThreadContinuation {
                        agent_id,
                        task_id: None,
                        preferred_session_hint: None,
                        llm_user_content: content,
                        queued_at_ms: 0,
                        force_compaction: false,
                        rerun_participant_observers_after_turn: true,
                        internal_delegate_sender: None,
                        internal_delegate_message: None,
                    },
                )
                .await;
                if let Err(error) = self
                    .flush_deferred_visible_thread_continuations(&route_thread_id)
                    .await
                {
                    tracing::warn!(
                        thread_id = %route_thread_id,
                        error = %error,
                        "scheduled wakeup continuation flush failed"
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn engine_with_thread(thread_id: &str) -> std::sync::Arc<AgentEngine> {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let now = now_millis();
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Wakeup".to_string(),
                messages: vec![AgentMessage::user("kick off the job", now)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now,
                updated_at: now,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
        engine
    }

    async fn force_due(engine: &AgentEngine, id: &str) {
        let mut wakeups = engine.timer_wakeups.lock().await;
        wakeups
            .get_mut(id)
            .expect("wakeup should be present")
            .next_fire_at = 0;
    }

    async fn insert_goal(engine: &AgentEngine, status: GoalRunStatus) -> String {
        let goal = engine
            .start_goal_run(
                "wakeup lifecycle test goal".to_string(),
                Some("Wakeup lifecycle test".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        let goal_run_id = goal.id.clone();
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs
            .iter_mut()
            .find(|item| item.id == goal_run_id)
            .expect("started goal should be live")
            .status = status;
        goal_run_id
    }

    #[tokio::test]
    async fn single_fire_wakeup_enqueues_continuation_and_clears() {
        let thread_id = "thread-wakeup-single";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 5 * 60_000, 1, "check job progress")
            .await;
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: "glmus".to_string(),
                    responder_stack: vec![ThreadResponderFrame {
                        agent_id: "glmus".to_string(),
                        agent_name: "glmus".to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-glmus".to_string()),
                        linked_thread_id: None,
                    }],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;
        force_due(&engine, &wakeup.id).await;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("wakeup supervision should succeed");

        let continuations = engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await;
        assert_eq!(continuations.len(), 1);
        assert_eq!(
            continuations[0].agent_id, "glmus",
            "ordinary reminders must preserve the explicitly active responder"
        );
        assert!(continuations[0]
            .llm_user_content
            .contains("check job progress"));
        assert!(continuations[0]
            .llm_user_content
            .starts_with("[Scheduled wakeup]"));
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "a single-fire wakeup must be cleared after it fires"
        );
    }

    #[tokio::test]
    async fn repeating_wakeup_reschedules_until_exhausted() {
        let thread_id = "thread-wakeup-repeat";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine.schedule_wakeup(thread_id, 60_000, 2, "poll").await;
        force_due(&engine, &wakeup.id).await;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("first supervision tick should succeed");
        assert_eq!(
            engine.pending_wakeup_count().await,
            1,
            "a repeating wakeup must remain scheduled after its first fire"
        );
        {
            let wakeups = engine.timer_wakeups.lock().await;
            let stored = wakeups.get(&wakeup.id).expect("wakeup should still exist");
            assert_eq!(stored.repetitions_remaining, Some(1));
            assert!(
                stored.next_fire_at > now_millis(),
                "the wakeup must be rescheduled into the future"
            );
        }

        force_due(&engine, &wakeup.id).await;
        engine
            .supervise_timer_wakeups()
            .await
            .expect("second supervision tick should succeed");
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "the wakeup must be cleared after its final repetition"
        );
    }

    #[tokio::test]
    async fn goal_supervision_fires_once_with_reassessment_instruction() {
        let thread_id = "thread-goal-supervision-active";
        let engine = engine_with_thread(thread_id).await;
        let goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        {
            let mut goal_runs = engine.goal_runs.lock().await;
            goal_runs
                .iter_mut()
                .find(|goal| goal.id == goal_run_id)
                .expect("goal")
                .supervision_thread_id = Some(thread_id.to_string());
        }
        let wakeup = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "check progress",
                "goal_supervision",
                Some(&goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: "glmus".to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: "glmus".to_string(),
                            agent_name: "glmus".to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-glmus".to_string()),
                            linked_thread_id: None,
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;
        force_due(&engine, &wakeup.id).await;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("supervision tick");

        let continuations = engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await;
        assert_eq!(continuations.len(), 1);
        assert_eq!(
            continuations[0].agent_id, "glmus",
            "goal supervision must follow the target thread's current responder"
        );
        let prompt = &continuations[0].llm_user_content;
        assert!(prompt.contains(&goal_run_id));
        assert!(prompt.contains("Scheduler-observed status: Running"));
        assert!(prompt.contains("Re-read the current goal status"));
        assert!(prompt.contains("do not schedule another wakeup"));
        assert_eq!(engine.pending_wakeup_count().await, 0);
    }

    #[tokio::test]
    async fn terminal_goal_supervision_is_deleted_without_firing() {
        let thread_id = "thread-goal-supervision-terminal";
        let engine = engine_with_thread(thread_id).await;
        let goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        let wakeup = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "should not fire",
                "goal_supervision",
                Some(&goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");
        {
            let mut goal_runs = engine.goal_runs.lock().await;
            goal_runs
                .iter_mut()
                .find(|goal| goal.id == goal_run_id)
                .expect("goal")
                .status = GoalRunStatus::Completed;
        }
        force_due(&engine, &wakeup.id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("supervision tick");

        assert!(engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await
            .is_empty());
        assert_eq!(engine.pending_wakeup_count().await, 0);
        assert!(!engine
            .history
            .list_agent_wakeups()
            .await
            .expect("wakeups")
            .iter()
            .any(|row| row.id == wakeup.id));
    }

    #[tokio::test]
    async fn cancelling_goal_wakeups_preserves_unrelated_reminders() {
        let thread_id = "thread-goal-wakeup-cleanup";
        let engine = engine_with_thread(thread_id).await;
        let owned_goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        let other_goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        let owned = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "owned",
                "goal_supervision",
                Some(&owned_goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");
        let other = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "other",
                "goal_supervision",
                Some(&other_goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");
        let generic = engine
            .schedule_wakeup(thread_id, 60_000, 1, "generic")
            .await;

        assert_eq!(engine.cancel_goal_wakeups(&owned_goal_run_id).await, 1);
        let wakeups = engine.timer_wakeups.lock().await;
        assert!(!wakeups.contains_key(&owned.id));
        assert!(wakeups.contains_key(&other.id));
        assert!(wakeups.contains_key(&generic.id));
    }

    #[tokio::test]
    async fn goal_supervision_metadata_is_persisted_and_hydrated() {
        let thread_id = "thread-goal-wakeup-persistence";
        let root = tempdir().expect("tempdir should succeed");
        let data_dir = root.path().to_path_buf();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        let wakeup = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "persisted supervision",
                "goal_supervision",
                Some(&goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");

        let row = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("wakeups")
            .into_iter()
            .find(|row| row.id == wakeup.id)
            .expect("persisted wakeup");
        assert_eq!(row.wakeup_kind, "goal_supervision");
        assert_eq!(row.goal_run_id.as_deref(), Some(goal_run_id.as_str()));

        let rehydrated = AgentEngine::new_test(
            SessionManager::new_test(&data_dir).await,
            AgentConfig::default(),
            &data_dir,
        )
        .await;
        rehydrated.hydrate().await.expect("hydrate");
        let wakeups = rehydrated.timer_wakeups.lock().await;
        let restored = wakeups.get(&wakeup.id).expect("restored wakeup");
        assert_eq!(restored.wakeup_kind, "goal_supervision");
        assert_eq!(restored.goal_run_id.as_deref(), Some(goal_run_id.as_str()));
    }

    #[tokio::test]
    async fn missing_or_terminal_goal_supervision_is_rejected_immediately() {
        let engine = engine_with_thread("thread-goal-wakeup-invalid").await;
        let missing = engine
            .schedule_wakeup_with_context(
                "thread-goal-wakeup-invalid",
                60_000,
                1,
                "missing",
                "goal_supervision",
                Some("goal-missing"),
                None,
            )
            .await
            .expect_err("missing goal must be rejected");
        assert!(missing.to_string().contains("missing goal"));

        let terminal_goal_run_id = insert_goal(&engine, GoalRunStatus::Completed).await;
        let terminal = engine
            .schedule_wakeup_with_context(
                "thread-goal-wakeup-invalid",
                60_000,
                1,
                "terminal",
                "goal_supervision",
                Some(&terminal_goal_run_id),
                None,
            )
            .await
            .expect_err("terminal goal must be rejected");
        assert!(terminal.to_string().contains("terminal goal"));
        assert_eq!(engine.pending_wakeup_count().await, 0);
    }

    #[tokio::test]
    async fn repeated_goal_supervision_is_rejected_at_the_engine_boundary() {
        let engine = engine_with_thread("thread-goal-wakeup-infinite").await;
        let error = engine
            .schedule_wakeup_with_context(
                "thread-goal-wakeup-infinite",
                60_000,
                2,
                "do not loop forever",
                "goal_supervision",
                Some("goal-infinite"),
                None,
            )
            .await
            .expect_err("infinite goal supervision must be rejected");
        assert!(error.to_string().contains("exactly one wakeup"));
        assert_eq!(engine.pending_wakeup_count().await, 0);
    }

    #[tokio::test]
    async fn cancel_wakeup_removes_pending_wakeup() {
        let thread_id = "thread-wakeup-cancel";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 60_000, 0, "loop forever")
            .await;
        assert_eq!(engine.pending_wakeup_count().await, 1);
        assert!(engine.cancel_wakeup(&wakeup.id).await);
        assert_eq!(engine.pending_wakeup_count().await, 0);
        assert!(
            !engine.cancel_wakeup(&wakeup.id).await,
            "cancelling an unknown wakeup id is a no-op"
        );
    }

    #[tokio::test]
    async fn cancel_wakeup_clears_memory_when_history_delete_fails() {
        let thread_id = "thread-wakeup-cancel-history-fail";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 60_000, 0, "loop forever")
            .await;
        engine
            .history
            .conn
            .call(|conn| {
                conn.execute("DROP TABLE agent_wakeups", [])?;
                Ok(())
            })
            .await
            .expect("drop wakeups table to force persist failure");

        assert!(
            engine.cancel_wakeup(&wakeup.id).await,
            "a live wakeup must still cancel when persistence fails, otherwise it keeps firing"
        );
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "memory is the live schedule; a history failure must not leave a stale timer"
        );
    }

    #[tokio::test]
    async fn cancel_goal_wakeups_clears_memory_when_history_delete_fails() {
        let thread_id = "thread-goal-wakeup-cancel-history-fail";
        let engine = engine_with_thread(thread_id).await;
        let goal_run_id = insert_goal(&engine, GoalRunStatus::Running).await;
        engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "owned",
                "goal_supervision",
                Some(&goal_run_id),
                None,
            )
            .await
            .expect("valid goal supervision");
        engine
            .history
            .conn
            .call(|conn| {
                conn.execute("DROP TABLE agent_wakeups", [])?;
                Ok(())
            })
            .await
            .expect("drop wakeups table to force persist failure");

        assert_eq!(
            engine.cancel_goal_wakeups(&goal_run_id).await,
            1,
            "goal wakeup cancellation must succeed from memory even if history delete fails"
        );
        assert_eq!(
            engine.pending_wakeup_count().await,
            0,
            "a history failure must not leave supervised goal timers live"
        );
    }

    #[tokio::test]
    async fn wakeup_is_persisted_and_removed_on_cancel() {
        let thread_id = "thread-wakeup-persist";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup(thread_id, 60_000, 1, "persisted check")
            .await;

        let rows = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("listing wakeups should succeed");
        assert!(
            rows.iter()
                .any(|row| row.id == wakeup.id && row.message == "persisted check"),
            "a scheduled wakeup must be persisted to the database"
        );

        assert!(engine.cancel_wakeup(&wakeup.id).await);
        let rows = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("listing wakeups should succeed");
        assert!(
            !rows.iter().any(|row| row.id == wakeup.id),
            "a cancelled wakeup must be deleted from the database"
        );
    }
}

#[cfg(test)]
mod reason_tests {
    use super::*;
    use tempfile::tempdir;

    async fn engine_with_thread(thread_id: &str) -> std::sync::Arc<AgentEngine> {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let now = now_millis();
        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Wakeup".to_string(),
                messages: vec![AgentMessage::user("kick off the job", now)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now,
                updated_at: now,
                total_input_tokens: 0,
                total_output_tokens: 0,
            },
        );
        engine
    }

    #[tokio::test]
    async fn fired_wakeup_tells_agent_why_it_was_scheduled_and_how_to_cancel() {
        let thread_id = "thread-wakeup-reason";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                2,
                "poll",
                "generic",
                None,
                Some("waiting for the release build to finish"),
            )
            .await
            .expect("generic wakeup is valid");
        let rows = engine
            .history
            .list_agent_wakeups()
            .await
            .expect("listing wakeups should succeed");
        assert_eq!(
            rows.iter().find(|row| row.id == wakeup.id).and_then(|row| row.reason.as_deref()),
            Some("waiting for the release build to finish"),
            "the reason must survive a daemon restart"
        );
        engine.timer_wakeups.lock().await.get_mut(&wakeup.id).unwrap().next_fire_at = 0;
        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;

        engine
            .supervise_timer_wakeups()
            .await
            .expect("wakeup supervision should succeed");

        let continuations = engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await;
        let content = &continuations[0].llm_user_content;
        assert!(
            content.contains("Scheduled because: waiting for the release build to finish"),
            "a woken agent that lost context to compaction must learn why it was woken: {content}"
        );
        assert!(
            content.contains(&format!("cancel_wakeup(wakeup_id=\"{}\")", wakeup.id)),
            "a repeating wakeup must tell the agent how to stop it: {content}"
        );
    }

    #[tokio::test]
    async fn compaction_payload_keeps_pending_wakeups_and_running_subagents() {
        let thread_id = "thread-wakeup-compaction";
        let engine = engine_with_thread(thread_id).await;
        let wakeup = engine
            .schedule_wakeup_with_context(
                thread_id,
                60_000,
                1,
                "check the deploy",
                "generic",
                None,
                Some("deploy takes ten minutes"),
            )
            .await
            .expect("generic wakeup is valid");
        let task = |id: &str, status: &str| -> AgentTask {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "title": format!("worker {id}"),
                "description": format!("investigate flaky test for {id}"),
                "status": status,
                "created_at": 1,
                "source": "subagent",
                "parent_thread_id": thread_id,
            }))
            .expect("task fixture should deserialize")
        };
        {
            let mut tasks = engine.tasks.lock().await;
            tasks.push_back(task("task-running", "in_progress"));
            tasks.push_back(task("task-done", "completed"));
        }

        let payload = engine
            .append_pending_runtime_section(thread_id, "checkpoint".to_string())
            .await;

        assert!(payload.contains(&wakeup.id), "{payload}");
        assert!(payload.contains("reason: deploy takes ten minutes"), "{payload}");
        assert!(
            payload.contains("investigate flaky test for task-running"),
            "running subagents and why they were spawned must survive compaction: {payload}"
        );
        assert!(
            !payload.contains("task-done"),
            "finished subagents are not pending work: {payload}"
        );
    }
}
