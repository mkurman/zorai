use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_media_activity_stream_client_event(
        &mut self,
        event: ClientEvent,
    ) -> Option<ClientEvent> {
        match event {
            ClientEvent::SpeechToTextResult { content } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("STT failed: {error}");
                        self.show_input_notice(
                            "Speech-to-text failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("STT failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return None;
                    }
                }

                let transcript = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("text")
                            .and_then(|value| value.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| content.trim().to_string());
                if !transcript.is_empty() {
                    if !self.input.buffer().trim().is_empty() {
                        self.input.reduce(input::InputAction::InsertChar(' '));
                    }
                    for ch in transcript.chars() {
                        self.input.reduce(input::InputAction::InsertChar(ch));
                    }
                    self.focus = FocusArea::Input;
                    self.status_line = "Voice transcription ready".to_string();
                }
                None
            }
            ClientEvent::TextToSpeechResult { content } => {
                self.clear_matching_agent_activity("preparing speech");
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.pending_tts_cache_key = None;
                        self.status_line = format!("TTS failed: {error}");
                        self.show_input_notice(
                            "Text-to-speech failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("TTS failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return None;
                    }
                }

                let path = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("path")
                            .and_then(|value| value.as_str())
                            .map(str::to_string)
                    });
                if let Some(path) = path {
                    if let Some(key) = self.pending_tts_cache_key.take() {
                        self.tts_audio_cache
                            .insert(key, std::path::PathBuf::from(&path));
                    }
                    self.play_audio_path(&path);
                } else {
                    self.pending_tts_cache_key = None;
                    self.status_line = "TTS result missing audio path".to_string();
                    self.show_input_notice(
                        "TTS returned no playable path",
                        InputNoticeKind::Warning,
                        70,
                        true,
                    );
                }
                None
            }
            ClientEvent::GenerateImageResult { content } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("Image generation failed: {error}");
                        self.show_input_notice(
                            "Image generation failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("Image generation failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return None;
                    }

                    let thread_id = value
                        .get("thread_id")
                        .and_then(|entry| entry.as_str())
                        .map(str::to_string);
                    let status_target = value
                        .get("path")
                        .and_then(|entry| entry.as_str())
                        .or_else(|| value.get("url").and_then(|entry| entry.as_str()))
                        .or_else(|| value.get("file_url").and_then(|entry| entry.as_str()));

                    if let Some(thread_id) = thread_id {
                        if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                            self.request_authoritative_thread_refresh(thread_id.clone(), false);
                        } else {
                            self.open_thread_conversation(thread_id.clone());
                        }
                        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(
                            thread_id,
                        ));
                    }

                    self.status_line = status_target
                        .map(|target| format!("Image generated: {target}"))
                        .unwrap_or_else(|| "Image generated".to_string());
                } else {
                    self.status_line = "Image generated".to_string();
                }
                None
            }
            ClientEvent::GenerateVideoResult { content } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("Video generation failed: {error}");
                        self.show_input_notice(
                            "Video generation failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("Video generation failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return None;
                    }

                    let thread_id = value
                        .get("thread_id")
                        .and_then(|entry| entry.as_str())
                        .map(str::to_string);
                    let status_target = value
                        .get("path")
                        .and_then(|entry| entry.as_str())
                        .or_else(|| value.get("url").and_then(|entry| entry.as_str()))
                        .or_else(|| value.get("file_url").and_then(|entry| entry.as_str()));

                    if let Some(thread_id) = thread_id {
                        if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                            self.request_authoritative_thread_refresh(thread_id.clone(), false);
                        } else {
                            self.open_thread_conversation(thread_id.clone());
                        }
                        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(
                            thread_id,
                        ));
                    }

                    self.status_line = status_target
                        .map(|target| format!("Video generated: {target}"))
                        .unwrap_or_else(|| "Video generated".to_string());
                } else {
                    self.status_line = "Video generated".to_string();
                }
                None
            }
            ClientEvent::ModelsFetched(models) => {
                self.handle_models_fetched_event(models);
                None
            }
            ClientEvent::HeartbeatItems(items) => {
                self.handle_heartbeat_items_event(items);
                None
            }
            ClientEvent::HeartbeatDigest {
                cycle_id,
                actionable,
                digest,
                items,
                checked_at,
                explanation,
            } => {
                self.handle_heartbeat_digest_event(
                    cycle_id,
                    actionable,
                    digest,
                    items,
                    checked_at,
                    explanation,
                );
                None
            }
            ClientEvent::AuditEntry {
                id,
                timestamp,
                action_type,
                summary,
                explanation,
                confidence,
                confidence_band,
                causal_trace_id,
                thread_id,
            } => {
                self.handle_audit_entry_event(
                    id,
                    timestamp,
                    action_type,
                    summary,
                    explanation,
                    confidence,
                    confidence_band,
                    causal_trace_id,
                    thread_id,
                );
                None
            }
            ClientEvent::EscalationUpdate {
                thread_id,
                from_level,
                to_level,
                reason,
                attempts,
                audit_id,
            } => {
                self.handle_escalation_update_event(
                    thread_id, from_level, to_level, reason, attempts, audit_id,
                );
                None
            }
            ClientEvent::AnticipatoryItems(items) => {
                self.handle_anticipatory_items_event(items);
                None
            }
            ClientEvent::GatewayStatus {
                platform,
                status,
                last_error,
                consecutive_failures,
            } => {
                self.handle_gateway_status_event(
                    platform,
                    status,
                    last_error,
                    consecutive_failures,
                );
                None
            }
            ClientEvent::WhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                self.handle_whatsapp_link_status_event(state, phone, last_error);
                None
            }
            ClientEvent::WhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                self.handle_whatsapp_link_qr_event(ascii_qr, expires_at_ms);
                None
            }
            ClientEvent::WhatsAppLinked { phone } => {
                self.handle_whatsapp_linked_event(phone);
                None
            }
            ClientEvent::WhatsAppLinkError { message, .. } => {
                self.handle_whatsapp_link_error_event(message);
                None
            }
            ClientEvent::WhatsAppLinkDisconnected { reason } => {
                self.handle_whatsapp_link_disconnected_event(reason);
                None
            }
            ClientEvent::TierChanged { new_tier } => {
                self.handle_tier_changed_event(new_tier);
                None
            }
            ClientEvent::SemanticIndexRepaired { summary } => {
                self.status_line = summary;
                None
            }
            ClientEvent::Delta { thread_id, content } => {
                self.handle_delta_event(thread_id, content);
                None
            }
            ClientEvent::Reasoning { thread_id, content } => {
                self.handle_reasoning_event(thread_id, content);
                None
            }
            ClientEvent::ToolCall {
                thread_id,
                call_id,
                name,
                arguments,
                weles_review,
                message_id,
            } => {
                self.handle_tool_call_event(
                    thread_id,
                    call_id,
                    name,
                    arguments,
                    weles_review,
                    message_id,
                );
                None
            }
            ClientEvent::ToolResult {
                thread_id,
                call_id,
                name,
                content,
                is_error,
                weles_review,
                message_id,
            } => {
                self.handle_tool_result_event(
                    thread_id,
                    call_id,
                    name,
                    content,
                    is_error,
                    weles_review,
                    message_id,
                );
                None
            }
            ClientEvent::Done {
                thread_id,
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
            } => {
                self.handle_done_event(
                    thread_id,
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
                );
                None
            }
            other => Some(other),
        }
    }
}
