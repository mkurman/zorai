use crate::state::{chat, task};

#[cfg(not(test))]
thread_local! {
    pub(super) static SYSTEM_CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn convert_thread(t: crate::wire::AgentThread) -> chat::AgentThread {
    let window = chat::chat_window::MessageWindow::from_parts(
        t.total_message_count,
        t.loaded_message_start,
        t.loaded_message_end,
        t.messages.len(),
    );
    let messages: Vec<_> = t.messages.into_iter().map(convert_message).collect();
    let latest_turn_context_tokens = if window.end >= window.total {
        messages.iter().rev().find_map(|message| {
            let tokens = message.input_tokens.saturating_add(message.output_tokens);
            (tokens > 0).then_some(tokens)
        })
    } else {
        None
    };
    chat::AgentThread {
        id: t.id,
        agent_name: t.agent_name,
        profile_provider: t.profile_provider,
        profile_model: t.profile_model,
        profile_reasoning_effort: t.profile_reasoning_effort,
        profile_context_window_tokens: t.profile_context_window_tokens,
        title: t.title,
        created_at: t.created_at,
        updated_at: t.updated_at,
        messages,
        total_message_count: window.total,
        loaded_message_start: window.start,
        loaded_message_end: window.end,
        active_compaction_window_start: None,
        active_context_window_start: t.active_context_window_start,
        active_context_window_end: t.active_context_window_end,
        active_context_window_tokens: t.active_context_window_tokens,
        latest_turn_context_tokens,
        pinned_messages: t
            .pinned_messages
            .into_iter()
            .map(|message| chat::PinnedThreadMessage {
                message_id: message.message_id,
                absolute_index: message.absolute_index,
                role: match message.role {
                    crate::wire::MessageRole::System => chat::MessageRole::System,
                    crate::wire::MessageRole::User => chat::MessageRole::User,
                    crate::wire::MessageRole::Assistant => chat::MessageRole::Assistant,
                    crate::wire::MessageRole::Tool => chat::MessageRole::Tool,
                    crate::wire::MessageRole::Unknown => chat::MessageRole::Unknown,
                },
                content: message.content,
            })
            .collect(),
        older_page_pending: false,
        older_page_request_cooldown_until_tick: None,
        history_window_expanded: false,
        collapse_deadline_tick: None,
        total_input_tokens: t.total_input_tokens,
        total_output_tokens: t.total_output_tokens,
        thread_participants: t
            .thread_participants
            .into_iter()
            .map(|participant| chat::ThreadParticipantState {
                agent_id: participant.agent_id,
                agent_name: participant.agent_name,
                instruction: participant.instruction,
                status: participant.status,
                created_at: participant.created_at,
                updated_at: participant.updated_at,
                deactivated_at: participant.deactivated_at,
                last_contribution_at: participant.last_contribution_at,
                always_auto_response: participant.always_auto_response,
            })
            .collect(),
        queued_participant_suggestions: t
            .queued_participant_suggestions
            .into_iter()
            .map(|suggestion| chat::ThreadParticipantSuggestionVm {
                id: suggestion.id,
                target_agent_id: suggestion.target_agent_id,
                target_agent_name: suggestion.target_agent_name,
                instruction: suggestion.instruction,
                suggestion_kind: suggestion.suggestion_kind,
                force_send: suggestion.force_send,
                status: suggestion.status,
                created_at: suggestion.created_at,
                updated_at: suggestion.updated_at,
                auto_send_at: suggestion.auto_send_at,
                source_message_timestamp: suggestion.source_message_timestamp,
                error: suggestion.error,
            })
            .collect(),
        thread_handoff_state: t
            .thread_handoff_state
            .map(|state| chat::ThreadHandoffState {
                origin_agent_id: state.origin_agent_id,
                active_agent_id: state.active_agent_id,
                responder_stack: state
                    .responder_stack
                    .into_iter()
                    .map(|frame| chat::ThreadHandoffFrame {
                        agent_id: frame.agent_id,
                        agent_name: frame.agent_name,
                        entered_at: frame.entered_at,
                        linked_thread_id: frame.linked_thread_id,
                    })
                    .collect(),
                pending_approval_id: state.pending_approval_id,
            }),
        runtime_provider: None,
        runtime_model: None,
        runtime_reasoning_effort: None,
    }
}

pub(crate) fn convert_message(m: crate::wire::AgentMessage) -> chat::AgentMessage {
    chat::AgentMessage {
        id: m.id,
        role: match m.role {
            crate::wire::MessageRole::System => chat::MessageRole::System,
            crate::wire::MessageRole::User => chat::MessageRole::User,
            crate::wire::MessageRole::Assistant => chat::MessageRole::Assistant,
            crate::wire::MessageRole::Tool => chat::MessageRole::Tool,
            crate::wire::MessageRole::Unknown => chat::MessageRole::Unknown,
        },
        content: m.content,
        content_blocks: m
            .content_blocks
            .into_iter()
            .map(|block| match block {
                crate::wire::AgentContentBlock::Text { text } => {
                    chat::AgentContentBlock::Text { text }
                }
                crate::wire::AgentContentBlock::Image {
                    url,
                    data_url,
                    mime_type,
                } => chat::AgentContentBlock::Image {
                    url,
                    data_url,
                    mime_type,
                },
                crate::wire::AgentContentBlock::Audio {
                    url,
                    data_url,
                    mime_type,
                } => chat::AgentContentBlock::Audio {
                    url,
                    data_url,
                    mime_type,
                },
                crate::wire::AgentContentBlock::Video {
                    url,
                    data_url,
                    mime_type,
                } => chat::AgentContentBlock::Video {
                    url,
                    data_url,
                    mime_type,
                },
            })
            .collect(),
        reasoning: m.reasoning,
        author_agent_id: m.author_agent_id,
        author_agent_name: m.author_agent_name,
        is_operator_question: m.is_operator_question,
        operator_question_id: m.operator_question_id,
        operator_question_answer: m.operator_question_answer,
        provider_final_result_json: m.provider_final_result_json,
        tool_name: m.tool_name,
        tool_arguments: m.tool_arguments,
        tool_call_id: m.tool_call_id,
        tool_status: m.tool_status,
        weles_review: m.weles_review.map(|review| chat::WelesReviewMetaVm {
            weles_reviewed: review.weles_reviewed,
            verdict: review.verdict,
            reasons: review.reasons,
            audit_id: review.audit_id,
            security_override_mode: review.security_override_mode,
        }),
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        tps: m.tps,
        generation_ms: m.generation_ms,
        cost: m.cost,
        is_streaming: m.is_streaming,
        pinned_for_compaction: m.pinned_for_compaction,
        message_kind: m.message_kind,
        compaction_strategy: m.compaction_strategy,
        compaction_payload: m.compaction_payload,
        tool_output_preview_path: m.tool_output_preview_path,
        timestamp: m.timestamp,
        actions: Vec::new(),
        is_concierge_welcome: false,
        feedback: m.feedback,
    }
}

pub(crate) fn convert_task(t: crate::wire::AgentTask) -> task::AgentTask {
    task::AgentTask {
        id: t.id,
        title: t.title,
        description: t.description,
        thread_id: t.thread_id,
        parent_task_id: t.parent_task_id,
        parent_thread_id: t.parent_thread_id,
        created_at: t.created_at,
        status: t.status.map(|s| match s {
            crate::wire::TaskStatus::Queued => task::TaskStatus::Queued,
            crate::wire::TaskStatus::InProgress => task::TaskStatus::InProgress,
            crate::wire::TaskStatus::AwaitingApproval => task::TaskStatus::AwaitingApproval,
            crate::wire::TaskStatus::Blocked => task::TaskStatus::Blocked,
            crate::wire::TaskStatus::FailedAnalyzing => task::TaskStatus::FailedAnalyzing,
            crate::wire::TaskStatus::BudgetExceeded => task::TaskStatus::BudgetExceeded,
            crate::wire::TaskStatus::Completed => task::TaskStatus::Completed,
            crate::wire::TaskStatus::Failed => task::TaskStatus::Failed,
            crate::wire::TaskStatus::Cancelled => task::TaskStatus::Cancelled,
        }),
        progress: t.progress,
        session_id: t.session_id,
        goal_run_id: t.goal_run_id,
        goal_step_title: t.goal_step_title,
        command: t.command,
        awaiting_approval_id: t.awaiting_approval_id,
        blocked_reason: t.blocked_reason,
        sub_agent_def_id: t.sub_agent_def_id,
    }
}

pub(crate) fn convert_goal_run(r: crate::wire::GoalRun) -> task::GoalRun {
    task::GoalRun {
        id: r.id,
        title: r.title,
        thread_id: r.thread_id,
        root_thread_id: r.root_thread_id,
        active_thread_id: r.active_thread_id,
        execution_thread_ids: r.execution_thread_ids,
        session_id: r.session_id,
        status: r.status.map(|s| match s {
            crate::wire::GoalRunStatus::Queued => task::GoalRunStatus::Queued,
            crate::wire::GoalRunStatus::Planning => task::GoalRunStatus::Planning,
            crate::wire::GoalRunStatus::Running => task::GoalRunStatus::Running,
            crate::wire::GoalRunStatus::AwaitingApproval => task::GoalRunStatus::AwaitingApproval,
            crate::wire::GoalRunStatus::AwaitingReview => task::GoalRunStatus::AwaitingReview,
            crate::wire::GoalRunStatus::Paused => task::GoalRunStatus::Paused,
            crate::wire::GoalRunStatus::Blocked => task::GoalRunStatus::Blocked,
            crate::wire::GoalRunStatus::Completed => task::GoalRunStatus::Completed,
            crate::wire::GoalRunStatus::Failed => task::GoalRunStatus::Failed,
            crate::wire::GoalRunStatus::Cancelled => task::GoalRunStatus::Cancelled,
            crate::wire::GoalRunStatus::Contained => task::GoalRunStatus::Contained,
            crate::wire::GoalRunStatus::Compensated => task::GoalRunStatus::Compensated,
            crate::wire::GoalRunStatus::PartiallyCompensated => {
                task::GoalRunStatus::PartiallyCompensated
            }
            crate::wire::GoalRunStatus::BreakGlass => task::GoalRunStatus::BreakGlass,
        }),
        launch_assignment_snapshot: r
            .launch_assignment_snapshot
            .into_iter()
            .map(convert_goal_agent_assignment)
            .collect(),
        runtime_assignment_list: r
            .runtime_assignment_list
            .into_iter()
            .map(convert_goal_agent_assignment)
            .collect(),
        planner_owner_profile: r
            .planner_owner_profile
            .map(convert_goal_runtime_owner_profile),
        current_step_owner_profile: r
            .current_step_owner_profile
            .map(convert_goal_runtime_owner_profile),
        total_prompt_tokens: r.total_prompt_tokens,
        total_completion_tokens: r.total_completion_tokens,
        estimated_cost_usd: r.estimated_cost_usd,
        model_usage: r
            .model_usage
            .into_iter()
            .map(convert_goal_run_model_usage)
            .collect(),
        steps: r
            .steps
            .into_iter()
            .map(|step| task::GoalRunStep {
                id: step.id,
                title: step.title,
                status: step.status.map(|s| match s {
                    crate::wire::GoalRunStepStatus::Pending => task::GoalRunStatus::Queued,
                    crate::wire::GoalRunStepStatus::InProgress => task::GoalRunStatus::Running,
                    crate::wire::GoalRunStepStatus::Completed => task::GoalRunStatus::Completed,
                    crate::wire::GoalRunStepStatus::Failed => task::GoalRunStatus::Failed,
                    crate::wire::GoalRunStepStatus::Skipped => task::GoalRunStatus::Cancelled,
                }),
                order: step.position as u32,
                instructions: step.instructions,
                kind: format!("{:?}", step.kind).to_lowercase(),
                task_id: step.task_id,
                summary: step.summary,
                error: step.error,
            })
            .collect(),
        current_step_title: r.current_step_title,
        child_task_count: r.child_task_count,
        approval_count: r.approval_count,
        awaiting_approval_id: r.awaiting_approval_id,
        last_error: r.last_error,
        pending_review_report: r.pending_review_report,
        goal: r.goal,
        created_at: r.created_at,
        updated_at: r.updated_at,
        current_step_index: r.current_step_index,
        reflection_summary: r.reflection_summary,
        memory_updates: r.memory_updates,
        generated_skill_path: r.generated_skill_path,
        child_task_ids: r.child_task_ids,
        loaded_step_start: r.loaded_step_start,
        loaded_step_end: r.loaded_step_end,
        total_step_count: r.total_step_count,
        loaded_event_start: r.loaded_event_start,
        loaded_event_end: r.loaded_event_end,
        total_event_count: r.total_event_count,
        dossier: r.dossier.map(convert_goal_run_dossier),
        events: r
            .events
            .into_iter()
            .map(|event| task::GoalRunEvent {
                id: event.id,
                timestamp: event.timestamp,
                phase: event.phase,
                message: event.message,
                details: event.details,
                step_index: event.step_index,
                todo_snapshot: event
                    .todo_snapshot
                    .into_iter()
                    .map(|todo| convert_todo_with_fallback_step(todo, event.step_index))
                    .collect(),
            })
            .collect(),
        older_page_pending: false,
        older_page_request_cooldown_until_tick: None,
        sparse_update: r.sparse_update,
    }
}

fn convert_goal_runtime_owner_profile(
    profile: crate::wire::GoalRuntimeOwnerProfile,
) -> task::GoalRuntimeOwnerProfile {
    task::GoalRuntimeOwnerProfile {
        agent_label: profile.agent_label,
        provider: profile.provider,
        model: profile.model,
        reasoning_effort: profile.reasoning_effort,
    }
}

fn convert_goal_agent_assignment(
    assignment: crate::wire::GoalAgentAssignment,
) -> task::GoalAgentAssignment {
    task::GoalAgentAssignment {
        role_id: assignment.role_id,
        enabled: assignment.enabled,
        provider: assignment.provider,
        model: assignment.model,
        reasoning_effort: assignment.reasoning_effort,
        inherit_from_main: assignment.inherit_from_main,
    }
}

fn convert_goal_run_model_usage(usage: crate::wire::GoalRunModelUsage) -> task::GoalRunModelUsage {
    task::GoalRunModelUsage {
        provider: usage.provider,
        model: usage.model,
        request_count: usage.request_count,
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        estimated_cost_usd: usage.estimated_cost_usd,
        duration_ms: usage.duration_ms,
    }
}

fn convert_goal_evidence(record: crate::wire::GoalEvidenceRecord) -> task::GoalEvidenceRecord {
    task::GoalEvidenceRecord {
        id: record.id,
        title: record.title,
        source: record.source,
        uri: record.uri,
        summary: record.summary,
        captured_at: record.captured_at,
    }
}

fn convert_goal_proof_check(
    record: crate::wire::GoalProofCheckRecord,
) -> task::GoalProofCheckRecord {
    task::GoalProofCheckRecord {
        id: record.id,
        title: record.title,
        state: record.state,
        summary: record.summary,
        evidence_ids: record.evidence_ids,
        resolved_at: record.resolved_at,
    }
}

fn convert_goal_run_report(record: crate::wire::GoalRunReportRecord) -> task::GoalRunReportRecord {
    task::GoalRunReportRecord {
        summary: record.summary,
        state: record.state,
        notes: record.notes,
        evidence: record
            .evidence
            .into_iter()
            .map(convert_goal_evidence)
            .collect(),
        proof_checks: record
            .proof_checks
            .into_iter()
            .map(convert_goal_proof_check)
            .collect(),
        generated_at: record.generated_at,
    }
}

fn convert_goal_resume_decision(
    record: crate::wire::GoalResumeDecisionRecord,
) -> task::GoalResumeDecisionRecord {
    task::GoalResumeDecisionRecord {
        action: record.action,
        reason_code: record.reason_code,
        reason: record.reason,
        details: record.details,
        decided_at: record.decided_at,
        projection_state: record.projection_state,
    }
}

fn convert_goal_delivery_unit(
    record: crate::wire::GoalDeliveryUnitRecord,
) -> task::GoalDeliveryUnitRecord {
    task::GoalDeliveryUnitRecord {
        id: record.id,
        title: record.title,
        status: record.status,
        execution_binding: record.execution_binding,
        verification_binding: record.verification_binding,
        summary: record.summary,
        proof_checks: record
            .proof_checks
            .into_iter()
            .map(convert_goal_proof_check)
            .collect(),
        evidence: record
            .evidence
            .into_iter()
            .map(convert_goal_evidence)
            .collect(),
        report: record.report.map(convert_goal_run_report),
    }
}

fn convert_goal_run_dossier(record: crate::wire::GoalRunDossier) -> task::GoalRunDossier {
    task::GoalRunDossier {
        units: record
            .units
            .into_iter()
            .map(convert_goal_delivery_unit)
            .collect(),
        projection_state: record.projection_state,
        latest_resume_decision: record
            .latest_resume_decision
            .map(convert_goal_resume_decision),
        report: record.report.map(convert_goal_run_report),
        summary: record.summary,
        projection_error: record.projection_error,
    }
}

pub(crate) fn convert_checkpoint_summary(
    checkpoint: crate::wire::CheckpointSummary,
) -> task::GoalRunCheckpointSummary {
    task::GoalRunCheckpointSummary {
        id: checkpoint.id,
        checkpoint_type: checkpoint.checkpoint_type,
        step_index: checkpoint.step_index,
        task_count: checkpoint.task_count,
        context_summary_preview: checkpoint.context_summary_preview,
        created_at: checkpoint.created_at,
    }
}

pub(crate) fn convert_todo(t: crate::wire::TodoItem) -> task::TodoItem {
    convert_todo_with_fallback_step(t, None)
}

pub(crate) fn convert_todo_with_fallback_step(
    t: crate::wire::TodoItem,
    fallback_step_index: Option<usize>,
) -> task::TodoItem {
    task::TodoItem {
        id: t.id,
        content: t.content,
        status: t.status.map(|status| match status {
            crate::wire::TodoStatus::Pending => task::TodoStatus::Pending,
            crate::wire::TodoStatus::InProgress => task::TodoStatus::InProgress,
            crate::wire::TodoStatus::Completed => task::TodoStatus::Completed,
            crate::wire::TodoStatus::Blocked => task::TodoStatus::Blocked,
        }),
        position: t.position,
        step_index: t.step_index.or(fallback_step_index),
        created_at: t.created_at,
        updated_at: t.updated_at,
    }
}
