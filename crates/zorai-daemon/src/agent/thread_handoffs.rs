use super::*;
use serde::{Deserialize, Serialize};

pub(super) const THREAD_HANDOFF_SYSTEM_MARKER: &str = "[[handoff_event]]";
pub(super) const INTERNAL_HANDOFF_THREAD_PREFIX: &str = "handoff:";
const PARTICIPANT_HANDOFF_RETURN_INSTRUCTION: &str = "Remain attached to this thread so ownership can be handed back when your skills are needed again.";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandoffKind {
    Push,
    Return,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadHandoffRequestedBy {
    User,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadResponderFrame {
    pub agent_id: String,
    pub agent_name: String,
    pub entered_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entered_via_handoff_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadHandoffEvent {
    pub id: String,
    pub kind: ThreadHandoffKind,
    pub from_agent_id: String,
    pub to_agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_agent_name: Option<String>,
    pub requested_by: ThreadHandoffRequestedBy,
    pub reason: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    pub stack_depth_before: usize,
    pub stack_depth_after: usize,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThreadHandoffState {
    pub origin_agent_id: String,
    pub active_agent_id: String,
    #[serde(default)]
    pub responder_stack: Vec<ThreadResponderFrame>,
    #[serde(default)]
    pub events: Vec<ThreadHandoffEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PendingThreadHandoffActivation {
    pub thread_id: String,
    pub kind: ThreadHandoffKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,
    pub requested_by: ThreadHandoffRequestedBy,
    pub reason: String,
    pub summary: String,
}

pub(crate) fn build_thread_handoff_activation(
    thread_id: &str,
    action: &str,
    target_agent_id: Option<&str>,
    requested_by: ThreadHandoffRequestedBy,
    reason: &str,
    summary: &str,
) -> Result<PendingThreadHandoffActivation> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        anyhow::bail!("handoff_thread_agent requires a thread context");
    }
    let reason = reason.trim();
    if reason.is_empty() {
        anyhow::bail!("missing 'reason' argument");
    }
    let summary = summary.trim();
    if summary.is_empty() {
        anyhow::bail!("missing 'summary' argument");
    }

    let (kind, target_agent_id) = match action.trim() {
        "push_handoff" => {
            let target = target_agent_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("push_handoff requires 'target_agent_id'"))?;
            (ThreadHandoffKind::Push, Some(target.to_string()))
        }
        "return_handoff" => (ThreadHandoffKind::Return, None),
        other => anyhow::bail!("unsupported handoff action: {other}"),
    };

    Ok(PendingThreadHandoffActivation {
        thread_id: thread_id.to_string(),
        kind,
        target_agent_id,
        requested_by,
        reason: reason.to_string(),
        summary: summary.to_string(),
    })
}

pub(super) fn default_agent_id_for_thread(
    thread_id: &str,
    persisted_agent_name: Option<&str>,
) -> String {
    if thread_id == crate::agent::concierge::CONCIERGE_THREAD_ID {
        return CONCIERGE_AGENT_ID.to_string();
    }
    if is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return canonical_agent_id(persisted_agent_name.unwrap_or(MAIN_AGENT_ID)).to_string();
    }
    canonical_agent_id(persisted_agent_name.unwrap_or(MAIN_AGENT_ID)).to_string()
}

pub(in crate::agent) fn is_internal_handoff_thread(thread_id: &str) -> bool {
    thread_id.starts_with(INTERNAL_HANDOFF_THREAD_PREFIX)
}

pub(super) fn initial_thread_handoff_state(
    thread_id: &str,
    persisted_agent_name: Option<&str>,
    entered_at: u64,
) -> ThreadHandoffState {
    let agent_id = default_agent_id_for_thread(thread_id, persisted_agent_name);
    let agent_name = persisted_agent_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| canonical_agent_name(&agent_id).to_string());
    ThreadHandoffState {
        origin_agent_id: agent_id.clone(),
        active_agent_id: agent_id.clone(),
        responder_stack: vec![ThreadResponderFrame {
            agent_id: agent_id.clone(),
            agent_name,
            entered_at,
            entered_via_handoff_event_id: None,
            linked_thread_id: None,
        }],
        events: Vec::new(),
        pending_approval_id: None,
    }
}

pub(super) fn normalized_thread_handoff_state(
    thread_id: &str,
    persisted_agent_name: Option<&str>,
    created_at: u64,
    state: Option<ThreadHandoffState>,
) -> ThreadHandoffState {
    let mut state = state.unwrap_or_else(|| {
        initial_thread_handoff_state(thread_id, persisted_agent_name, created_at)
    });
    if state.origin_agent_id.trim().is_empty() {
        state.origin_agent_id = default_agent_id_for_thread(thread_id, persisted_agent_name);
    }
    if state.active_agent_id.trim().is_empty() {
        state.active_agent_id = state.origin_agent_id.clone();
    }
    if state.responder_stack.is_empty() {
        let fallback_name = persisted_agent_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| canonical_agent_name(&state.active_agent_id).to_string());
        state.responder_stack.push(ThreadResponderFrame {
            agent_id: state.active_agent_id.clone(),
            agent_name: fallback_name,
            entered_at: created_at,
            entered_via_handoff_event_id: None,
            linked_thread_id: None,
        });
    }
    state
}

pub(super) fn active_agent_name_for_thread(
    thread: &AgentThread,
    state: Option<&ThreadHandoffState>,
) -> String {
    if let Some(value) = thread
        .agent_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return value.to_string();
    }
    if let Some(state) = state {
        if let Some(frame_name) = state
            .responder_stack
            .last()
            .map(|frame| frame.agent_name.trim())
            .filter(|value| !value.is_empty())
        {
            return frame_name.to_string();
        }
        return canonical_agent_name(&state.active_agent_id).to_string();
    }
    if thread.id == crate::agent::concierge::CONCIERGE_THREAD_ID {
        return CONCIERGE_AGENT_NAME.to_string();
    }
    if is_internal_dm_thread(&thread.id)
        || is_participant_playground_thread(&thread.id)
        || is_internal_handoff_thread(&thread.id)
    {
        return "Internal DM".to_string();
    }
    MAIN_AGENT_NAME.to_string()
}

pub(super) fn visible_thread_owner_agent_name_for_handoff_state(
    thread_id: &str,
    state: &ThreadHandoffState,
    has_thread_participants: bool,
) -> Option<String> {
    if !has_thread_participants
        || is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return None;
    }

    // Prefer the responder stack frame name for the origin agent so custom
    // sub-agent display names (e.g. "DeepSeekorrr") survive. `canonical_agent_name`
    // only knows built-in personas and collapses unfamiliar ids (sub-agent UUIDs)
    // back to "Svarog", which would silently overwrite the sub-agent owner name
    // whenever a participant joins the thread.
    let origin_name = state
        .responder_stack
        .iter()
        .find(|frame| frame.agent_id.eq_ignore_ascii_case(&state.origin_agent_id))
        .map(|frame| frame.agent_name.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| canonical_agent_name(&state.origin_agent_id).to_string());
    Some(origin_name)
}

pub(super) fn persisted_agent_name_for_thread_surface(
    thread: &AgentThread,
    state: Option<&ThreadHandoffState>,
    has_thread_participants: bool,
) -> String {
    if let Some(owner_name) = state.and_then(|state| {
        visible_thread_owner_agent_name_for_handoff_state(
            &thread.id,
            state,
            has_thread_participants,
        )
    }) {
        return owner_name;
    }

    active_agent_name_for_thread(thread, state)
}

fn resolve_thread_handoff_agent(alias: &str) -> Option<(String, String)> {
    let trimmed = alias.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(agent_id) = crate::agent::agent_identity::agent_id_for_alias(trimmed) {
        return resolve_thread_handoff_agent(agent_id);
    }
    let normalized = trimmed.to_ascii_lowercase();
    let builtin = match normalized.as_str() {
        MAIN_AGENT_ID
        | "svarog"
        | MAIN_AGENT_ALIAS
        | MAIN_AGENT_LEGACY_ALIAS
        | MAIN_AGENT_FALLBACK_ALIAS => {
            Some((MAIN_AGENT_ID.to_string(), MAIN_AGENT_NAME.to_string()))
        }
        CONCIERGE_AGENT_ID | CONCIERGE_AGENT_ALIAS | CONCIERGE_AGENT_LEGACY_ALIAS => Some((
            CONCIERGE_AGENT_ID.to_string(),
            CONCIERGE_AGENT_NAME.to_string(),
        )),
        SWAROZYC_AGENT_ID => Some((
            SWAROZYC_AGENT_ID.to_string(),
            SWAROZYC_AGENT_NAME.to_string(),
        )),
        RADOGOST_AGENT_ID => Some((
            RADOGOST_AGENT_ID.to_string(),
            RADOGOST_AGENT_NAME.to_string(),
        )),
        DOMOWOJ_AGENT_ID => Some((DOMOWOJ_AGENT_ID.to_string(), DOMOWOJ_AGENT_NAME.to_string())),
        SWIETOWIT_AGENT_ID => Some((
            SWIETOWIT_AGENT_ID.to_string(),
            SWIETOWIT_AGENT_NAME.to_string(),
        )),
        PERUN_AGENT_ID => Some((PERUN_AGENT_ID.to_string(), PERUN_AGENT_NAME.to_string())),
        MOKOSH_AGENT_ID => Some((MOKOSH_AGENT_ID.to_string(), MOKOSH_AGENT_NAME.to_string())),
        DAZHBOG_AGENT_ID => Some((DAZHBOG_AGENT_ID.to_string(), DAZHBOG_AGENT_NAME.to_string())),
        ROD_AGENT_ID => Some((ROD_AGENT_ID.to_string(), ROD_AGENT_NAME.to_string())),
        WELES_AGENT_ID | "veles" => {
            Some((WELES_AGENT_ID.to_string(), WELES_AGENT_NAME.to_string()))
        }
        _ => None,
    };
    builtin
}

fn thread_handoff_system_message(event: &ThreadHandoffEvent) -> String {
    let from_agent_name = event
        .from_agent_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| canonical_agent_name(&event.from_agent_id));
    let to_agent_name = event
        .to_agent_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| canonical_agent_name(&event.to_agent_id));
    let payload = serde_json::json!({
        "id": event.id,
        "kind": match event.kind {
            ThreadHandoffKind::Push => "push",
            ThreadHandoffKind::Return => "return",
        },
        "from_agent_id": event.from_agent_id,
        "from_agent_name": from_agent_name,
        "to_agent_id": event.to_agent_id,
        "to_agent_name": to_agent_name,
        "requested_by": match event.requested_by {
            ThreadHandoffRequestedBy::User => "user",
            ThreadHandoffRequestedBy::Agent => "agent",
        },
        "reason": event.reason,
        "summary": event.summary,
        "linked_thread_id": event.linked_thread_id,
        "approval_id": event.approval_id,
        "stack_depth_before": event.stack_depth_before,
        "stack_depth_after": event.stack_depth_after,
        "created_at": event.created_at,
    });
    format!(
        "{THREAD_HANDOFF_SYSTEM_MARKER}{}\n{} handed this thread to {}. Summary: {}",
        payload, from_agent_name, to_agent_name, event.summary
    )
}

fn handoff_context_message(
    event_id: &str,
    primary_thread_id: &str,
    from_agent_id: &str,
    to_agent_id: &str,
    reason: &str,
    summary: &str,
) -> String {
    serde_json::json!({
        "kind": "thread_handoff_context",
        "event_id": event_id,
        "primary_thread_id": primary_thread_id,
        "from_agent_id": from_agent_id,
        "from_agent_name": canonical_agent_name(from_agent_id),
        "to_agent_id": to_agent_id,
        "to_agent_name": canonical_agent_name(to_agent_id),
        "reason": reason,
        "summary": summary,
    })
    .to_string()
}

fn thread_handoff_approval_command(request: &PendingThreadHandoffActivation) -> String {
    format!(
        "handoff_thread_agent {}",
        serde_json::to_string(request).unwrap_or_else(|_| "{}".to_string())
    )
}

impl AgentEngine {
    async fn resolve_plain_thread_handoff_target(
        &self,
        target_alias: &str,
    ) -> Result<(String, String)> {
        if let Some(resolved) = resolve_thread_handoff_agent(target_alias) {
            return Ok(resolved);
        }
        let trimmed = target_alias.trim();
        if let Some(sub_agent) = self.list_sub_agents().await.into_iter().find(|entry| {
            entry.enabled
                && (entry.id.eq_ignore_ascii_case(trimmed)
                    || entry.name.eq_ignore_ascii_case(trimmed))
        }) {
            return Ok((sub_agent.id, sub_agent.name));
        }
        anyhow::bail!("unknown handoff target: {target_alias}");
    }

    pub(crate) async fn apply_operator_thread_handoff(
        &self,
        thread_id: &str,
        action: &str,
        target_agent_id: Option<&str>,
        reason: &str,
        summary: &str,
    ) -> Result<ThreadHandoffEvent> {
        let request = build_thread_handoff_activation(
            thread_id,
            action,
            target_agent_id,
            ThreadHandoffRequestedBy::User,
            reason,
            summary,
        )?;
        self.apply_thread_handoff_activation(&request, None).await
    }

    pub async fn thread_handoff_state(&self, thread_id: &str) -> Option<ThreadHandoffState> {
        self.thread_handoff_states
            .read()
            .await
            .get(thread_id)
            .cloned()
    }

    pub async fn set_thread_handoff_state(&self, thread_id: &str, state: ThreadHandoffState) {
        let created_at = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.created_at)
            .unwrap_or_else(now_millis);
        let normalized = normalized_thread_handoff_state(thread_id, None, created_at, Some(state));
        let has_thread_participants = self
            .thread_participants
            .read()
            .await
            .get(thread_id)
            .is_some_and(|participants| !participants.is_empty());
        // Prefer the responder stack's frame name over `canonical_agent_name`.
        // The canonical helper only knows the built-in personas and collapses
        // any unfamiliar id (e.g. a user-defined sub-agent UUID like
        // "subagent-<timestamp>") back to "Svarog", which would silently
        // overwrite a custom sub-agent's display name on every handoff-state
        // update.
        let stack_name = normalized
            .responder_stack
            .iter()
            .rev()
            .find(|frame| {
                frame
                    .agent_id
                    .eq_ignore_ascii_case(&normalized.active_agent_id)
            })
            .and_then(|frame| {
                let trimmed = frame.agent_name.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            });
        let fallback_name = stack_name
            .unwrap_or_else(|| canonical_agent_name(&normalized.active_agent_id).to_string());
        let thread_agent_name = visible_thread_owner_agent_name_for_handoff_state(
            thread_id,
            &normalized,
            has_thread_participants,
        )
        .unwrap_or(fallback_name);
        self.thread_handoff_states
            .write()
            .await
            .insert(thread_id.to_string(), normalized);
        if let Some(thread) = self.threads.write().await.get_mut(thread_id) {
            thread.agent_name = Some(thread_agent_name);
        }
    }

    pub async fn active_agent_id_for_thread(&self, thread_id: &str) -> Option<String> {
        self.thread_handoff_states
            .read()
            .await
            .get(thread_id)
            .map(|state| state.active_agent_id.clone())
    }

    /// Find the in-memory task whose `thread_id` matches the given thread.
    /// Used by `force_compact_and_continue` to thread the task's
    /// `override_provider` / `override_model` into the compaction
    /// continuation — without it the builtin-persona-via-task-override
    /// scenario fails the persona setup gate.
    pub(in crate::agent) async fn task_id_owning_thread(&self, thread_id: &str) -> Option<String> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.thread_id.as_deref() == Some(thread_id))
            .map(|task| task.id.clone())
    }

    async fn ensure_linked_handoff_thread(
        &self,
        primary_thread_id: &str,
        from_agent_id: &str,
        to_agent_id: &str,
        reason: &str,
        summary: &str,
        event_id: &str,
    ) -> String {
        let linked_thread_id =
            format!("{INTERNAL_HANDOFF_THREAD_PREFIX}{primary_thread_id}:{event_id}");
        let now = now_millis();
        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .entry(linked_thread_id.clone())
                .or_insert_with(|| AgentThread {
                    id: linked_thread_id.clone(),
                    agent_name: Some(canonical_agent_name(to_agent_id).to_string()),
                    title: format!(
                        "Handoff · {} -> {}",
                        canonical_agent_name(from_agent_id),
                        canonical_agent_name(to_agent_id)
                    ),
                    messages: Vec::new(),
                    pinned: false,
                    upstream_thread_id: Some(primary_thread_id.to_string()),
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    created_at: now,
                    updated_at: now,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                });
            thread.agent_name = Some(canonical_agent_name(to_agent_id).to_string());
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::System,
                content: handoff_context_message(
                    event_id,
                    primary_thread_id,
                    from_agent_id,
                    to_agent_id,
                    reason,
                    summary,
                ),
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
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now,
                feedback: None,
            });
            thread.updated_at = now;
        }
        self.thread_handoff_states.write().await.insert(
            linked_thread_id.clone(),
            initial_thread_handoff_state(
                &linked_thread_id,
                Some(canonical_agent_name(to_agent_id)),
                now,
            ),
        );
        self.persist_thread_by_id(&linked_thread_id).await;
        linked_thread_id
    }

    pub(super) async fn apply_thread_handoff_activation(
        &self,
        request: &PendingThreadHandoffActivation,
        approval_id: Option<String>,
    ) -> Result<ThreadHandoffEvent> {
        let thread_id = request.thread_id.trim();
        if thread_id.is_empty() {
            anyhow::bail!("missing thread id for handoff activation");
        }
        let thread_created_at = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.created_at)
            .ok_or_else(|| anyhow::anyhow!("thread {thread_id} does not exist"))?;
        let mut state = self
            .thread_handoff_state(thread_id)
            .await
            .unwrap_or_else(|| initial_thread_handoff_state(thread_id, None, thread_created_at));
        if state
            .pending_approval_id
            .as_deref()
            .is_some_and(|value| approval_id.as_deref() != Some(value))
        {
            anyhow::bail!("thread already has a different pending handoff approval");
        }

        let now = now_millis();
        let event_id = format!("handoff_{}", Uuid::new_v4());
        let from_agent_id = state.active_agent_id.clone();
        let from_agent_name = state
            .responder_stack
            .iter()
            .rev()
            .find(|frame| frame.agent_id.eq_ignore_ascii_case(&from_agent_id))
            .map(|frame| frame.agent_name.trim().to_string())
            .filter(|name| !name.is_empty());
        let stack_depth_before = state.responder_stack.len();
        let thread_participants = self.list_thread_participants(thread_id).await;

        let (to_agent_id, to_agent_name, linked_thread_id, stack_depth_after) = match request.kind {
            ThreadHandoffKind::Push => {
                let target_alias = request
                    .target_agent_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("push_handoff requires target_agent_id"))?;
                let participant_managed_thread = !thread_participants.is_empty();
                let (target_agent_id, target_agent_name) = if participant_managed_thread {
                    let (resolved_id, resolved_name) =
                        self.resolve_thread_participant_target(target_alias).await?;
                    let active_participant = thread_participants.iter().find(|participant| {
                        participant.status == ThreadParticipantStatus::Active
                            && participant.agent_id.eq_ignore_ascii_case(&resolved_id)
                    });
                    if active_participant.is_none() {
                        anyhow::bail!(
                            "participant-managed threads may hand off only to active thread participants"
                        );
                    }
                    (resolved_id, resolved_name)
                } else {
                    self.resolve_plain_thread_handoff_target(target_alias)
                        .await?
                };
                if target_agent_id == from_agent_id {
                    anyhow::bail!("cannot hand off a thread to the current active responder");
                }
                if participant_managed_thread
                    && !thread_participants.iter().any(|participant| {
                        participant.status == ThreadParticipantStatus::Active
                            && participant.agent_id.eq_ignore_ascii_case(&from_agent_id)
                    })
                {
                    self.upsert_thread_participant(
                        thread_id,
                        &from_agent_id,
                        PARTICIPANT_HANDOFF_RETURN_INSTRUCTION,
                    )
                    .await?;
                }
                let linked_thread_id = self
                    .ensure_linked_handoff_thread(
                        thread_id,
                        &from_agent_id,
                        &target_agent_id,
                        &request.reason,
                        &request.summary,
                        &event_id,
                    )
                    .await;
                state.responder_stack.push(ThreadResponderFrame {
                    agent_id: target_agent_id.clone(),
                    agent_name: target_agent_name.clone(),
                    entered_at: now,
                    entered_via_handoff_event_id: Some(event_id.clone()),
                    linked_thread_id: Some(linked_thread_id.clone()),
                });
                state.active_agent_id = target_agent_id.clone();
                (
                    target_agent_id,
                    target_agent_name,
                    Some(linked_thread_id),
                    state.responder_stack.len(),
                )
            }
            ThreadHandoffKind::Return => {
                if state.responder_stack.len() <= 1 {
                    anyhow::bail!("cannot return handoff when no previous responder exists");
                }
                let current_frame = state
                    .responder_stack
                    .pop()
                    .expect("responder stack length already checked");
                let previous_frame = state
                    .responder_stack
                    .last()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing previous responder after pop"))?;
                let linked_thread_id = current_frame.linked_thread_id.clone();
                state.active_agent_id = previous_frame.agent_id.clone();
                (
                    previous_frame.agent_id,
                    previous_frame.agent_name,
                    linked_thread_id,
                    state.responder_stack.len(),
                )
            }
        };

        let event = ThreadHandoffEvent {
            id: event_id,
            kind: request.kind,
            from_agent_id: from_agent_id.clone(),
            to_agent_id: to_agent_id.clone(),
            from_agent_name: from_agent_name.clone(),
            to_agent_name: Some(to_agent_name.clone()),
            requested_by: request.requested_by,
            reason: request.reason.clone(),
            summary: request.summary.clone(),
            linked_thread_id,
            approval_id: approval_id.clone(),
            stack_depth_before,
            stack_depth_after,
            created_at: now,
            approved_at: approval_id.as_ref().map(|_| now),
            completed_at: Some(now),
            failed_at: None,
            failure_reason: None,
        };
        state.pending_approval_id = None;
        state.events.push(event.clone());
        self.thread_handoff_states
            .write()
            .await
            .insert(thread_id.to_string(), state.clone());

        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .get_mut(thread_id)
                .ok_or_else(|| anyhow::anyhow!("thread {thread_id} disappeared during handoff"))?;
            thread.agent_name = Some(to_agent_name);
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::System,
                content: thread_handoff_system_message(&event),
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
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now,
                feedback: None,
            });
            thread.updated_at = now;
        }

        self.set_thread_execution_profile(thread_id, None).await;
        self.clear_thread_continuation_state(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        self.persist_thread_by_id(thread_id).await;
        Ok(event)
    }

    pub(super) async fn queue_thread_handoff_approval(
        &self,
        request: &PendingThreadHandoffActivation,
        pending_approval: &ToolPendingApproval,
    ) -> Result<AgentTask> {
        let title = match request.kind {
            ThreadHandoffKind::Push => format!(
                "Approve thread handoff to {}",
                request
                    .target_agent_id
                    .as_deref()
                    .map(canonical_agent_name)
                    .unwrap_or(MAIN_AGENT_NAME)
            ),
            ThreadHandoffKind::Return => "Approve thread handoff return".to_string(),
        };
        let task = self
            .enqueue_task(
                title,
                request.summary.clone(),
                "high",
                Some(serde_json::to_string(request)?),
                None,
                Vec::new(),
                None,
                "thread_handoff",
                None,
                None,
                Some(request.thread_id.clone()),
                Some("daemon".to_string()),
            )
            .await;
        if self
            .auto_approve_task_if_rule_matches(&task.id, &request.thread_id, pending_approval)
            .await
        {
            return Ok(task);
        }
        self.mark_task_awaiting_approval(&task.id, &request.thread_id, pending_approval)
            .await;
        self.record_operator_approval_requested(pending_approval)
            .await?;
        let thread_created_at = self
            .threads
            .read()
            .await
            .get(&request.thread_id)
            .map(|thread| thread.created_at)
            .unwrap_or_else(now_millis);
        let mut state = self
            .thread_handoff_state(&request.thread_id)
            .await
            .unwrap_or_else(|| {
                initial_thread_handoff_state(&request.thread_id, None, thread_created_at)
            });
        state.pending_approval_id = Some(pending_approval.approval_id.clone());
        self.thread_handoff_states
            .write()
            .await
            .insert(request.thread_id.clone(), state);
        self.persist_thread_by_id(&request.thread_id).await;
        Ok(task)
    }

    pub(super) async fn clear_pending_thread_handoff_approval(
        &self,
        thread_id: &str,
        approval_id: &str,
    ) {
        let thread_created_at = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.created_at)
            .unwrap_or_else(now_millis);
        let mut state = self
            .thread_handoff_state(thread_id)
            .await
            .unwrap_or_else(|| initial_thread_handoff_state(thread_id, None, thread_created_at));
        if state.pending_approval_id.as_deref() == Some(approval_id) {
            state.pending_approval_id = None;
            self.thread_handoff_states
                .write()
                .await
                .insert(thread_id.to_string(), state);
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub(super) fn thread_handoff_pending_approval(
        &self,
        request: &PendingThreadHandoffActivation,
        risk_level: &str,
    ) -> Result<ToolPendingApproval> {
        let approval_id = format!("thread-handoff-{}", Uuid::new_v4());
        Ok(ToolPendingApproval {
            approval_id: approval_id.clone(),
            execution_id: format!("thread-handoff-exec-{}", Uuid::new_v4()),
            command: thread_handoff_approval_command(request),
            rationale: request.reason.clone(),
            risk_level: risk_level.to_string(),
            blast_radius: "thread".to_string(),
            reasons: vec![request.reason.clone()],
            session_id: None,
        })
    }
}
