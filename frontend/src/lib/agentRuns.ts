import { getBridge } from "./bridge";
import { formatTaskStatus, formatTaskTimestamp, isTaskActive, isTaskTerminal, taskStatusColor, type AgentTaskPriority, type AgentTaskStatus } from "./agentTaskQueue";

export type AgentRunKind = "task" | "subagent";
export type AgentRunClassification = "coding" | "research" | "ops" | "browser" | "messaging" | "mixed" | string;

export interface AgentRun {
    id: string;
    task_id: string;
    kind: AgentRunKind;
    classification: AgentRunClassification;
    title: string;
    description: string;
    status: AgentTaskStatus;
    priority: AgentTaskPriority;
    progress: number;
    created_at: number;
    started_at?: number | null;
    completed_at?: number | null;
    thread_id?: string | null;
    session_id?: string | null;
    provider?: string | null;
    model?: string | null;
    reasoning_effort?: string | null;
    workspace_id?: string | null;
    source: string;
    runtime?: string | null;
    goal_run_id?: string | null;
    goal_run_title?: string | null;
    goal_step_id?: string | null;
    goal_step_title?: string | null;
    parent_run_id?: string | null;
    parent_task_id?: string | null;
    parent_thread_id?: string | null;
    parent_title?: string | null;
    blocked_reason?: string | null;
    error?: string | null;
    result?: string | null;
    last_error?: string | null;
}

export const ACTIVE_RUN_POLL_MS = 4_000;
export const IDLE_RUN_POLL_MS = 30_000;
const MAX_RENDERED_RUNS = 200;

const FAST_POLL_STATUSES = new Set<AgentTaskStatus>([
    "in_progress",
    "queued",
    "awaiting_approval",
    "failed_analyzing",
]);

export function runListIsActive(runs: readonly Pick<AgentRun, "status">[]): boolean {
    return runs.some((run) => FAST_POLL_STATUSES.has(run.status));
}

export function capRenderedRuns(runs: AgentRun[]): AgentRun[] {
    if (runs.length <= MAX_RENDERED_RUNS) return runs;
    return [...runs].sort((left, right) => right.created_at - left.created_at).slice(0, MAX_RENDERED_RUNS);
}

export async function fetchAgentRuns(parentThreadId?: string | null): Promise<AgentRun[]> {
    const zorai = getBridge();
    if (!zorai?.agentListRuns) {
        return [];
    }

    try {
        const result = await zorai.agentListRuns(parentThreadId);
        return Array.isArray(result) ? capRenderedRuns(result as AgentRun[]) : [];
    } catch {
        return [];
    }
}

export function isRunTerminal(run: AgentRun): boolean {
    return isTaskTerminal(run);
}

export function isRunActive(run: AgentRun): boolean {
    return isTaskActive(run);
}

export function isSubagentRun(run: AgentRun): boolean {
    return run.kind === "subagent" || Boolean(run.parent_run_id || run.parent_task_id || run.parent_thread_id);
}

export function formatRunStatus(run: AgentRun): string {
    return formatTaskStatus(run);
}

export function runStatusColor(status: AgentTaskStatus): string {
    return taskStatusColor(status);
}

export function formatRunTimestamp(timestamp?: number | null): string {
    return formatTaskTimestamp(timestamp);
}
