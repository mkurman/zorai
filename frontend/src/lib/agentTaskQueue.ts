import { getBridge } from "./bridge";

export type AgentTaskStatus =
    | "queued"
    | "in_progress"
    | "awaiting_approval"
    | "blocked"
    | "failed_analyzing"
    | "budget_exceeded"
    | "completed"
    | "failed"
    | "cancelled";

export type AgentTaskPriority = "low" | "normal" | "high" | "urgent";

export type AgentTaskLogLevel = "info" | "warn" | "error";

export interface AgentTaskLogEntry {
    id: string;
    timestamp: number;
    level: AgentTaskLogLevel;
    phase: string;
    message: string;
    details?: string | null;
    attempt: number;
}

export interface AgentQueueTask {
    id: string;
    title: string;
    description: string;
    status: AgentTaskStatus;
    priority: AgentTaskPriority;
    progress: number;
    created_at: number;
    started_at?: number | null;
    completed_at?: number | null;
    error?: string | null;
    result?: string | null;
    thread_id?: string | null;
    source: string;
    notify_on_complete?: boolean;
    notify_channels?: string[];
    dependencies?: string[];
    command?: string | null;
    session_id?: string | null;
    goal_run_id?: string | null;
    goal_run_title?: string | null;
    goal_step_id?: string | null;
    goal_step_title?: string | null;
    parent_task_id?: string | null;
    parent_thread_id?: string | null;
    runtime?: string | null;
    retry_count?: number;
    max_retries?: number;
    next_retry_at?: number | null;
    scheduled_at?: number | null;
    blocked_reason?: string | null;
    awaiting_approval_id?: string | null;
    lane_id?: string | null;
    last_error?: string | null;
    logs?: AgentTaskLogEntry[];
}

export type AgentTaskFilter = {
    relatedThreadIds?: string[];
    goalRunId?: string | null;
    activeOnly?: boolean;
    limit?: number;
};

export async function fetchAgentTasksFiltered(filter: AgentTaskFilter): Promise<AgentQueueTask[]> {
    const zorai = getBridge();
    if (!zorai?.agentListTasksFiltered) {
        return [];
    }

    try {
        const result = await zorai.agentListTasksFiltered(filter);
        return Array.isArray(result) ? (result as AgentQueueTask[]) : [];
    } catch {
        return [];
    }
}

export function isTaskTerminal(task: { status: AgentTaskStatus }): boolean {
    return task.status === "completed"
        || task.status === "failed"
        || task.status === "cancelled"
        || task.status === "budget_exceeded";
}

export function isTaskActive(task: { status: AgentTaskStatus }): boolean {
    return !isTaskTerminal(task);
}

export function isSubagentTask(task: AgentQueueTask): boolean {
    return Boolean(task.parent_task_id || task.parent_thread_id || task.source === "subagent");
}

export function formatTaskStatus(task: { status: AgentTaskStatus }): string {
    switch (task.status) {
        case "in_progress":
            return "In progress";
        case "awaiting_approval":
            return "Awaiting approval";
        case "failed_analyzing":
            return "Analyzing failure";
        case "budget_exceeded":
            return "Budget exceeded";
        default:
            return task.status.replace(/_/g, " ");
    }
}

export function taskStatusColor(status: AgentTaskStatus): string {
    switch (status) {
        case "in_progress":
            return "var(--accent)";
        case "awaiting_approval":
            return "var(--approval)";
        case "blocked":
            return "var(--text-muted)";
        case "failed_analyzing":
            return "var(--warning)";
        case "completed":
            return "var(--success)";
        case "failed":
        case "budget_exceeded":
            return "var(--danger)";
        case "cancelled":
            return "var(--text-muted)";
        default:
            return "var(--text-secondary)";
    }
}

export function formatTaskTimestamp(timestamp?: number | null): string {
    if (!timestamp) {
        return "-";
    }
    return new Date(timestamp).toLocaleTimeString();
}
