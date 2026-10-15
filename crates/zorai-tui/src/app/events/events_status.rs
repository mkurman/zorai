use super::*;

impl TuiModel {
    fn replace_recent_actions_from_status_snapshot(&mut self, recent_actions_json: &str) {
        let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(recent_actions_json)
        else {
            return;
        };

        let mut parsed_actions: Vec<super::RecentActionVm> = actions
            .into_iter()
            .filter_map(|action| {
                let summary = action.get("summary").and_then(|value| value.as_str())?;
                Some(super::RecentActionVm {
                    action_type: action
                        .get("action_type")
                        .and_then(|value| value.as_str())
                        .unwrap_or("activity")
                        .to_string(),
                    summary: summary.to_string(),
                    timestamp: action
                        .get("timestamp")
                        .and_then(|value| value.as_u64())
                        .unwrap_or_default(),
                })
            })
            .collect();

        parsed_actions.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| left.summary.cmp(&right.summary))
        });
        parsed_actions.truncate(3);
        self.recent_actions = parsed_actions;
    }

    pub(in crate::app) fn handle_status_snapshot_event(
        &mut self,
        snapshot: crate::client::AgentStatusSnapshotVm,
    ) {
        self.replace_recent_actions_from_status_snapshot(&snapshot.recent_actions_json);
        self.status_modal_snapshot = Some(snapshot);
        self.status_modal_loading = false;
        self.status_modal_error = None;
        self.status_modal_scroll = 0;
    }

    pub(in crate::app) fn handle_statistics_snapshot_event(
        &mut self,
        snapshot: zorai_protocol::AgentStatisticsSnapshot,
    ) {
        self.statistics_modal_snapshot = Some(snapshot);
        self.statistics_modal_loading = false;
        self.statistics_modal_error = None;
        self.statistics_modal_scroll = 0;
    }

    pub(in crate::app) fn handle_prompt_inspection_event(
        &mut self,
        prompt: crate::client::AgentPromptInspectionVm,
    ) {
        self.prompt_modal_snapshot = Some(prompt);
        self.prompt_modal_loading = false;
        self.prompt_modal_error = None;
        self.prompt_modal_scroll = 0;
    }

    pub(in crate::app) fn handle_agent_config_event(
        &mut self,
        cfg: crate::wire::AgentConfigSnapshot,
    ) {
        let before_profile = self.current_conversation_agent_profile();
        self.config.reduce(config::ConfigAction::ConfigReceived(
            config::AgentConfigSnapshot {
                provider: cfg.provider,
                base_url: cfg.base_url,
                model: cfg.model,
                custom_model_name: String::new(),
                api_key: cfg.api_key,
                assistant_id: cfg.assistant_id,
                auth_source: cfg.auth_source,
                api_transport: cfg.api_transport,
                reasoning_effort: cfg.reasoning_effort,
                context_window_tokens: cfg.context_window_tokens,
            },
        ));
        self.reapply_pending_svarog_reasoning_effort();
        self.invalidate_active_header_runtime_profile_if_profile_changed(&before_profile);
    }

    pub(in crate::app) fn handle_agent_config_raw_event(&mut self, raw: serde_json::Value) {
        let before_profile = self.current_conversation_agent_profile();
        let was_loaded = self.agent_config_loaded;
        tracing::info!(
            was_loaded,
            connected = self.connected,
            "app: handle_agent_config_raw_event entered"
        );
        let apply_started = std::time::Instant::now();
        self.apply_config_json(&raw);
        tracing::info!(
            apply_ms = apply_started.elapsed().as_millis() as u64,
            "app: apply_config_json done"
        );
        self.reconcile_pending_svarog_reasoning_effort_after_raw_config();
        if let Some(concierge) = raw.get("concierge").filter(|value| value.is_object()) {
            self.handle_concierge_config_event(concierge.clone());
        }
        self.chat
            .set_history_page_size(self.config.tui_chat_history_page_size as usize);
        self.invalidate_active_header_runtime_profile_if_profile_changed(&before_profile);
        self.agent_config_loaded = true;
        if self.connected && !was_loaded {
            tracing::info!(
                "app: agent_config_loaded transitioning false→true; firing startup cascade"
            );
            let restored_thread = self.begin_pending_reconnect_restore();
            if !restored_thread {
                self.request_concierge_welcome();
            }
            self.maybe_request_operator_profile_autostart_summary();
            self.send_daemon_command(DaemonCommand::RefreshServices);
            self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
            self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
            self.send_daemon_command(DaemonCommand::ListSubAgents);
            self.send_daemon_command(DaemonCommand::GetConciergeConfig);
            self.send_daemon_command(DaemonCommand::ListNotifications);
            self.send_daemon_command(DaemonCommand::ListTaskApprovalRules);
            self.send_daemon_command(DaemonCommand::PluginList);
            self.send_daemon_command(DaemonCommand::PluginListCommands);
            self.send_daemon_command(DaemonCommand::GetMlflowTracingStatus);
            tracing::info!("app: startup cascade dispatched");
        } else {
            tracing::info!(
                connected = self.connected,
                was_loaded,
                "app: cascade SKIPPED (either already loaded or not connected)"
            );
        }
    }

    pub(in crate::app) fn handle_models_fetched_event(
        &mut self,
        models: Vec<crate::wire::FetchedModel>,
    ) {
        let models = models
            .into_iter()
            .map(|model| config::FetchedModel {
                id: model.id,
                name: model.name,
                context_window: model.context_window,
                pricing: model.pricing.map(|pricing| config::FetchedModelPricing {
                    prompt: pricing.prompt,
                    completion: pricing.completion,
                    image: pricing.image,
                    request: pricing.request,
                    web_search: pricing.web_search,
                    internal_reasoning: pricing.internal_reasoning,
                    input_cache_read: pricing.input_cache_read,
                    input_cache_write: pricing.input_cache_write,
                    audio: pricing.audio,
                }),
                metadata: model.metadata,
            })
            .collect();
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.modal.top() == Some(crate::state::modal::ModalKind::ModelPicker) {
            self.sync_model_picker_item_count();
        }
    }

    pub(in crate::app) fn handle_heartbeat_items_event(
        &mut self,
        items: Vec<crate::wire::HeartbeatItem>,
    ) {
        let items = items
            .into_iter()
            .map(conversion::convert_heartbeat)
            .collect();
        self.tasks
            .reduce(task::TaskAction::HeartbeatItemsReceived(items));
    }

    pub(in crate::app) fn handle_heartbeat_digest_event(
        &mut self,
        cycle_id: String,
        actionable: bool,
        digest: String,
        items: Vec<(u8, String, String, String)>,
        checked_at: u64,
        explanation: Option<String>,
    ) {
        let vm_items: Vec<task::HeartbeatDigestItemVm> = items
            .into_iter()
            .map(
                |(priority, check_type, title, suggestion)| task::HeartbeatDigestItemVm {
                    priority,
                    check_type,
                    title,
                    suggestion,
                },
            )
            .collect();
        let item_count = vm_items.len();

        for item in &vm_items {
            self.recent_actions.push(super::RecentActionVm {
                action_type: item.check_type.clone(),
                summary: item.title.clone(),
                timestamp: checked_at,
            });
        }
        if self.recent_actions.len() > 3 {
            let start = self.recent_actions.len() - 3;
            self.recent_actions = self.recent_actions.split_off(start);
        }

        self.tasks.reduce(task::TaskAction::HeartbeatDigestReceived(
            task::HeartbeatDigestVm {
                cycle_id,
                actionable,
                digest: digest.clone(),
                items: vm_items,
                checked_at,
                explanation,
            },
        ));
        if actionable && item_count > 0 {
            self.status_line = format!("♥ Heartbeat: {}", digest);
        }
    }

    pub(in crate::app) fn handle_notification_snapshot_event(
        &mut self,
        notifications: Vec<zorai_protocol::InboxNotification>,
    ) {
        self.notifications
            .reduce(crate::state::NotificationsAction::Replace(notifications));
    }

    pub(in crate::app) fn handle_notification_upsert_event(
        &mut self,
        notification: zorai_protocol::InboxNotification,
    ) {
        let previous_unread = self.notifications.unread_count();
        let title = notification.title.clone();
        self.notifications
            .reduce(crate::state::NotificationsAction::Upsert(notification));
        let unread = self.notifications.unread_count();
        if unread > previous_unread {
            self.status_line = format!("🔔 {}", title);
        }
    }

    pub(in crate::app) fn handle_audit_entry_event(
        &mut self,
        id: String,
        timestamp: u64,
        action_type: String,
        summary: String,
        explanation: Option<String>,
        confidence: Option<f64>,
        confidence_band: Option<String>,
        causal_trace_id: Option<String>,
        thread_id: Option<String>,
    ) {
        self.audit
            .reduce(crate::state::audit::AuditAction::EntryReceived(
                crate::state::audit::AuditEntryVm {
                    id,
                    timestamp,
                    action_type,
                    summary,
                    explanation,
                    confidence,
                    confidence_band,
                    causal_trace_id,
                    thread_id,
                    dismissed: false,
                },
            ));
    }

    pub(in crate::app) fn handle_escalation_update_event(
        &mut self,
        thread_id: String,
        from_level: String,
        to_level: String,
        reason: String,
        attempts: u32,
        audit_id: Option<String>,
    ) {
        self.status_line = format!("Escalating: {}->{} {}", from_level, to_level, reason);
        self.audit
            .reduce(crate::state::audit::AuditAction::EscalationUpdate(
                crate::state::audit::EscalationVm {
                    thread_id,
                    from_level,
                    to_level,
                    reason,
                    attempts,
                    audit_id,
                },
            ));
    }

    pub(in crate::app) fn handle_anticipatory_items_event(
        &mut self,
        _items: Vec<crate::wire::AnticipatoryItem>,
    ) {
    }

    pub(in crate::app) fn handle_gateway_status_event(
        &mut self,
        platform: String,
        status: String,
        last_error: Option<String>,
        consecutive_failures: u32,
    ) {
        let status_changed = self
            .gateway_statuses
            .iter()
            .find(|g| g.platform == platform)
            .is_none_or(|existing| existing.status != status || existing.last_error != last_error);
        let vm = chat::GatewayStatusVm {
            platform: platform.clone(),
            status: status.clone(),
            last_error,
            consecutive_failures,
        };
        if let Some(existing) = self
            .gateway_statuses
            .iter_mut()
            .find(|g| g.platform == platform)
        {
            *existing = vm;
        } else {
            self.gateway_statuses.push(vm);
        }
        if status_changed {
            self.status_line = format!("🌐 Gateway {}: {}", platform, status);
        }
    }

    pub(in crate::app) fn handle_whatsapp_link_status_event(
        &mut self,
        state: String,
        phone: Option<String>,
        last_error: Option<String>,
    ) {
        tracing::info!(
            state = %state,
            phone = phone.as_deref().unwrap_or(""),
            has_last_error = last_error.is_some(),
            "tui received whatsapp link status"
        );
        self.modal
            .set_whatsapp_link_status(&state, phone.clone(), last_error.clone());
        self.status_line = match state.as_str() {
            "connected" => {
                format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"))
            }
            "error" => format!(
                "WhatsApp link error: {}",
                last_error.as_deref().unwrap_or("unknown")
            ),
            "disconnected" => format!(
                "WhatsApp link disconnected: {}",
                last_error.as_deref().unwrap_or("none")
            ),
            "qr_ready" | "awaiting_qr" => "WhatsApp link awaiting QR scan".to_string(),
            "starting" => "WhatsApp link starting".to_string(),
            _ => "WhatsApp link status updated".to_string(),
        };
    }

    pub(in crate::app) fn handle_whatsapp_link_qr_event(
        &mut self,
        ascii_qr: String,
        expires_at_ms: Option<u64>,
    ) {
        tracing::info!(
            qr_len = ascii_qr.len(),
            expires_at_ms,
            "tui received whatsapp link qr"
        );
        self.modal.set_whatsapp_link_qr(ascii_qr, expires_at_ms);
        if self.modal.top() != Some(crate::state::modal::ModalKind::WhatsAppLink) {
            self.modal.reduce(crate::state::modal::ModalAction::Push(
                crate::state::modal::ModalKind::WhatsAppLink,
            ));
        }
        self.status_line = "WhatsApp QR ready — scan with your phone".to_string();
    }

    pub(in crate::app) fn handle_whatsapp_linked_event(&mut self, phone: Option<String>) {
        tracing::info!(
            phone = phone.as_deref().unwrap_or(""),
            "tui received whatsapp linked event"
        );
        self.modal.set_whatsapp_link_connected(phone.clone());
        self.status_line = format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"));
    }

    pub(in crate::app) fn handle_whatsapp_link_error_event(&mut self, message: String) {
        tracing::warn!(message = %message, "tui received whatsapp link error");
        self.modal.set_whatsapp_link_error(message.clone());
        if self.modal.top() != Some(crate::state::modal::ModalKind::WhatsAppLink) {
            self.modal.reduce(crate::state::modal::ModalAction::Push(
                crate::state::modal::ModalKind::WhatsAppLink,
            ));
        }
        self.status_line = format!("WhatsApp link error: {message}");
    }

    pub(in crate::app) fn handle_whatsapp_link_disconnected_event(
        &mut self,
        reason: Option<String>,
    ) {
        tracing::info!(
            reason = reason.as_deref().unwrap_or(""),
            "tui received whatsapp link disconnected"
        );
        self.modal.set_whatsapp_link_disconnected(reason.clone());
        let display_reason = self
            .modal
            .whatsapp_link()
            .last_error()
            .map(str::to_string)
            .or(reason.clone())
            .unwrap_or_else(|| "none".to_string());
        self.status_line = format!("WhatsApp link disconnected: {}", display_reason);
    }

    pub(in crate::app) fn handle_tier_changed_event(&mut self, new_tier: String) {
        self.tier.on_tier_changed(&new_tier);
        self.status_line = format!("Tier: {}", new_tier);
    }
}
