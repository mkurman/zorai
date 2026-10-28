use super::*;

mod events_activity;
mod events_audio;
mod events_connection;
mod events_dispatch_auth_agents_plugins_status;
mod events_dispatch_lifecycle_thread;
mod events_dispatch_media_activity_stream;
mod events_dispatch_workspace_goal_context;
mod events_integrations;
mod events_status;
mod events_tasks;

pub struct DaemonPumpOutcome {
    pub processed: usize,
    pub stopped_for_render: bool,
}

impl TuiModel {
    pub(crate) fn is_internal_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
        let normalized_id = thread_id.trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("dm:") || normalized_title.starts_with("internal dm")
    }

    pub(crate) fn is_hidden_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
        let normalized_id = thread_id.trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("handoff:")
            || normalized_id.starts_with("playground:")
            || normalized_title.starts_with("handoff ")
            || normalized_title.starts_with("participant playground ")
            || normalized_title == "weles"
            || normalized_title.starts_with("weles ")
    }

    fn should_ignore_internal_thread_activity(&self, thread_id: &str) -> bool {
        Self::is_internal_agent_thread(thread_id, None)
            && self.chat.active_thread_id() != Some(thread_id)
    }

    fn sync_open_thread_picker(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::ThreadPicker) {
            self.sync_thread_picker_item_count();
        }
    }

    pub fn pump_daemon_events_budgeted(&mut self, limit: usize) -> DaemonPumpOutcome {
        let mut processed = 0;
        let mut stopped_for_render = false;
        while processed < limit {
            match self.daemon_events_rx.try_recv() {
                Ok(event) => {
                    let streaming_content_len_before = self.chat.streaming_content().len();
                    let streaming_reasoning_len_before = self.chat.streaming_reasoning().len();
                    self.handle_client_event(event);
                    processed += 1;
                    let streaming_grew = self.chat.streaming_content().len()
                        > streaming_content_len_before
                        || self.chat.streaming_reasoning().len() > streaming_reasoning_len_before;
                    if streaming_grew {
                        stopped_for_render = true;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        DaemonPumpOutcome {
            processed,
            stopped_for_render,
        }
    }

    #[cfg(test)]
    pub fn pump_daemon_events(&mut self) {
        let _ = self.pump_daemon_events_budgeted(usize::MAX);
    }

    #[cfg(test)]
    pub fn on_tick(&mut self) -> bool {
        self.on_tick_elapsed(1)
    }

    pub(crate) fn on_tick_elapsed(&mut self, elapsed_ticks: u64) -> bool {
        let render_revision_before = self.chat.render_revision();
        let tick_driven_render_before = self.tick_driven_render_epoch(self.tick_counter);
        let pending_stop_before = self.pending_stop;
        let voice_player_active_before = self.voice_player.is_some();
        let input_notice_active_before = self.input_notice.is_some();
        let system_monitor_before = self.system_monitor;
        let image_preview_cache_revision_before = self.image_preview_cache_revision;
        let status_line_before = self.status_line.clone();
        let hidden_auto_response_count_before = self.hidden_auto_response_suggestion_ids.len();

        self.tick_counter = self.tick_counter.saturating_add(elapsed_ticks.max(1));
        self.maybe_refresh_system_monitor();
        self.chat.clear_expired_copy_feedback(self.tick_counter);
        self.maybe_request_older_chat_history();
        self.maybe_request_older_goal_run_history();
        self.maybe_refresh_spawned_sidebar_tasks();
        self.maybe_auto_refresh_visible_work();
        self.maybe_schedule_chat_history_collapse();
        self.chat.maybe_collapse_history(self.tick_counter);
        let queued_prompt_copy_feedback_changed = self.clear_expired_queued_prompt_copy_feedback();

        if let Some(player) = self.voice_player.as_mut() {
            match player.try_wait() {
                Ok(Some(_status)) => {
                    self.voice_player = None;
                    self.clear_voice_player_ipc();
                    if self.status_line.starts_with("Playing speech")
                        || self.status_line == "Playing synthesized speech..."
                        || self.status_line == "Speech paused"
                        || self.status_line == "Speech resumed"
                    {
                        self.status_line = "Audio playback finished".to_string();
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.voice_player = None;
                    self.clear_voice_player_ipc();
                    self.status_line = "Audio playback process error".to_string();
                    self.last_error = Some(format!("Audio playback monitor failed: {error}"));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                }
            }
        }

        let _ = self.maybe_auto_send_always_auto_response();
        if self
            .active_auto_response_countdown_secs()
            .is_some_and(|remaining| remaining == 0)
            && !self.assistant_busy()
        {
            let _ = self.execute_active_auto_response_action(AutoResponseActionSelection::Yes);
        }
        if self.pending_stop && !self.pending_stop_active() {
            self.pending_stop = false;
        }
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| self.tick_counter >= notice.expires_at_tick)
        {
            self.input_notice = None;
        }
        self.publish_attention_surface_if_changed();
        self.image_preview_cache_revision = widgets::image_preview::preview_cache_revision();

        self.chat.render_revision() != render_revision_before
            || tick_driven_render_before != self.tick_driven_render_epoch(self.tick_counter)
            || self.pending_stop != pending_stop_before
            || self.voice_player.is_some() != voice_player_active_before
            || self.input_notice.is_some() != input_notice_active_before
            || self.system_monitor != system_monitor_before
            || self.image_preview_cache_revision != image_preview_cache_revision_before
            || self.status_line != status_line_before
            || self.hidden_auto_response_suggestion_ids.len() != hidden_auto_response_count_before
            || queued_prompt_copy_feedback_changed
    }

    pub(crate) fn dispatch_due_thread_picker_refresh(&mut self, now: Instant) -> bool {
        if self.modal.top() != Some(modal::ModalKind::ThreadPicker) {
            self.pending_thread_picker_refresh = None;
            self.thread_picker_loading_tab = None;
            return false;
        }

        let Some(pending) = self.pending_thread_picker_refresh.clone() else {
            return false;
        };

        if now < pending.ready_at {
            return false;
        }

        if self.modal.thread_picker_tab() != pending.tab {
            self.pending_thread_picker_refresh = None;
            return false;
        }

        self.pending_thread_picker_refresh = None;
        self.thread_picker_loading_tab = Some(pending.tab);
        self.send_daemon_command(DaemonCommand::RefreshThreadsForAgent {
            agent_filter: pending.agent_filter,
        });
        self.status_line = "Refreshing threads…".to_string();
        true
    }

    pub(crate) fn wants_fast_tick(&self) -> bool {
        self.chat.is_streaming() || self.tick_driven_render_epoch(self.tick_counter).is_some()
    }

    fn tick_driven_render_epoch(&self, tick: u64) -> Option<u64> {
        let mut epoch = None;
        let mut include = |value: u64| {
            epoch = Some(epoch.map_or(value, |current: u64| current.max(value)));
        };

        if self.should_show_daemon_connection_loading()
            || self.should_show_concierge_hero_loading()
            || self.should_show_thread_loading()
        {
            include(tick / 3);
        }

        if self.footer_activity_text().is_some() {
            include(tick / 2);
        }

        if self.chat.retry_status().is_some()
            || self.active_auto_response_countdown_secs().is_some()
        {
            let ticks_per_second = (1_000 / TUI_TICK_RATE_MS).max(1);
            include(tick / ticks_per_second);
        }

        if self.chat.active_thread_older_page_pending() {
            include(tick / 3);
        }

        // Static thread timestamps must not schedule redraws. `format_time_ago`
        // can tolerate being stale until the next real UI/event update; the old
        // condition below repainted the entire TUI once per second forever as
        // soon as any selected thread had a nonzero creation timestamp.

        if self.voice_recording || self.voice_player.is_some() {
            include(tick / 8);
        }

        if self.error_active {
            include(tick / 10);
        }

        // Goal progress is event-driven. Do not animate the entire goal screen:
        // every spinner frame previously rebuilt and repainted a large Ratatui
        // buffer four times per second even when no goal state had changed.
        // Real goal/task daemon updates already request a redraw.

        epoch
    }

    fn maybe_refresh_spawned_sidebar_tasks(&mut self) {
        let Some(active_thread_id) = self.chat.active_thread_id() else {
            return;
        };
        if self.thread_loading_id.is_some() {
            return;
        }
        if self.sidebar.active_tab() != sidebar::SidebarTab::Spawned {
            return;
        }
        if !widgets::sidebar::has_spawned_tab(&self.tasks, &self.chat, Some(active_thread_id)) {
            return;
        }
        if self.tick_counter < self.next_spawned_sidebar_task_refresh_tick {
            return;
        }

        self.send_daemon_command(DaemonCommand::ListTasks);
        self.next_spawned_sidebar_task_refresh_tick = self
            .tick_counter
            .saturating_add(SPAWNED_SIDEBAR_TASK_REFRESH_TICKS);
    }

    fn maybe_refresh_system_monitor(&mut self) {
        if self.tick_counter < self.next_system_monitor_tick {
            return;
        }

        let ticks_per_second = (1_000 / TUI_TICK_RATE_MS).max(1);
        self.next_system_monitor_tick = self.tick_counter.saturating_add(ticks_per_second * 3);
        if let Some(snapshot) = self.system_monitor_sampler.sample() {
            self.system_monitor = Some(snapshot);
        }
    }

    fn auto_refresh_interval_ticks(&self) -> Option<u64> {
        let secs = self.config.auto_refresh_interval_secs;
        if secs == 0 {
            return None;
        }
        Some(((secs as u64 * 1000) / TUI_TICK_RATE_MS).max(1))
    }

    fn active_auto_refresh_target(&self) -> Option<AutoRefreshTarget> {
        match &self.main_pane_view {
            MainPaneView::Task(target) => {
                let goal_run_id = target_goal_run_id(self, target)?;
                let run = self.tasks.goal_run_by_id(&goal_run_id)?;
                if goal_run_status_is_terminal(run.status.clone()) {
                    None
                } else {
                    Some(AutoRefreshTarget::Goal(goal_run_id))
                }
            }
            MainPaneView::Workspace => {
                let workspace_id = self.workspace.workspace_id().to_string();
                self.workspace_has_refreshable_tasks(&workspace_id)
                    .then_some(AutoRefreshTarget::Workspace(workspace_id))
            }
            _ => None,
        }
    }

    fn workspace_has_refreshable_tasks(&self, workspace_id: &str) -> bool {
        self.workspace
            .tasks_for(workspace_id)
            .iter()
            .any(workspace_task_needs_refresh)
    }

    fn maybe_auto_refresh_visible_work(&mut self) {
        let Some(interval_ticks) = self.auto_refresh_interval_ticks() else {
            self.auto_refresh_target = None;
            self.next_auto_refresh_tick = 0;
            return;
        };
        let Some(target) = self.active_auto_refresh_target() else {
            self.auto_refresh_target = None;
            self.next_auto_refresh_tick = 0;
            return;
        };

        if self.auto_refresh_target.as_ref() != Some(&target) {
            self.auto_refresh_target = Some(target);
            self.next_auto_refresh_tick = self.tick_counter.saturating_add(interval_ticks);
            return;
        }

        if self.tick_counter < self.next_auto_refresh_tick {
            return;
        }

        match target {
            AutoRefreshTarget::Goal(goal_run_id) => {
                self.request_authoritative_goal_run_refresh(goal_run_id);
            }
            AutoRefreshTarget::Workspace(_) => {
                self.refresh_workspace_board();
            }
        }
        self.next_auto_refresh_tick = self.tick_counter.saturating_add(interval_ticks);
    }

    pub(crate) fn handle_client_event(&mut self, event: ClientEvent) {
        if let Some(ref cancelled_id) = self.cancelled_thread_id.clone() {
            let skip = match &event {
                ClientEvent::Delta { thread_id, .. }
                | ClientEvent::Reasoning { thread_id, .. }
                | ClientEvent::ToolCall { thread_id, .. }
                | ClientEvent::ToolResult { thread_id, .. }
                | ClientEvent::RetryStatus { thread_id, .. } => thread_id == cancelled_id,
                ClientEvent::Done { thread_id, .. } => {
                    if thread_id == cancelled_id {
                        self.cancelled_thread_id = None;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if skip {
                return;
            }
        }

        let Some(event) = self.handle_lifecycle_thread_client_event(event) else {
            return;
        };
        let Some(event) = self.handle_workspace_goal_context_client_event(event) else {
            return;
        };
        let Some(event) = self.handle_media_activity_stream_client_event(event) else {
            return;
        };
        let _ = self.handle_auth_agents_plugins_status_client_event(event);
    }

    fn handle_external_runtime_migration_result(&mut self, raw: serde_json::Value) {
        let summary = raw.get("summary").unwrap_or(&raw);
        let runtime = summary
            .get("runtime")
            .and_then(|value| value.as_str())
            .or_else(|| raw.get("runtime").and_then(|value| value.as_str()))
            .unwrap_or("external runtime");
        let persisted = summary
            .get("persisted")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let dry_run = summary
            .get("dry_run")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let asset_count = summary
            .get("asset_count")
            .and_then(|value| value.as_u64())
            .or_else(|| {
                raw.get("assets")
                    .and_then(|value| value.as_array())
                    .map(|items| items.len() as u64)
            });

        self.status_line = if raw.get("sources").is_some() {
            "Migration sources checked".to_string()
        } else if dry_run {
            format!(
                "{runtime} migration preview: {} asset(s)",
                asset_count.unwrap_or(0)
            )
        } else if persisted {
            format!(
                "{runtime} migration imported: {} asset(s)",
                asset_count.unwrap_or(0)
            )
        } else {
            format!("{runtime} migration report ready")
        };

        let notice = serde_json::to_string_pretty(&raw)
            .unwrap_or_else(|_| "Migration response could not be formatted".to_string());
        self.show_input_notice(&notice, InputNoticeKind::Success, 160, true);
    }
}

fn workspace_task_active_thread_id(task: &zorai_protocol::WorkspaceTask) -> Option<String> {
    task.thread_id.clone().or_else(|| {
        task.runtime_history
            .iter()
            .find_map(|entry| entry.thread_id.clone())
    })
}

fn goal_run_status_is_terminal(status: Option<task::GoalRunStatus>) -> bool {
    matches!(
        status,
        Some(task::GoalRunStatus::Completed)
            | Some(task::GoalRunStatus::Failed)
            | Some(task::GoalRunStatus::Cancelled)
    )
}

fn workspace_task_needs_refresh(task: &&zorai_protocol::WorkspaceTask) -> bool {
    task.deleted_at.is_none() && task.status != zorai_protocol::WorkspaceTaskStatus::Done
}

#[cfg(test)]
#[path = "tests/events.rs"]
mod tests;
