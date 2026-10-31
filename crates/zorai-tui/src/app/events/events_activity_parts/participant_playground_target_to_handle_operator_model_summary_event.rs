use super::super::super::*;
use super::super::events_audio::text_to_speech_result_path;
use super::super::*;

impl TuiModel {
    fn participant_playground_target(thread_id: &str) -> Option<(&str, &str)> {
        let remainder = thread_id.strip_prefix("playground:")?;
        let (participant_agent_id, visible_thread_id) = remainder.split_once(':')?;
        if participant_agent_id.is_empty() || visible_thread_id.is_empty() {
            return None;
        }
        Some((participant_agent_id, visible_thread_id))
    }

    fn fallback_participant_agent_name(agent_id: &str) -> String {
        let mut chars = agent_id.chars();
        match chars.next() {
            Some(first) => {
                let mut name = first.to_uppercase().collect::<String>();
                name.push_str(chars.as_str());
                name
            }
            None => "Participant".to_string(),
        }
    }

    fn resolve_participant_agent_name(
        &self,
        visible_thread_id: &str,
        participant_agent_id: &str,
    ) -> String {
        self.chat
            .threads()
            .iter()
            .find(|thread| thread.id == visible_thread_id)
            .and_then(|thread| {
                thread
                    .thread_participants
                    .iter()
                    .find(|participant| {
                        participant
                            .agent_id
                            .eq_ignore_ascii_case(participant_agent_id)
                    })
                    .map(|participant| participant.agent_name.clone())
            })
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| Self::fallback_participant_agent_name(participant_agent_id))
    }

    fn mark_participant_playground_active(&mut self, playground_thread_id: &str) -> bool {
        let Some((participant_agent_id, visible_thread_id)) =
            Self::participant_playground_target(playground_thread_id)
        else {
            return false;
        };

        let participant_agent_name =
            self.resolve_participant_agent_name(visible_thread_id, participant_agent_id);
        self.participant_playground_activity.insert(
            playground_thread_id.to_string(),
            super::ParticipantPlaygroundActivity {
                visible_thread_id: visible_thread_id.to_string(),
                participant_agent_id: participant_agent_id.to_string(),
                participant_agent_name,
            },
        );
        true
    }

    fn clear_participant_playground_activity(
        &mut self,
        playground_thread_id: &str,
    ) -> Option<String> {
        self.participant_playground_activity
            .remove(playground_thread_id)
            .map(|activity| activity.visible_thread_id)
    }

    pub(crate) fn participant_footer_activity(&self) -> Option<String> {
        let active_thread_id = self.chat.active_thread_id()?;
        let mut participants = self
            .participant_playground_activity
            .values()
            .filter(|activity| activity.visible_thread_id == active_thread_id)
            .map(|activity| {
                (
                    activity.participant_agent_id.to_ascii_lowercase(),
                    activity.participant_agent_name.clone(),
                )
            })
            .collect::<Vec<_>>();
        participants.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        participants.dedup_by(|left, right| left.0 == right.0);

        let first = participants.first()?.1.clone();
        if participants.len() == 1 {
            Some(format!("{first} crafting response"))
        } else {
            Some(format!(
                "{first} +{} crafting responses",
                participants.len() - 1
            ))
        }
    }

    pub(crate) fn footer_activity_text(&self) -> Option<String> {
        self.current_thread_agent_activity()
            .map(str::to_string)
            .or_else(|| self.participant_footer_activity())
    }

    fn should_surface_thread_activity(&self, thread_id: &str) -> bool {
        match self.chat.active_thread_id() {
            None => true,
            Some(active_thread_id) => active_thread_id == thread_id,
        }
    }

    pub(super) fn should_accept_retry_status_event(&self, thread_id: &str) -> bool {
        if self.chat.is_streaming()
            || self.chat.retry_status().is_some()
            || self.current_thread_agent_activity().is_some()
        {
            return true;
        }

        match self.chat.active_thread_id() {
            None => true,
            Some(active_thread_id) if active_thread_id != thread_id => false,
            Some(_) => self
                .chat
                .active_thread()
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            !message.content.trim().is_empty()
                                && !matches!(message.role, chat::MessageRole::System)
                        })
                        .map(|message| message.role == chat::MessageRole::User)
                })
                .unwrap_or(false),
        }
    }

    pub(in crate::app) fn handle_delta_event(&mut self, thread_id: String, content: String) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        let viewport_anchor = self.capture_locked_chat_viewport(Some(thread_id.as_str()));
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), "writing");
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        self.chat
            .reduce(chat::ChatAction::Delta { thread_id, content });
        self.restore_locked_chat_viewport(viewport_anchor);
    }

    pub(in crate::app) fn handle_reasoning_event(&mut self, thread_id: String, content: String) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        let viewport_anchor = self.capture_locked_chat_viewport(Some(thread_id.as_str()));
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), "reasoning");
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        self.chat
            .reduce(chat::ChatAction::Reasoning { thread_id, content });
        self.restore_locked_chat_viewport(viewport_anchor);
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_tool_call_event(
        &mut self,
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
        weles_review: Option<crate::client::WelesReviewMetaVm>,
        message_id: Option<String>,
    ) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), format!("⚙  {}", name));
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::ToolCall {
                thread_id,
                call_id,
                name,
                args: arguments,
                weles_review,
                message_id,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_tool_result_event(
        &mut self,
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        weles_review: Option<crate::client::WelesReviewMetaVm>,
        message_id: Option<String>,
    ) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        let tool_call_still_active = self
            .chat
            .thread_has_active_tool_call(thread_id.as_str(), call_id.as_str());
        if tool_call_still_active {
            self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
            self.set_agent_activity_for(Some(thread_id.clone()), format!("⚙  {} ✓", name));
        }
        if tool_call_still_active && self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        let maybe_tts_path = text_to_speech_result_path(&name, &content, is_error);
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::ToolResult {
                thread_id,
                call_id,
                name,
                content,
                is_error,
                weles_review,
                message_id,
            },
        );
        if let Some(path) = maybe_tts_path {
            self.play_audio_path(&path);
        }
        self.dispatch_next_queued_prompt_if_ready();
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_done_event(
        &mut self,
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
        reasoning: Option<String>,
        provider_final_result_json: Option<String>,
        message_id: Option<String>,
    ) {
        if let Some(visible_thread_id) = self.clear_participant_playground_activity(&thread_id) {
            if self.chat.active_thread_id() == Some(visible_thread_id.as_str()) {
                self.request_authoritative_thread_refresh(visible_thread_id, false);
            }
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        if self.done_arrived_before_pending_prompt_output(thread_id.as_str()) {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.clear_pending_prompt_response_thread(thread_id.as_str());
        self.clear_agent_activity_for(Some(thread_id.as_str()));
        if self.should_surface_thread_activity(&thread_id) {
            self.pending_stop = false;
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
            if self
                .input_notice
                .as_ref()
                .is_some_and(|notice| notice.kind == InputNoticeKind::Warning)
            {
                self.input_notice = None;
            }
        }
        self.reduce_chat_for_thread(
            Some(thread_id.as_str()),
            chat::ChatAction::TurnDone {
                thread_id: thread_id.clone(),
                input_tokens,
                output_tokens,
                cost,
                provider,
                model,
                tps,
                generation_ms,
                reasoning,
                provider_final_result_json,
                message_id,
            },
        );

        self.auto_speak_finished_turn(&thread_id);
        let _ = self.maybe_request_auto_response_for_open_thread(&thread_id);
        let _ = self.maybe_auto_send_always_auto_response();
        self.dispatch_next_queued_prompt_if_ready();
    }

    pub(in crate::app) fn handle_operator_profile_session_started_event(
        &mut self,
        session_id: String,
        kind: String,
    ) {
        self.operator_profile.visible = true;
        self.operator_profile.loading = true;
        self.operator_profile.session_id = Some(session_id);
        self.operator_profile.session_kind = Some(kind);
        self.operator_profile.deferred_session_id = None;
        self.operator_profile.question = None;
        self.operator_profile.bool_answer = None;
        self.operator_profile.warning = None;
        self.open_operator_profile_onboarding_modal();
        self.set_main_pane_conversation(FocusArea::Input);
        self.status_line = "Operator profile onboarding started".to_string();
        self.send_daemon_command(DaemonCommand::GetOperatorProfileSummary);
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_operator_profile_question_event(
        &mut self,
        session_id: String,
        question_id: String,
        field_key: String,
        prompt: String,
        input_kind: String,
        optional: bool,
    ) {
        if self.operator_profile.deferred_session_id.as_deref() == Some(session_id.as_str()) {
            self.close_operator_profile_onboarding_modal();
            return;
        }
        self.operator_profile.visible = true;
        self.operator_profile.loading = false;
        self.operator_profile.session_id = Some(session_id.clone());
        let is_bool_question =
            TuiModel::normalize_operator_profile_input_kind(&input_kind) == "bool";
        self.operator_profile.question = Some(super::OperatorProfileQuestionVm {
            session_id,
            question_id,
            field_key,
            prompt,
            input_kind,
            optional,
        });
        self.operator_profile.bool_answer = is_bool_question.then_some(true);
        self.operator_profile.warning = None;
        self.open_operator_profile_onboarding_modal();
        self.set_main_pane_conversation(if is_bool_question {
            FocusArea::Chat
        } else {
            FocusArea::Input
        });
        self.input.reduce(input::InputAction::Clear);
        if !is_bool_question {
            if let Some(options) = self.current_operator_profile_select_options() {
                if let Some(first) = options.first() {
                    self.input.set_text(first);
                }
            }
        }
        self.status_line = "Operator profile question ready".to_string();
        self.show_input_notice(
            "Answer then Enter • Ctrl+S skip • Ctrl+D defer",
            InputNoticeKind::Success,
            120,
            true,
        );
    }

    pub(in crate::app) fn handle_operator_profile_progress_event(
        &mut self,
        session_id: String,
        answered: u32,
        remaining: u32,
        completion_ratio: f64,
    ) {
        if self.operator_profile.deferred_session_id.as_deref() == Some(session_id.as_str()) {
            self.close_operator_profile_onboarding_modal();
            return;
        }
        self.operator_profile.visible = true;
        self.operator_profile.loading = true;
        self.operator_profile.session_id = Some(session_id.clone());
        self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
            answered,
            remaining,
            completion_ratio,
        });
        self.open_operator_profile_onboarding_modal();
        self.send_daemon_command(DaemonCommand::NextOperatorProfileQuestion { session_id });
        self.status_line = format!(
            "Operator profile progress: {} answered, {} remaining",
            answered, remaining
        );
    }

    pub(in crate::app) fn handle_operator_question_event(
        &mut self,
        question_id: String,
        content: String,
        options: Vec<String>,
        thread_id: Option<String>,
    ) {
        let target_thread_id =
            thread_id.or_else(|| self.chat.active_thread_id().map(str::to_string));
        let Some(target_thread_id) = target_thread_id else {
            return;
        };

        self.reduce_chat_for_thread(
            Some(target_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id: target_thread_id.clone(),
                message: chat::AgentMessage {
                    role: chat::MessageRole::Assistant,
                    content,
                    is_operator_question: true,
                    operator_question_id: Some(question_id.clone()),
                    actions: options
                        .into_iter()
                        .map(|option| chat::MessageAction {
                            label: option.clone(),
                            action_type: format!("operator_question_answer:{question_id}:{option}"),
                            thread_id: Some(target_thread_id.clone()),
                        })
                        .chain(std::iter::once(chat::MessageAction {
                            label: "Other".to_string(),
                            action_type: format!("operator_question_other:{question_id}"),
                            thread_id: Some(target_thread_id.clone()),
                        }))
                        .collect(),
                    ..Default::default()
                },
            },
        );
        self.status_line = "Operator question ready".to_string();
    }

    pub(in crate::app) fn handle_operator_question_resolved_event(
        &mut self,
        question_id: String,
        answer: String,
    ) {
        if self
            .chat
            .resolve_operator_question_answer(&question_id, answer.clone())
        {
            self.status_line = format!("Operator question answered: {answer}");
        }
    }

    pub(in crate::app) fn handle_operator_profile_summary_event(&mut self, summary_json: String) {
        self.operator_profile.summary_json = Some(summary_json.clone());
        if self.operator_profile.progress.is_none() {
            if let Ok(summary) = serde_json::from_str::<serde_json::Value>(&summary_json) {
                let answered = summary
                    .get("field_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0) as u32;
                self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
                    answered,
                    remaining: self
                        .operator_profile
                        .question
                        .as_ref()
                        .map(|_| 1u32)
                        .unwrap_or(0),
                    completion_ratio: 0.0,
                });
            }
        }
        if self.operator_profile_auto_start_pending_summary {
            let _ = self.try_start_operator_profile_autostart_from_pending_summary();
        }
    }

    pub(in crate::app) fn try_start_operator_profile_autostart_from_pending_summary(
        &mut self,
    ) -> bool {
        if !self.operator_profile_auto_start_pending_summary {
            return false;
        }
        if self.concierge.loading {
            return true;
        }
        let Some(summary_json) = self.operator_profile.summary_json.clone() else {
            return true;
        };

        self.operator_profile_auto_start_pending_summary = false;
        if !Self::operator_profile_summary_has_completed_onboarding(&summary_json)
            && !self.operator_profile.loading
            && self.operator_profile.session_id.is_none()
            && self.operator_profile.question.is_none()
        {
            self.send_daemon_command(DaemonCommand::StartOperatorProfileSession {
                kind: "first_run_onboarding".to_string(),
            });
        }
        true
    }

    fn operator_profile_summary_has_completed_onboarding(summary_json: &str) -> bool {
        let Ok(summary) = serde_json::from_str::<serde_json::Value>(summary_json) else {
            return false;
        };
        let fields = summary.get("fields").and_then(serde_json::Value::as_object);
        let has_required_fields = ["name", "role", "primary_language"]
            .iter()
            .all(|field_key| fields.is_some_and(|fields| fields.contains_key(*field_key)));
        if has_required_fields {
            return true;
        }

        let Some(consents) = summary.get("consents") else {
            return false;
        };
        let required_consents = [
            "enabled",
            "allow_message_statistics",
            "allow_approval_learning",
            "allow_attention_tracking",
            "allow_implicit_feedback",
        ];
        if let Some(consents) = consents.as_object() {
            return required_consents
                .iter()
                .all(|consent_key| consents.contains_key(*consent_key));
        }
        let Some(consents) = consents.as_array() else {
            return false;
        };
        required_consents.iter().all(|required_key| {
            consents.iter().any(|entry| {
                entry.get("consent_key").and_then(serde_json::Value::as_str) == Some(*required_key)
            })
        })
    }

    pub(in crate::app) fn handle_operator_model_summary_event(&mut self, model_json: String) {
        let pretty = serde_json::from_str::<serde_json::Value>(&model_json)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(model_json);
        self.last_error = Some(pretty);
        self.error_active = true;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
        self.status_line = "Operator model snapshot loaded".to_string();
    }
}
