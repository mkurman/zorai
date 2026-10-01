import { describe, expect, it } from "vitest";
import type { AgentRun } from "@/lib/agentRuns";
import {
  delegatedSessionView,
  formatWorkerPhase,
  isWorkerThread,
  sameAgentRunSnapshot,
  sessionActivityLabel,
  threadIsUnread,
  threadIsWorking,
  workerCountForThread,
  workerThreadIds,
} from "./sessionCooperation";

function run(id: string, extras: Partial<AgentRun> = {}): AgentRun {
  return {
    id,
    task_id: id,
    kind: "subagent",
    classification: "explore",
    title: id,
    description: "",
    status: "completed",
    priority: "normal",
    progress: 0,
    created_at: 1,
    source: "spawn_subagent",
    thread_id: id,
    ...extras,
  };
}

describe("lead sessions stay separate from delegated workers", () => {
  const child = run("child-run", {
    thread_id: "thread-child",
    parent_thread_id: "thread-lead",
    description: "Old study",
    created_at: 10,
  });
  const reused = run("child-run-2", {
    thread_id: "thread-child",
    parent_thread_id: "thread-lead",
    classification: "execute",
    description: "Implement graders",
    created_at: 30,
  });
  const sibling = run("other-run", {
    thread_id: "thread-other",
    parent_thread_id: "thread-lead",
    description: "Other worker",
    created_at: 20,
  });
  const lead = { id: "local-lead", daemonThreadId: "thread-lead", title: "Literature review", updatedAt: 40 };
  const worker = { id: "local-child", daemonThreadId: "thread-child", title: "Old study (@explore subagent)", updatedAt: 30 };

  it("hides worker threads from the lead list without rewriting their historical title", () => {
    const ids = workerThreadIds([child, reused, sibling]);
    expect(isWorkerThread(worker, ids)).toBe(true);
    expect(isWorkerThread(lead, ids)).toBe(false);
    const goalWorker = run("goal-worker", {
      source: "goal_run",
      thread_id: "goal:run-1",
      parent_thread_id: "thread-lead",
    });
    expect(isWorkerThread({ id: "goal:run-1", daemonThreadId: "goal:run-1" }, workerThreadIds([goalWorker]))).toBe(false);
    expect(worker.title).toBe("Old study (@explore subagent)");
    expect(workerCountForThread(lead, [child, reused, sibling])).toBe(2);
  });

  it("follows the latest brief when a worker thread is reused", () => {
    const view = delegatedSessionView(worker, [child, sibling, reused]);
    expect(view?.assignment).toEqual({ phase: "execute", description: "Implement graders" });
    expect(view?.parentThreadId).toBe("thread-lead");
    expect(view?.siblings.map((item) => item.thread_id)).toEqual(["thread-child", "thread-other"]);
    expect(view?.index).toBe(0);
    expect(delegatedSessionView(lead, [child, sibling, reused])).toBeNull();
    const goalWorker = run("goal-worker", {
      source: "goal_run",
      thread_id: "thread-lead",
      parent_thread_id: "thread-owner",
      description: "You are the sole worker for this goal.",
    });
    expect(delegatedSessionView(lead, [goalWorker])).toBeNull();
    expect(formatWorkerPhase("execute")).toBe("Execute agent");
  });

  it("marks a lead working while a child is still running, and unread only after the operator has seen it once", () => {
    const live = run("live", {
      thread_id: "thread-child",
      parent_thread_id: "thread-lead",
      status: "in_progress",
      created_at: 5,
    });
    expect(threadIsWorking(lead, [live], null)).toBe(true);
    expect(threadIsWorking(worker, [child], null)).toBe(false);
    expect(threadIsWorking({ id: "local-lead" }, [], "local-lead")).toBe(true);
    const seenAt = Date.parse("2026-09-30T12:00:00Z");
    expect(threadIsUnread({ ...lead, updatedAt: seenAt + 1000 }, seenAt, false)).toBe(true);
    expect(threadIsUnread({ ...lead, updatedAt: seenAt + 1000 }, seenAt, true)).toBe(false);
    expect(threadIsUnread({ ...lead, updatedAt: seenAt + 1000 }, null, false)).toBe(false);
  });

  it("keeps an unchanged run snapshot so a poll does not rebuild the thread", () => {
    expect(sameAgentRunSnapshot([child], [child])).toBe(true);
    expect(sameAgentRunSnapshot([child], [{ ...child, status: "in_progress" }])).toBe(false);
    expect(sameAgentRunSnapshot([child], [child, sibling])).toBe(false);
  });

  it("labels session recency so the rail does not read like an audit log", () => {
    const now = Date.parse("2026-09-30T12:00:00Z");
    expect(sessionActivityLabel(now - 20_000, now)).toBe("Just now");
    expect(sessionActivityLabel(now - 5 * 60_000, now)).toBe("5m ago");
    expect(sessionActivityLabel(now - 3 * 60 * 60_000, now)).toBe("3h ago");
    expect(sessionActivityLabel(now - 2 * 24 * 60 * 60_000, now)).toBe("2d ago");
    expect(sessionActivityLabel(Math.floor((now - 20_000) / 1000), now)).toBe("Just now");
  });
});
