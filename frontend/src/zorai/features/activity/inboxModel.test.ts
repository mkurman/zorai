import { describe, expect, it } from "vitest";
import type { ApprovalRequest } from "@/lib/agent-mission-store/types";
import type { AgentThread } from "@/lib/agentStore";
import type { TerminalNotification } from "@/lib/types";
import { buildInboxItems, filterInboxItems, inboxAttentionCount, plainSummary, type InboxSources } from "./inboxModel";

const BASELINE = 1_000;

function thread(id: string, updatedAt: number, title = id): AgentThread {
  return {
    id: `local-${id}`,
    daemonThreadId: id,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "zorai",
    title,
    createdAt: 1,
    updatedAt,
    messageCount: 2,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: `${title} preview`,
  };
}

function notification(id: string, kind: string, metadata: Record<string, string> | null, at: number, overrides: Partial<TerminalNotification> = {}): TerminalNotification {
  return {
    id,
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    title: `${kind} ${id}`,
    subtitle: null,
    body: "",
    icon: null,
    progress: null,
    isRead: false,
    timestamp: at,
    source: "system",
    kind,
    severity: kind === "task_failed" ? "error" : "info",
    metadataJson: metadata ? JSON.stringify(metadata) : null,
    ...overrides,
  };
}

function approval(id: string, overrides: Partial<ApprovalRequest> = {}): ApprovalRequest {
  return {
    id,
    createdAt: 2_000,
    paneId: "pane",
    workspaceId: null,
    surfaceId: null,
    sessionId: null,
    threadId: "thread-a",
    command: "rm -rf build",
    reasons: ["destructive"],
    riskLevel: "high",
    blastRadius: "workspace",
    status: "pending",
    handledAt: null,
    ...overrides,
  } as ApprovalRequest;
}

function sources(overrides: Partial<InboxSources> = {}): InboxSources {
  return {
    approvals: [],
    question: null,
    goalRuns: [],
    threads: [],
    notifications: [],
    now: 9_999,
    baselineAt: BASELINE,
    threadReadAt: {},
    threadArchivedAt: {},
    ...overrides,
  };
}

describe("inbox model", () => {
  it("puts what blocks an agent first, then agent replies, then background updates", () => {
    const items = buildInboxItems(sources({
      approvals: [approval("a1")],
      question: { question_id: "q1", content: "Pick one", options: ["A"], threadId: "thread-b" },
      threads: [thread("thread-c", 5_000)],
      notifications: [
        notification("n1", "stuck_hint", { goal_run_id: "g1" }, 6_000),
        notification("n2", "task_failed", { thread_id: "thread-d" }, 3_000),
      ],
    }));

    expect(items.map((item) => item.section)).toEqual(["needs", "needs", "needs", "replies", "updates"]);
    expect(items.filter((item) => item.section === "needs").map((item) => item.kind)).toEqual(["question", "notification", "approval"]);
  });

  it("leaves routine task-completed notices out of the inbox entirely", () => {
    const items = buildInboxItems(sources({
      notifications: [notification("done", "task_completed", { goal_run_id: "g1" }, 2_000), notification("hint", "stuck_hint", { goal_run_id: "g1" }, 2_100)],
    }));

    expect(items).toHaveLength(1);
    expect(items[0].kind === "updates" && items[0].notifications.map((entry) => entry.id)).toEqual(["hint"]);
  });

  it("files hundreds of background notices for one goal into a single update entry", () => {
    const notices = Array.from({ length: 300 }, (_, index) => notification(`n${index}`, "stuck_hint", { goal_run_id: "g1" }, 2_000 + index));
    const items = buildInboxItems(sources({ notifications: notices }));

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "updates", section: "updates", goalRunId: "g1" });
    expect(items[0].kind === "updates" && items[0].notifications).toHaveLength(300);
  });

  it("does not count background updates toward the badge, only things that need the operator", () => {
    const items = buildInboxItems(sources({
      approvals: [approval("a1")],
      threads: [thread("thread-c", 5_000)],
      notifications: [notification("n1", "stuck_hint", { goal_run_id: "g1" }, 6_000)],
    }));

    expect(inboxAttentionCount(items)).toBe(2);
  });

  it("treats a thread as unread only when it changed after the operator last read it", () => {
    const items = buildInboxItems(sources({
      threads: [thread("read", 5_000), thread("unread", 5_000), thread("old", 500)],
      threadReadAt: { read: 5_000, unread: 4_000 },
    }));

    expect(items.map((item) => [item.threadId, item.unread])).toEqual([["read", false], ["unread", true]]);
  });

  it("an archived thread comes back only when the agent writes again", () => {
    const archived = buildInboxItems(sources({ threads: [thread("t", 5_000)], threadArchivedAt: { t: 5_000 } }));
    const newActivity = buildInboxItems(sources({ threads: [thread("t", 6_000)], threadArchivedAt: { t: 5_000 } }));

    expect(archived).toHaveLength(0);
    expect(newActivity.map((item) => item.unread)).toEqual([true]);
  });

  it("hides approvals that were already handled or that policy auto-approves", () => {
    const items = buildInboxItems(sources({
      approvals: [approval("handled", { handledAt: 3_000 }), approval("auto"), approval("shown")],
      isApprovalHiddenByPolicy: (entry) => entry.id === "auto",
    }));

    expect(items.map((item) => item.id)).toEqual(["approval:shown"]);
  });

  it("never offers replies for internal DM or goal-owned threads", () => {
    const items = buildInboxItems(sources({
      threads: [thread("dm:svarog:weles", 5_000), thread("goal:g1", 5_000), thread("user-thread", 5_000)],
    }));

    expect(items.map((item) => item.threadId)).toEqual(["user-thread"]);
  });

  it("filters like a mail client: by section, unread only, and text", () => {
    const items = buildInboxItems(sources({
      approvals: [approval("a1", { command: "git push origin main" })],
      threads: [thread("t1", 5_000, "Dataset review"), thread("t2", 5_000, "Release notes")],
      threadReadAt: { t2: 5_000 },
    }));

    expect(filterInboxItems(items, "replies", false, "").map((item) => item.threadId)).toEqual(["t1", "t2"]);
    expect(filterInboxItems(items, "replies", true, "").map((item) => item.threadId)).toEqual(["t1"]);
    expect(filterInboxItems(items, "all", false, "git push").map((item) => item.kind)).toEqual(["approval"]);
  });

  it("shows list summaries as plain text without mangling identifiers", () => {
    expect(plainSummary("Which **dataset** for `task_fa1d`?\n## Next\n[docs](https://x)")).toBe("Which dataset for task_fa1d? Next docs");
  });
});
