import type { ApprovalRequest } from "@/lib/agent-mission-store/types";
import type { AgentThread } from "@/lib/agentStore";
import { isInternalAgentThread } from "@/lib/agentStore/history";
import type { GoalRun } from "@/lib/goalRuns";
import type { OperatorQuestion } from "@/lib/operatorQuestionStore";
import type { TerminalNotification } from "@/lib/types";

export type InboxSection = "needs" | "replies" | "updates";
export type InboxFilter = "all" | InboxSection;

type InboxItemBase = {
  id: string;
  section: InboxSection;
  at: number;
  unread: boolean;
  title: string;
  summary: string;
  threadId: string | null;
  goalRunId: string | null;
};

export type InboxItem =
  | (InboxItemBase & { kind: "approval"; approval: ApprovalRequest })
  | (InboxItemBase & { kind: "question"; question: OperatorQuestion })
  | (InboxItemBase & { kind: "goal"; goalRun: GoalRun })
  | (InboxItemBase & { kind: "reply"; thread: AgentThread; threadKey: string })
  | (InboxItemBase & { kind: "notification"; notification: TerminalNotification })
  | (InboxItemBase & { kind: "updates"; notifications: TerminalNotification[] });

export type InboxSources = {
  approvals: readonly ApprovalRequest[];
  question: OperatorQuestion | null;
  goalRuns: readonly GoalRun[];
  threads: readonly AgentThread[];
  notifications: readonly TerminalNotification[];
  now: number;
  baselineAt: number;
  threadReadAt: Readonly<Record<string, number>>;
  threadArchivedAt: Readonly<Record<string, number>>;
  isApprovalHiddenByPolicy?: (approval: ApprovalRequest) => boolean;
};

const ATTENTION_KINDS = new Set(["task_failed", "task_budget_exceeded", "plugin_needs_reconnect"]);
const ATTENTION_SEVERITIES = new Set(["error", "alert"]);
const MESSAGE_KINDS = new Set(["tool_notify_user"]);
const EXCLUDED_KINDS = new Set(["task_completed"]);

export function threadKey(thread: Pick<AgentThread, "id" | "daemonThreadId">): string {
  return thread.daemonThreadId || thread.id;
}

export function notificationMetadata(notification: TerminalNotification): { goalRunId: string | null; threadId: string | null } {
  if (!notification.metadataJson) return { goalRunId: null, threadId: null };
  try {
    const metadata = JSON.parse(notification.metadataJson) as Record<string, unknown>;
    return {
      goalRunId: typeof metadata.goal_run_id === "string" && metadata.goal_run_id ? metadata.goal_run_id : null,
      threadId: typeof metadata.thread_id === "string" && metadata.thread_id ? metadata.thread_id : null,
    };
  } catch {
    return { goalRunId: null, threadId: null };
  }
}

export function notificationSection(notification: TerminalNotification): InboxSection {
  if (ATTENTION_KINDS.has(notification.kind ?? "") || ATTENTION_SEVERITIES.has(notification.severity ?? "")) return "needs";
  if (MESSAGE_KINDS.has(notification.kind ?? "")) return "replies";
  return "updates";
}

function threadCanReply(thread: AgentThread): boolean {
  return !isInternalAgentThread(thread) && !threadKey(thread).startsWith("goal:");
}

export function notificationTime(notification: TerminalNotification): number {
  return notification.createdAt ?? notification.timestamp;
}

export function plainSummary(text: string): string {
  return text
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/\*\*|__|~~|`/g, "")
    .replace(/(^|\s)[#>]+\s/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

export function buildInboxItems(sources: InboxSources): InboxItem[] {
  const items: InboxItem[] = [];

  for (const approval of sources.approvals) {
    if (approval.status !== "pending" || approval.handledAt !== null) continue;
    if (sources.isApprovalHiddenByPolicy?.(approval)) continue;
    items.push({
      kind: "approval",
      id: `approval:${approval.id}`,
      section: "needs",
      at: approval.createdAt,
      unread: true,
      title: approval.command || "Approval request",
      summary: approval.reasons.join(" · ") || approval.blastRadius || "Waiting for your decision",
      threadId: approval.threadId ?? null,
      goalRunId: null,
      approval,
    });
  }

  if (sources.question) {
    items.push({
      kind: "question",
      id: `question:${sources.question.question_id}`,
      section: "needs",
      at: sources.now,
      unread: true,
      title: "Question from agent",
      summary: sources.question.content,
      threadId: sources.question.threadId,
      goalRunId: null,
      question: sources.question,
    });
  }

  for (const goalRun of sources.goalRuns) {
    if (goalRun.status !== "awaiting_approval") continue;
    items.push({
      kind: "goal",
      id: `goal:${goalRun.id}`,
      section: "needs",
      at: goalRun.started_at ?? goalRun.created_at,
      unread: true,
      title: goalRun.title || goalRun.goal,
      summary: goalRun.current_step_title || "Goal is waiting for approval",
      threadId: goalRun.thread_id ?? null,
      goalRunId: goalRun.id,
      goalRun,
    });
  }

  for (const thread of sources.threads) {
    if (!threadCanReply(thread)) continue;
    const key = threadKey(thread);
    if (thread.updatedAt <= sources.baselineAt) continue;
    if ((sources.threadArchivedAt[key] ?? 0) >= thread.updatedAt) continue;
    items.push({
      kind: "reply",
      id: `reply:${key}`,
      section: "replies",
      at: thread.updatedAt,
      unread: (sources.threadReadAt[key] ?? sources.baselineAt) < thread.updatedAt,
      title: thread.title || "Untitled thread",
      summary: thread.lastMessagePreview,
      threadId: key,
      goalRunId: null,
      thread,
      threadKey: key,
    });
  }

  const updateGroups = new Map<string, TerminalNotification[]>();
  for (const notification of sources.notifications) {
    if (notification.archivedAt != null || notification.deletedAt != null) continue;
    if (EXCLUDED_KINDS.has(notification.kind ?? "")) continue;
    const section = notificationSection(notification);
    const { goalRunId, threadId } = notificationMetadata(notification);
    if (section !== "updates") {
      items.push({
        kind: "notification",
        id: `notification:${notification.id}`,
        section,
        at: notificationTime(notification),
        unread: !notification.isRead,
        title: notification.title,
        summary: notification.body || notification.subtitle || "",
        threadId,
        goalRunId,
        notification,
      });
      continue;
    }
    const groupKey = goalRunId ? `goal:${goalRunId}` : threadId ? `thread:${threadId}` : `kind:${notification.kind ?? notification.source}`;
    updateGroups.set(groupKey, [...(updateGroups.get(groupKey) ?? []), notification]);
  }

  for (const [groupKey, notifications] of updateGroups) {
    const sorted = [...notifications].sort((a, b) => notificationTime(b) - notificationTime(a));
    const latest = sorted[0];
    const { goalRunId, threadId } = notificationMetadata(latest);
    const goalTitle = goalRunId ? sources.goalRuns.find((goalRun) => goalRun.id === goalRunId)?.title : null;
    items.push({
      kind: "updates",
      id: `updates:${groupKey}`,
      section: "updates",
      at: notificationTime(latest),
      unread: sorted.some((notification) => !notification.isRead),
      title: goalTitle || latest.title,
      summary: sorted.length > 1 ? `${sorted.length} updates · latest: ${latest.title}` : latest.body || latest.subtitle || "",
      threadId,
      goalRunId,
      notifications: sorted,
    });
  }

  const sectionOrder: Record<InboxSection, number> = { needs: 0, replies: 1, updates: 2 };
  for (const item of items) item.summary = plainSummary(item.summary);
  return items.sort((a, b) => sectionOrder[a.section] - sectionOrder[b.section] || b.at - a.at);
}

export function filterInboxItems(items: readonly InboxItem[], filter: InboxFilter, unreadOnly: boolean, query: string): InboxItem[] {
  const needle = query.trim().toLowerCase();
  return items.filter((item) => {
    if (filter !== "all" && item.section !== filter) return false;
    if (unreadOnly && !item.unread) return false;
    if (!needle) return true;
    return `${item.title} ${item.summary}`.toLowerCase().includes(needle);
  });
}

export function inboxAttentionCount(items: readonly InboxItem[]): number {
  return items.filter((item) => item.unread && item.section !== "updates").length;
}

export function inboxSectionCounts(items: readonly InboxItem[]): Record<InboxFilter, number> {
  const counts: Record<InboxFilter, number> = { all: 0, needs: 0, replies: 0, updates: 0 };
  for (const item of items) {
    if (!item.unread) continue;
    counts[item.section] += 1;
    counts.all += 1;
  }
  return counts;
}

export function formatWhen(timestamp: number, now = Date.now()): string {
  if (!Number.isFinite(timestamp)) return "";
  const delta = now - timestamp;
  if (delta < 60_000) return "now";
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h`;
  return new Date(timestamp).toLocaleDateString();
}
