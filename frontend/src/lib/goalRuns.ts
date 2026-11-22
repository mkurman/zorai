export type GoalRunStatus =
    | "queued"
    | "planning"
    | "running"
    | "awaiting_approval"
    | "awaiting_review"
    | "paused"
    | "completed"
    | "failed"
    | "cancelled";

export type GoalRunControlAction =
    | "pause"
    | "resume"
    | "cancel"
    | "accept"
    | "soft_reject"
    | "hard_reject";

export type GoalRunStepKind = "reason" | "command" | "research" | "memory" | "skill" | "unknown";

export type TodoStatus = "pending" | "in_progress" | "completed" | "blocked";

export interface TodoItem {
    id: string;
    content: string;
    status: TodoStatus;
    position: number;
    step_index?: number | null;
    created_at?: number | null;
    updated_at?: number | null;
}

export interface GoalRunStep {
    id: string;
    title: string;
    kind: GoalRunStepKind;
    position?: number | null;
    status?: string | null;
    instructions?: string;
    success_condition?: string | null;
    session_id?: string | null;
    task_id?: string | null;
    summary?: string | null;
    error?: string | null;
}

export interface GoalRunEvent {
    id: string;
    timestamp: number;
    phase: string;
    message: string;
    details?: string | null;
    step_index?: number | null;
    todo_snapshot: TodoItem[];
}

export interface GoalRunModelUsage {
    provider: string;
    model: string;
    request_count: number;
    prompt_tokens: number;
    completion_tokens: number;
    estimated_cost_usd?: number | null;
    duration_ms?: number | null;
}

export interface GoalRuntimeOwnerProfile {
    agent_label: string;
    provider: string;
    model: string;
    reasoning_effort?: string | null;
}

export interface GoalAgentAssignment {
    role_id: string;
    enabled: boolean;
    provider: string;
    model: string;
    reasoning_effort?: string | null;
    inherit_from_main: boolean;
}

export interface GoalEvidenceRecord {
    id: string;
    title: string;
    source?: string | null;
    uri?: string | null;
    summary?: string | null;
    captured_at?: number | null;
}

export interface GoalProofCheckRecord {
    id: string;
    title: string;
    state: string;
    summary?: string | null;
    evidence_ids: string[];
    resolved_at?: number | null;
}

export interface GoalRunReportRecord {
    summary: string;
    state: string;
    notes: string[];
    evidence: GoalEvidenceRecord[];
    proof_checks: GoalProofCheckRecord[];
    generated_at?: number | null;
}

export interface GoalResumeDecisionRecord {
    action: string;
    reason_code: string;
    reason?: string | null;
    details: string[];
    decided_at?: number | null;
    projection_state: string;
}

export interface GoalDeliveryUnitRecord {
    id: string;
    title: string;
    status: string;
    execution_binding: string;
    verification_binding: string;
    summary?: string | null;
    proof_checks: GoalProofCheckRecord[];
    evidence: GoalEvidenceRecord[];
    report?: GoalRunReportRecord | null;
}

export interface GoalRunDossier {
    units: GoalDeliveryUnitRecord[];
    projection_state: string;
    latest_resume_decision?: GoalResumeDecisionRecord | null;
    report?: GoalRunReportRecord | null;
    summary?: string | null;
    projection_error?: string | null;
}

export interface GoalRun {
    id: string;
    title: string;
    goal: string;
    client_request_id?: string | null;
    status: GoalRunStatus;
    priority?: string | null;
    created_at: number;
    started_at?: number | null;
    completed_at?: number | null;
    thread_id?: string | null;
    root_thread_id?: string | null;
    supervision_thread_id?: string | null;
    active_thread_id?: string | null;
    execution_thread_ids?: string[];
    current_step_index?: number | null;
    current_step_title?: string | null;
    current_step_kind?: GoalRunStepKind | null;
    replan_count: number;
    plan_summary?: string | null;
    reflection_summary?: string | null;
    result?: string | null;
    error?: string | null;
    last_error?: string | null;
    failure_cause?: string | null;
    memory_updates?: string[];
    generated_skill_path?: string | null;
    child_task_ids?: string[];
    child_task_count?: number | null;
    approval_count?: number | null;
    duration_ms?: number | null;
    session_id?: string | null;
    awaiting_approval_id?: string | null;
    active_task_id?: string | null;
    pending_review_report?: string | null;
    total_prompt_tokens?: number | null;
    total_completion_tokens?: number | null;
    estimated_cost_usd?: number | null;
    model_usage?: GoalRunModelUsage[];
    launch_assignment_snapshot?: GoalAgentAssignment[];
    runtime_assignment_list?: GoalAgentAssignment[];
    planner_owner_profile?: GoalRuntimeOwnerProfile | null;
    current_step_owner_profile?: GoalRuntimeOwnerProfile | null;
    steps?: GoalRunStep[];
    events?: GoalRunEvent[];
    dossier?: GoalRunDossier | null;
}

export interface StartGoalRunPayload {
    goal: string;
    title?: string | null;
    sessionId?: string | null;
    priority?: string | null;
    threadId?: string | null;
    targetAgentId?: string | null;
    clientRequestId?: string | null;
    requiresApproval?: boolean;
    launchAssignments?: GoalAgentAssignment[];
}

import { getBridge } from "./bridge";

function toStepKind(value: unknown): GoalRunStepKind {
    if (typeof value !== "string") {
        return "unknown";
    }

    switch (value) {
        case "reason":
        case "command":
        case "research":
        case "memory":
        case "skill":
            return value;
        default:
            return "unknown";
    }
}

function toStatus(value: unknown): GoalRunStatus {
    if (typeof value !== "string") {
        return "queued";
    }

    switch (value) {
        case "queued":
        case "planning":
        case "running":
        case "awaiting_approval":
        case "awaiting_review":
        case "paused":
        case "completed":
        case "failed":
        case "cancelled":
            return value;
        default:
            return value as GoalRunStatus;
    }
}

function toStringArray(value: unknown): string[] {
    return Array.isArray(value)
        ? value.filter((entry): entry is string => typeof entry === "string" && entry.trim().length > 0)
        : [];
}

function parseJsonValue(value: unknown): unknown {
    if (typeof value !== "string" || !value.trim()) {
        return value;
    }

    try {
        return JSON.parse(value);
    } catch {
        return value;
    }
}

function toNumberOrNull(value: unknown): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function toStringOrEmpty(value: unknown): string {
    return typeof value === "string" ? value : "";
}

function normalizeOwnerProfile(raw: unknown): GoalRuntimeOwnerProfile | null {
    const profile = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!profile) {
        return null;
    }

    const agentLabel = toStringOrEmpty(profile.agent_label ?? profile.agentLabel);
    const provider = toStringOrEmpty(profile.provider);
    const model = toStringOrEmpty(profile.model);
    if (!agentLabel || !provider || !model) {
        return null;
    }

    return {
        agent_label: agentLabel,
        provider,
        model,
        reasoning_effort: typeof profile.reasoning_effort === "string"
            ? profile.reasoning_effort
            : typeof profile.reasoningEffort === "string"
                ? profile.reasoningEffort
                : null,
    };
}

function normalizeAssignmentList(raw: unknown): GoalAgentAssignment[] {
    if (!Array.isArray(raw)) {
        return [];
    }

    return raw
        .map((entry): GoalAgentAssignment | null => {
            const assignment = (entry && typeof entry === "object") ? (entry as Record<string, unknown>) : {};
            const roleId = toStringOrEmpty(assignment.role_id ?? assignment.roleId);
            if (!roleId) {
                return null;
            }

            return {
                role_id: roleId,
                enabled: typeof assignment.enabled === "boolean" ? assignment.enabled : true,
                provider: toStringOrEmpty(assignment.provider),
                model: toStringOrEmpty(assignment.model),
                reasoning_effort: typeof assignment.reasoning_effort === "string"
                    ? assignment.reasoning_effort
                    : typeof assignment.reasoningEffort === "string"
                        ? assignment.reasoningEffort
                        : null,
                inherit_from_main: Boolean(assignment.inherit_from_main ?? assignment.inheritFromMain),
            };
        })
        .filter((entry): entry is GoalAgentAssignment => entry !== null);
}

function normalizeModelUsage(raw: unknown): GoalRunModelUsage[] {
    if (!Array.isArray(raw)) {
        return [];
    }

    return raw
        .map((entry): GoalRunModelUsage | null => {
            const usage = (entry && typeof entry === "object") ? (entry as Record<string, unknown>) : {};
            const provider = typeof usage.provider === "string" ? usage.provider : "";
            const model = typeof usage.model === "string" ? usage.model : "";
            if (!provider || !model) {
                return null;
            }
            return {
                provider,
                model,
                request_count: toNumberOrNull(usage.request_count ?? usage.requestCount) ?? 0,
                prompt_tokens: toNumberOrNull(usage.prompt_tokens ?? usage.promptTokens) ?? 0,
                completion_tokens: toNumberOrNull(usage.completion_tokens ?? usage.completionTokens) ?? 0,
                estimated_cost_usd: toNumberOrNull(usage.estimated_cost_usd ?? usage.estimatedCostUsd),
                duration_ms: toNumberOrNull(usage.duration_ms ?? usage.durationMs),
            };
        })
        .filter((entry): entry is GoalRunModelUsage => entry !== null);
}

function normalizeEvidence(raw: unknown): GoalEvidenceRecord | null {
    const evidence = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!evidence) return null;
    const id = toStringOrEmpty(evidence.id);
    const title = toStringOrEmpty(evidence.title);
    if (!id || !title) return null;
    return {
        id,
        title,
        source: typeof evidence.source === "string" ? evidence.source : null,
        uri: typeof evidence.uri === "string" ? evidence.uri : null,
        summary: typeof evidence.summary === "string" ? evidence.summary : null,
        captured_at: toNumberOrNull(evidence.captured_at ?? evidence.capturedAt),
    };
}

function normalizeProofCheck(raw: unknown): GoalProofCheckRecord | null {
    const proof = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!proof) return null;
    const id = toStringOrEmpty(proof.id);
    const title = toStringOrEmpty(proof.title);
    if (!id || !title) return null;
    return {
        id,
        title,
        state: toStringOrEmpty(proof.state) || "pending",
        summary: typeof proof.summary === "string" ? proof.summary : null,
        evidence_ids: toStringArray(proof.evidence_ids ?? proof.evidenceIds),
        resolved_at: toNumberOrNull(proof.resolved_at ?? proof.resolvedAt),
    };
}

function normalizeReport(raw: unknown): GoalRunReportRecord | null {
    const report = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!report) return null;
    const summary = toStringOrEmpty(report.summary);
    const state = toStringOrEmpty(report.state);
    const proofRaw = report.proof_checks ?? report.proofChecks;
    if (!summary && !state) return null;
    return {
        summary,
        state: state || "pending",
        notes: toStringArray(report.notes),
        evidence: Array.isArray(report.evidence)
            ? report.evidence.map(normalizeEvidence).filter((entry): entry is GoalEvidenceRecord => entry !== null)
            : [],
        proof_checks: Array.isArray(proofRaw)
            ? proofRaw.map(normalizeProofCheck).filter((entry): entry is GoalProofCheckRecord => entry !== null)
            : [],
        generated_at: toNumberOrNull(report.generated_at ?? report.generatedAt),
    };
}

function normalizeResumeDecision(raw: unknown): GoalResumeDecisionRecord | null {
    const decision = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!decision) return null;
    const action = toStringOrEmpty(decision.action);
    const reasonCode = toStringOrEmpty(decision.reason_code ?? decision.reasonCode);
    if (!action && !reasonCode) return null;
    return {
        action: action || "continue",
        reason_code: reasonCode || "unknown",
        reason: typeof decision.reason === "string" ? decision.reason : null,
        details: toStringArray(decision.details),
        decided_at: toNumberOrNull(decision.decided_at ?? decision.decidedAt),
        projection_state: toStringOrEmpty(decision.projection_state ?? decision.projectionState) || "unknown",
    };
}

function normalizeDeliveryUnit(raw: unknown): GoalDeliveryUnitRecord | null {
    const unit = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!unit) return null;
    const id = toStringOrEmpty(unit.id);
    const title = toStringOrEmpty(unit.title);
    if (!id || !title) return null;
    const proofRaw = unit.proof_checks ?? unit.proofChecks;
    return {
        id,
        title,
        status: toStringOrEmpty(unit.status) || "pending",
        execution_binding: toStringOrEmpty(unit.execution_binding ?? unit.executionBinding),
        verification_binding: toStringOrEmpty(unit.verification_binding ?? unit.verificationBinding),
        summary: typeof unit.summary === "string" ? unit.summary : null,
        proof_checks: Array.isArray(proofRaw)
            ? proofRaw.map(normalizeProofCheck).filter((entry): entry is GoalProofCheckRecord => entry !== null)
            : [],
        evidence: Array.isArray(unit.evidence)
            ? unit.evidence.map(normalizeEvidence).filter((entry): entry is GoalEvidenceRecord => entry !== null)
            : [],
        report: normalizeReport(unit.report),
    };
}

function normalizeDossier(raw: unknown): GoalRunDossier | null {
    const dossier = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : null;
    if (!dossier) return null;
    const unitsRaw = Array.isArray(dossier.units) ? dossier.units : [];
    return {
        units: unitsRaw.map(normalizeDeliveryUnit).filter((entry): entry is GoalDeliveryUnitRecord => entry !== null),
        projection_state: toStringOrEmpty(dossier.projection_state ?? dossier.projectionState) || "unknown",
        latest_resume_decision: normalizeResumeDecision(dossier.latest_resume_decision ?? dossier.latestResumeDecision),
        report: normalizeReport(dossier.report),
        summary: typeof dossier.summary === "string" ? dossier.summary : null,
        projection_error: typeof dossier.projection_error === "string"
            ? dossier.projection_error
            : typeof dossier.projectionError === "string"
                ? dossier.projectionError
                : null,
    };
}

function toTodoStatus(value: unknown): TodoStatus {
    switch (value) {
        case "in_progress":
        case "completed":
        case "blocked":
            return value;
        default:
            return "pending";
    }
}

function normalizeTodoItem(raw: unknown, index: number): TodoItem {
    const item = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    return {
        id: typeof item.id === "string" && item.id ? item.id : `todo-${index}`,
        content: typeof item.content === "string" && item.content
            ? item.content
            : typeof item.title === "string" && item.title
                ? item.title
                : `Todo ${index + 1}`,
        status: toTodoStatus(item.status),
        position: typeof item.position === "number" ? item.position : index,
        step_index: toNumberOrNull(item.step_index ?? item.stepIndex),
        created_at: toNumberOrNull(item.created_at ?? item.createdAt),
        updated_at: toNumberOrNull(item.updated_at ?? item.updatedAt),
    };
}

function normalizeEvent(raw: unknown, index: number): GoalRunEvent {
    const event = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    const todoSnapshotRaw = Array.isArray(event.todo_snapshot)
        ? event.todo_snapshot
        : Array.isArray(event.todoSnapshot)
            ? event.todoSnapshot
            : [];
    return {
        id: typeof event.id === "string" && event.id ? event.id : `goal-event-${index}`,
        timestamp: typeof event.timestamp === "number" ? event.timestamp : Date.now(),
        phase: typeof event.phase === "string" && event.phase ? event.phase : "event",
        message: typeof event.message === "string" && event.message ? event.message : "Goal updated",
        details: typeof event.details === "string" ? event.details : null,
        step_index: toNumberOrNull(event.step_index ?? event.stepIndex),
        todo_snapshot: todoSnapshotRaw.map((item, todoIndex) => normalizeTodoItem(item, todoIndex)),
    };
}

function normalizeStep(raw: unknown, index: number): GoalRunStep {
    const step = (raw && typeof raw === "object") ? (raw as Record<string, unknown>) : {};
    return {
        id: typeof step.id === "string" && step.id ? step.id : `step-${index}`,
        title: typeof step.title === "string" && step.title ? step.title : `Step ${index + 1}`,
        kind: toStepKind(step.kind),
        position: toNumberOrNull(step.position),
        status: typeof step.status === "string" ? step.status : null,
        instructions: typeof step.instructions === "string" ? step.instructions : "",
        success_condition: typeof step.success_condition === "string"
            ? step.success_condition
            : typeof step.success_criteria === "string"
                ? step.success_criteria
            : typeof step.successCondition === "string"
                ? step.successCondition
                : null,
        session_id: typeof step.session_id === "string"
            ? step.session_id
            : typeof step.sessionId === "string"
                ? step.sessionId
                : null,
        task_id: typeof step.task_id === "string"
            ? step.task_id
            : typeof step.taskId === "string"
                ? step.taskId
                : null,
        summary: typeof step.summary === "string" ? step.summary : null,
        error: typeof step.error === "string" ? step.error : null,
    };
}

export function normalizeGoalRun(raw: unknown): GoalRun | null {
    if (!raw || typeof raw !== "object") {
        return null;
    }

    const goalRun = raw as Record<string, unknown>;
    const id = typeof goalRun.id === "string" ? goalRun.id : "";
    const goal = typeof goalRun.goal === "string"
        ? goalRun.goal
        : typeof goalRun.prompt === "string"
            ? goalRun.prompt
            : "";

    if (!id || !goal) {
        return null;
    }

    const stepsRaw = Array.isArray(goalRun.steps) ? goalRun.steps : [];
    const normalizedSteps = stepsRaw.map((step, index) => normalizeStep(step, index));
    const eventsRaw = Array.isArray(goalRun.events) ? goalRun.events : [];
    const normalizedEvents = eventsRaw.map((event, index) => normalizeEvent(event, index));
    const currentStepIndex = toNumberOrNull(goalRun.current_step_index ?? goalRun.currentStepIndex);
    const derivedCurrentStep = typeof currentStepIndex === "number" ? normalizedSteps[currentStepIndex] : null;
    const currentStepKindRaw = goalRun.current_step_kind ?? goalRun.currentStepKind ?? derivedCurrentStep?.kind;

    return {
        id,
        title: typeof goalRun.title === "string" && goalRun.title ? goalRun.title : goal.slice(0, 72),
        goal,
        client_request_id: typeof goalRun.client_request_id === "string"
            ? goalRun.client_request_id
            : typeof goalRun.clientRequestId === "string"
                ? goalRun.clientRequestId
                : null,
        status: toStatus(goalRun.status),
        priority: typeof goalRun.priority === "string" ? goalRun.priority : null,
        created_at: typeof goalRun.created_at === "number" ? goalRun.created_at : Date.now(),
        started_at: toNumberOrNull(goalRun.started_at),
        completed_at: toNumberOrNull(goalRun.completed_at),
        thread_id: typeof goalRun.thread_id === "string"
            ? goalRun.thread_id
            : typeof goalRun.threadId === "string"
                ? goalRun.threadId
                : null,
        root_thread_id: typeof goalRun.root_thread_id === "string"
            ? goalRun.root_thread_id
            : typeof goalRun.rootThreadId === "string"
                ? goalRun.rootThreadId
                : null,
        supervision_thread_id: typeof goalRun.supervision_thread_id === "string"
            ? goalRun.supervision_thread_id
            : typeof goalRun.supervisionThreadId === "string"
                ? goalRun.supervisionThreadId
                : null,
        active_thread_id: typeof goalRun.active_thread_id === "string"
            ? goalRun.active_thread_id
            : typeof goalRun.activeThreadId === "string"
                ? goalRun.activeThreadId
                : null,
        execution_thread_ids: toStringArray(parseJsonValue(
            goalRun.execution_thread_ids
            ?? goalRun.executionThreadIds
            ?? goalRun.execution_thread_ids_json,
        )),
        current_step_index: currentStepIndex,
        current_step_title: typeof goalRun.current_step_title === "string"
            ? goalRun.current_step_title
            : typeof goalRun.currentStepTitle === "string"
                ? goalRun.currentStepTitle
                : derivedCurrentStep?.title ?? null,
        current_step_kind: currentStepKindRaw == null ? null : toStepKind(currentStepKindRaw),
        replan_count: typeof goalRun.replan_count === "number"
            ? goalRun.replan_count
            : typeof goalRun.replanCount === "number"
                ? goalRun.replanCount
                : 0,
        plan_summary: typeof goalRun.plan_summary === "string"
            ? goalRun.plan_summary
            : typeof goalRun.planSummary === "string"
                ? goalRun.planSummary
                : null,
        reflection_summary: typeof goalRun.reflection_summary === "string"
            ? goalRun.reflection_summary
            : typeof goalRun.reflectionSummary === "string"
                ? goalRun.reflectionSummary
                : null,
        result: typeof goalRun.result === "string" ? goalRun.result : null,
        error: typeof goalRun.error === "string" ? goalRun.error : null,
        last_error: typeof goalRun.last_error === "string"
            ? goalRun.last_error
            : typeof goalRun.lastError === "string"
                ? goalRun.lastError
                : null,
        failure_cause: typeof goalRun.failure_cause === "string"
            ? goalRun.failure_cause
            : typeof goalRun.failureCause === "string"
                ? goalRun.failureCause
                : null,
        memory_updates: toStringArray(parseJsonValue(
            goalRun.memory_updates
            ?? goalRun.memoryUpdates
            ?? goalRun.memory_updates_json,
        )),
        generated_skill_path: typeof goalRun.generated_skill_path === "string"
            ? goalRun.generated_skill_path
            : typeof goalRun.generatedSkillPath === "string"
                ? goalRun.generatedSkillPath
                : null,
        child_task_ids: toStringArray(parseJsonValue(
            goalRun.child_task_ids
            ?? goalRun.childTaskIds
            ?? goalRun.child_task_ids_json,
        )),
        child_task_count: toNumberOrNull(goalRun.child_task_count ?? goalRun.childTaskCount),
        approval_count: toNumberOrNull(goalRun.approval_count ?? goalRun.approvalCount),
        duration_ms: toNumberOrNull(goalRun.duration_ms ?? goalRun.durationMs),
        total_prompt_tokens: toNumberOrNull(goalRun.total_prompt_tokens ?? goalRun.totalPromptTokens),
        total_completion_tokens: toNumberOrNull(goalRun.total_completion_tokens ?? goalRun.totalCompletionTokens),
        estimated_cost_usd: toNumberOrNull(goalRun.estimated_cost_usd ?? goalRun.estimatedCostUsd),
        model_usage: normalizeModelUsage(parseJsonValue(
            goalRun.model_usage
            ?? goalRun.modelUsage
            ?? goalRun.model_usage_json,
        )),
        launch_assignment_snapshot: normalizeAssignmentList(
            parseJsonValue(
                goalRun.launch_assignment_snapshot
                ?? goalRun.launchAssignmentSnapshot
                ?? goalRun.launch_assignment_snapshot_json,
            ),
        ),
        runtime_assignment_list: normalizeAssignmentList(
            parseJsonValue(
                goalRun.runtime_assignment_list
                ?? goalRun.runtimeAssignmentList
                ?? goalRun.runtime_assignment_list_json,
            ),
        ),
        planner_owner_profile: normalizeOwnerProfile(
            parseJsonValue(
                goalRun.planner_owner_profile
                ?? goalRun.plannerOwnerProfile
                ?? goalRun.planner_owner_profile_json,
            ),
        ),
        current_step_owner_profile: normalizeOwnerProfile(
            parseJsonValue(
                goalRun.current_step_owner_profile
                ?? goalRun.currentStepOwnerProfile
                ?? goalRun.current_step_owner_profile_json,
            ),
        ),
        session_id: typeof goalRun.session_id === "string"
            ? goalRun.session_id
            : typeof goalRun.sessionId === "string"
                ? goalRun.sessionId
                : null,
        awaiting_approval_id: typeof goalRun.awaiting_approval_id === "string"
            ? goalRun.awaiting_approval_id
            : typeof goalRun.awaitingApprovalId === "string"
                ? goalRun.awaitingApprovalId
                : null,
        active_task_id: typeof goalRun.active_task_id === "string"
            ? goalRun.active_task_id
            : typeof goalRun.activeTaskId === "string"
                ? goalRun.activeTaskId
                : null,
        pending_review_report: typeof goalRun.pending_review_report === "string"
            ? goalRun.pending_review_report
            : typeof goalRun.pendingReviewReport === "string"
                ? goalRun.pendingReviewReport
                : null,
        steps: normalizedSteps,
        events: normalizedEvents,
        dossier: normalizeDossier(parseJsonValue(goalRun.dossier ?? goalRun.dossier_json)),
    };
}

export function normalizeGoalRunList(raw: unknown): GoalRun[] {
    if (!Array.isArray(raw)) {
        return [];
    }

    return raw
        .map((entry) => normalizeGoalRun(entry))
        .filter((entry): entry is GoalRun => Boolean(entry))
        .sort((a, b) => (b.created_at || 0) - (a.created_at || 0));
}

export function goalRunSupportAvailable(): boolean {
    const bridge = getBridge();
    return Boolean(bridge?.agentListGoalRuns && bridge?.agentStartGoalRun && bridge?.agentControlGoalRun);
}

export async function fetchGoalRuns(): Promise<GoalRun[]> {
    const bridge = getBridge();
    try {
        if (!bridge?.agentListGoalRuns) {
            return await fetchGoalRunsFromDatabase();
        }
        const result = await bridge.agentListGoalRuns();
        const normalized = normalizeGoalRunList(result);
        return normalized.length > 0 ? normalized : await fetchGoalRunsFromDatabase();
    } catch {
        return await fetchGoalRunsFromDatabase();
    }
}

export async function fetchGoalRun(goalRunId: string): Promise<GoalRun | null> {
    const bridge = getBridge();
    if (!bridge?.agentGetGoalRun) {
        return null;
    }
    try {
        return normalizeGoalRun(await bridge.agentGetGoalRun(goalRunId));
    } catch {
        return null;
    }
}

export function upsertGoalRun(goalRuns: GoalRun[], goalRun: GoalRun): GoalRun[] {
    const index = goalRuns.findIndex((entry) => entry.id === goalRun.id);
    if (index < 0) {
        return [goalRun, ...goalRuns].sort((a, b) => (b.created_at || 0) - (a.created_at || 0));
    }
    const next = goalRuns.slice();
    next[index] = goalRun;
    return next;
}

async function fetchGoalRunsFromDatabase(): Promise<GoalRun[]> {
    const page = await getBridge()?.dbQueryDatabaseRows?.({
        tableName: "goal_runs",
        offset: 0,
        limit: 500,
        sortColumn: "created_at",
        sortDirection: "desc",
    });
    return normalizeGoalRunList(
        databasePageRows(page).filter((row) => {
            if (!row || typeof row !== "object") {
                return false;
            }
            return (row as { deleted_at?: unknown }).deleted_at == null;
        }),
    );
}

function databasePageRows(page: unknown): unknown[] {
    const rows = page && typeof page === "object" && Array.isArray((page as { rows?: unknown }).rows)
        ? (page as { rows: unknown[] }).rows
        : [];

    return rows.map((row) => {
        if (row && typeof row === "object" && "values" in row) {
            const values = (row as { values?: unknown }).values;
            return values && typeof values === "object" ? values : {};
        }
        return row;
    });
}

export async function startGoalRun(payload: StartGoalRunPayload): Promise<GoalRun | null> {
    const bridge = getBridge();
    if (!bridge?.agentStartGoalRun) {
        return null;
    }

    try {
        const result = await bridge.agentStartGoalRun({
            ...payload,
            clientRequestId: payload.clientRequestId
                ?? (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
                    ? crypto.randomUUID()
                    : `goal-run-${Date.now()}`),
        });
        return normalizeGoalRun(result);
    } catch {
        return null;
    }
}

export async function controlGoalRun(
    goalRunId: string,
    action: GoalRunControlAction,
    stepIndex?: number | null,
    explanation?: string | null,
): Promise<boolean> {
    const bridge = getBridge();
    if (!bridge?.agentControlGoalRun) {
        return false;
    }

    try {
        const payloadJson = explanation && explanation.trim()
            ? JSON.stringify({ explanation: explanation.trim() })
            : undefined;
        const result = await bridge.agentControlGoalRun(goalRunId, action, stepIndex ?? null, payloadJson);
        if (typeof result === "boolean") {
            return result;
        }
        if (result && typeof result === "object") {
            const payload = result as Record<string, unknown>;
            if (typeof payload.ok === "boolean") {
                return payload.ok;
            }
            if (typeof payload.success === "boolean") {
                return payload.success;
            }
        }
        return true;
    } catch {
        return false;
    }
}

export function isGoalRunTerminal(goalRun: GoalRun | GoalRunStatus): boolean {
    const status = typeof goalRun === "string" ? goalRun : goalRun.status;
    return status === "completed" || status === "failed" || status === "cancelled";
}

export function isGoalRunActive(goalRun: GoalRun): boolean {
    return !isGoalRunTerminal(goalRun);
}

export function goalRunsNeedAutoRefresh(goalRuns: Array<Pick<GoalRun, "status">>): boolean {
    return goalRuns.some((goalRun) => !isGoalRunTerminal(goalRun.status));
}

export function formatGoalRunStatus(status: GoalRunStatus): string {
    switch (status) {
        case "awaiting_approval":
            return "Awaiting approval";
        case "awaiting_review":
            return "Awaiting review";
        default:
            return status.replace(/_/g, " ");
    }
}

export function goalRunStatusColor(status: GoalRunStatus): string {
    switch (status) {
        case "planning":
            return "var(--mission)";
        case "running":
            return "var(--accent)";
        case "awaiting_approval":
            return "var(--approval)";
        case "awaiting_review":
            return "var(--approval)";
        case "paused":
            return "var(--warning)";
        case "completed":
            return "var(--success)";
        case "failed":
            return "var(--danger)";
        case "cancelled":
            return "var(--text-muted)";
        default:
            return "var(--text-secondary)";
    }
}

export function summarizeGoalRunStep(goalRun: GoalRun): string {
    if (goalRun.status === "awaiting_review") {
        return "Awaiting supervisor review";
    }
    if (goalRun.status === "queued") {
        return "Waiting for worker";
    }
    if (goalRun.status === "paused") {
        return "Paused";
    }
    if (goalRun.status === "completed") {
        return "Accepted";
    }
    if (goalRun.status === "failed") {
        return goalRun.last_error || goalRun.error || "Failed";
    }
    if (goalRun.status === "cancelled") {
        return "Cancelled";
    }
    const todos = latestGoalRunTodoSnapshot(goalRun);
    const working = todos.find((todo) => todo.status === "in_progress")
        ?? todos.find((todo) => todo.status === "pending");
    if (working?.content) {
        return working.content;
    }
    if (goalRun.status === "running") {
        return "Worker in progress";
    }
    return formatGoalRunStatus(goalRun.status);
}

export function goalRunChildTaskCount(goalRun: GoalRun): number {
    if (typeof goalRun.child_task_count === "number") {
        return goalRun.child_task_count;
    }
    return goalRun.child_task_ids?.length ?? 0;
}

export function formatGoalRunDuration(durationMs?: number | null): string {
    if (typeof durationMs !== "number" || !Number.isFinite(durationMs) || durationMs <= 0) {
        return "-";
    }

    const totalSeconds = Math.max(1, Math.round(durationMs / 1000));
    if (totalSeconds < 60) {
        return `${totalSeconds}s`;
    }

    const totalMinutes = Math.round(totalSeconds / 60);
    if (totalMinutes < 60) {
        return `${totalMinutes}m`;
    }

    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
}

export function latestGoalRunTodoSnapshot(goalRun: GoalRun): TodoItem[] {
    const events = goalRun.events ?? [];
    for (let index = events.length - 1; index >= 0; index -= 1) {
        const snapshot = events[index]?.todo_snapshot ?? [];
        if (snapshot.length > 0) {
            return snapshot;
        }
    }
    return [];
}
