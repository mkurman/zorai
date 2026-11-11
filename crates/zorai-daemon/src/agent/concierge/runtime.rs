use super::*;

impl ConciergeEngine {
    pub fn new(
        config: Arc<RwLock<AgentConfig>>,
        event_tx: broadcast::Sender<AgentEvent>,
        http_client: reqwest::Client,
        circuit_breakers: Arc<CircuitBreakerRegistry>,
    ) -> Self {
        Self {
            config,
            event_tx,
            http_client,
            circuit_breakers,
            welcome_cache: Arc::new(RwLock::new(None)),
            recovery_investigations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn maybe_start_recovery_investigation(
        &self,
        agent: &super::super::AgentEngine,
        thread_id: &str,
        signature: &str,
        failure_class: &str,
        summary: &str,
        diagnostics: &Value,
    ) -> Option<String> {
        let key = format!("{thread_id}::{signature}");

        {
            let mut investigations = self.recovery_investigations.lock().await;
            if let Some(existing_task_id) = investigations.get(&key).cloned() {
                let still_running = agent
                    .count_tasks_filtered(&crate::history::AgentTaskListQuery {
                        id: Some(existing_task_id),
                        status: None,
                        statuses: Vec::new(),
                        source: None,
                        thread_id: None,
                        thread_ids: Vec::new(),
                        goal_run_id: None,
                        parent_task_id: None,
                        awaiting_approval_id: None,
                        supervisor_config_present: false,
                        exclude_terminal_statuses: true,
                        order_by_recent_activity_desc: false,
                        limit: Some(1),
                        ids: Vec::new(),
                        parent_task_ids: Vec::new(),
                    })
                    .await
                    > 0;
                if still_running {
                    return None;
                }
                investigations.remove(&key);
            }
        }

        let task = agent
            .enqueue_task(
                format!("Investigate daemon recovery: {signature}"),
                build_recovery_investigation_description(
                    thread_id,
                    signature,
                    failure_class,
                    summary,
                    diagnostics,
                ),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "concierge_recovery",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let task = agent.retarget_task_to_weles(&task.id).await.unwrap_or(task);

        self.recovery_investigations
            .lock()
            .await
            .insert(key, task.id.clone());

        Some(task.id)
    }

    pub async fn initialize(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            thread.agent_name = Some(CONCIERGE_AGENT_NAME.to_string());
            let now = super::super::now_millis();
            let recent_welcome = thread
                .messages
                .iter()
                .rev()
                .find(|msg| {
                    msg.role == MessageRole::Assistant
                        && msg.provider.as_deref() == Some("concierge")
                        && now.saturating_sub(msg.timestamp) < WELCOME_REUSE_WINDOW_MS
                })
                .cloned();
            thread.messages.clear();
            if let Some(welcome) = recent_welcome {
                thread.messages.push(welcome);
                tracing::info!("concierge: preserved recent welcome message in existing thread");
            } else {
                tracing::info!("concierge: cleared stale messages from existing thread");
            }
        } else {
            let now = super::super::now_millis();
            let thread = AgentThread {
                id: CONCIERGE_THREAD_ID.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Concierge".to_string(),
                created_at: now,
                updated_at: now,
                messages: Vec::new(),
                pinned: true,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
            };
            threads_guard.insert(CONCIERGE_THREAD_ID.to_string(), thread);
            tracing::info!("concierge: created pinned thread");
        }
    }

    pub async fn on_client_connected(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        history: &crate::history::HistoryStore,
    ) {
        self.on_client_connected_with_persisted_threads(threads, history, &[])
            .await;
    }

    pub(crate) async fn on_client_connected_with_persisted_threads(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        history: &crate::history::HistoryStore,
        persisted_recent_threads: &[ThreadSummary],
    ) {
        tracing::info!("concierge: on_client_connected called");
        let config = self.config.read().await;
        if !config.concierge.enabled {
            tracing::info!("concierge: disabled in config, skipping");
            return;
        }

        let detail_level = config.concierge.detail_level;
        tracing::info!("concierge: gathering context at level {:?}", detail_level);
        drop(config);

        let context = self
            .gather_context(history, detail_level, persisted_recent_threads)
            .await;
        tracing::info!(
            "concierge: gathered {} threads, latest_goal={}, running_goals={}, paused_goals={}",
            context.recent_threads.len(),
            context.latest_goal_run.is_some(),
            context.running_goal_total,
            context.paused_goal_total
        );
        let reused = self
            .reuse_welcome_from_history(threads, history, detail_level, &context)
            .await;
        let is_reused = reused.is_some();
        let (content, actions) = if let Some(existing) = reused {
            tracing::info!("concierge: reusing persisted welcome payload");
            existing
        } else {
            self.compose_welcome(detail_level, &context).await
        };

        if content.is_empty() {
            tracing::warn!("concierge: empty welcome content, skipping emit");
            return;
        }

        if !is_reused {
            self.replace_welcome_message(threads, &content).await;
        }

        let send_result = self.event_tx.send(AgentEvent::ConciergeWelcome {
            thread_id: CONCIERGE_THREAD_ID.to_string(),
            content,
            detail_level,
            actions,
        });
        tracing::info!(
            "concierge: ConciergeWelcome event emitted, receivers={}",
            send_result.unwrap_or(0)
        );
    }

    pub async fn generate_welcome(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        history: &crate::history::HistoryStore,
    ) -> Option<(String, ConciergeDetailLevel, Vec<ConciergeAction>)> {
        let config = self.config.read().await;
        if !config.concierge.enabled {
            tracing::info!("concierge: disabled, skipping generate_welcome");
            return None;
        }
        let detail_level = config.concierge.detail_level;
        drop(config);

        let context = self.gather_context(history, detail_level, &[]).await;
        let reused = self
            .reuse_welcome_from_history(threads, history, detail_level, &context)
            .await;
        let is_reused = reused.is_some();
        let (content, actions) = if let Some(existing) = reused {
            existing
        } else {
            self.compose_welcome(detail_level, &context).await
        };

        if content.is_empty() {
            return None;
        }

        if !is_reused {
            self.replace_welcome_message(threads, &content).await;
        }
        Some((content, detail_level, actions))
    }

    pub async fn prune_welcome_messages(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            let before = thread.messages.len();
            thread.messages.retain(|msg| {
                !(msg.role == MessageRole::Assistant
                    && msg.provider.as_deref() == Some("concierge"))
            });
            if thread.messages.len() != before {
                thread.updated_at = super::super::now_millis();
            }
        }
        *self.welcome_cache.write().await = None;
    }

    pub(super) async fn replace_welcome_message(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        content: &str,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            thread.messages.clear();
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: content.to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: Some("concierge".into()),
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: super::super::now_millis(),
                feedback: None,
            });
            thread.updated_at = super::super::now_millis();
        }
    }

    pub(super) async fn reuse_welcome_from_history(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        history: &crate::history::HistoryStore,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> Option<(String, Vec<ConciergeAction>)> {
        let latest_welcome = {
            let threads_guard = threads.read().await;
            let concierge_thread = threads_guard.get(CONCIERGE_THREAD_ID)?;
            concierge_thread
                .messages
                .iter()
                .rev()
                .find(|msg| {
                    msg.role == MessageRole::Assistant
                        && msg.provider.as_deref() == Some("concierge")
                })
                .cloned()?
        };

        let now = super::super::now_millis();
        if now.saturating_sub(latest_welcome.timestamp) >= WELCOME_REUSE_WINDOW_MS {
            return None;
        }

        let history_says_active = match history
            .has_non_heartbeat_user_message_after(latest_welcome.timestamp)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    "failed to query user activity after concierge welcome from history: {error}"
                );
                false
            }
        };
        // Consult the live in-memory threads as well as history. The live
        // queue is the source of truth for messages that arrived this
        // session; history catches up via background persistence and may
        // lag. Without this OR-with-memory check a fresh user message
        // that hasn't been flushed yet gets ignored and we'd reuse a
        // stale welcome despite obvious user activity.
        let memory_says_active = {
            let threads_guard = threads.read().await;
            threads_guard
                .values()
                .filter(|thread| !is_heartbeat_thread(thread))
                .flat_map(|thread| thread.messages.iter())
                .any(|msg| {
                    msg.role == MessageRole::User && msg.timestamp > latest_welcome.timestamp
                })
        };
        let has_user_message_after_welcome = history_says_active || memory_says_active;
        if has_user_message_after_welcome {
            return None;
        }

        Some((
            latest_welcome.content.clone(),
            self.build_welcome_actions(detail_level, context),
        ))
    }

    pub(super) async fn cached_welcome(
        &self,
        signature: &str,
    ) -> Option<(String, Vec<ConciergeAction>)> {
        let cache = self.welcome_cache.read().await;
        let entry = cache.as_ref()?;
        if entry.signature != signature || entry.created_at.elapsed() > WELCOME_CACHE_TTL {
            return None;
        }
        Some((entry.content.clone(), entry.actions.clone()))
    }

    pub(super) async fn cache_welcome(
        &self,
        signature: &str,
        content: &str,
        actions: &[ConciergeAction],
    ) {
        *self.welcome_cache.write().await = Some(WelcomeCacheEntry {
            signature: signature.to_string(),
            content: content.to_string(),
            actions: actions.to_vec(),
            created_at: std::time::Instant::now(),
        });
    }
}

impl super::super::AgentEngine {
    pub(crate) async fn request_concierge_welcome(&self) {
        let request_started = std::time::Instant::now();
        let (onboarding_done, tier) = {
            let cfg = self.config.read().await;
            if cfg.provider.trim().is_empty() || cfg.model.trim().is_empty() {
                tracing::info!("concierge.welcome: skipped until provider and model are set up");
                return;
            }
            let done = cfg.tier.onboarding_completed;
            let tier = cfg
                .tier
                .user_self_assessment
                .unwrap_or(crate::agent::capability_tier::CapabilityTier::Newcomer);
            (done, tier)
        };

        let recent_threads_started = std::time::Instant::now();
        let recent_history_threads = self
            .concierge
            .recent_persisted_history_threads(&self.session_manager)
            .await;
        tracing::info!(
            elapsed_ms = recent_threads_started.elapsed().as_millis() as u64,
            count = recent_history_threads.len(),
            "concierge.welcome: recent_persisted_history_threads"
        );
        let should_deliver_onboarding = !onboarding_done && recent_history_threads.is_empty();

        if !onboarding_done && !should_deliver_onboarding {
            let mut cfg = self.config.write().await;
            cfg.tier.onboarding_completed = true;
        }

        if should_deliver_onboarding {
            let onboarding_started = std::time::Instant::now();
            if let Err(error) = self.concierge.deliver_onboarding(tier, &self.threads).await {
                tracing::warn!(
                    "onboarding delivery failed, falling back to generic welcome: {error}"
                );
            } else {
                self.persist_thread_by_id(CONCIERGE_THREAD_ID).await;
                let mut cfg = self.config.write().await;
                cfg.tier.onboarding_completed = true;
                tracing::info!(
                    elapsed_ms = request_started.elapsed().as_millis() as u64,
                    onboarding_ms = onboarding_started.elapsed().as_millis() as u64,
                    "concierge.welcome: onboarding delivered"
                );
                return;
            }

            let mut cfg = self.config.write().await;
            cfg.tier.onboarding_completed = true;
        }

        let connect_started = std::time::Instant::now();
        self.concierge
            .on_client_connected_with_persisted_threads(
                &self.threads,
                &self.history,
                &recent_history_threads,
            )
            .await;
        tracing::info!(
            elapsed_ms = connect_started.elapsed().as_millis() as u64,
            "concierge.welcome: on_client_connected_with_persisted_threads"
        );

        let persist_started = std::time::Instant::now();
        self.persist_thread_by_id(CONCIERGE_THREAD_ID).await;
        tracing::info!(
            elapsed_ms = persist_started.elapsed().as_millis() as u64,
            total_ms = request_started.elapsed().as_millis() as u64,
            "concierge.welcome: persisted (request done)"
        );
    }
}
