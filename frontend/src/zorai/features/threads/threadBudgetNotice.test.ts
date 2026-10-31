import { describe, expect, it } from "vitest";
import { deriveSpawnedAgentTree } from "@/lib/spawnedAgentTree";
import {
  activeThreadBudgetExceededNotice,
  formatThreadBudgetExceededNotice,
  isSubagentBudgetExceededSystemContent,
  isThreadBudgetExceededSystemContent,
} from "./threadBudgetNotice";

describe("threadBudgetNotice", () => {
  it("surfaces the TUI footer copy when this thread exhausted its subagent budget", () => {
    const threadId = "thread-child";
    const notice = activeThreadBudgetExceededNotice(
      threadId,
      [{
        role: "system",
        content: `Task budget exceeded for this thread.\n\nThread \`${threadId}\` exhausted its execution budget and is now locked for further operator messages.`,
      }],
      null,
    );

    expect(notice).toBe(formatThreadBudgetExceededNotice(threadId));
    expect(notice).toContain("continue from the parent thread");
  });

  it("locks the child from its spawned-tree status even before the system message hydrates", () => {
    const tree = deriveSpawnedAgentTree(
      [{
        id: "task-child",
        status: "budget_exceeded",
        created_at: 1,
        thread_id: "thread-child",
        parent_thread_id: "thread-parent",
      }],
      "thread-child",
    );

    expect(activeThreadBudgetExceededNotice("thread-child", [], tree)).toBe(
      formatThreadBudgetExceededNotice("thread-child"),
    );
  });

  it("does not lock the parent when only a child exhausted its budget", () => {
    const tree = deriveSpawnedAgentTree(
      [{
        id: "task-child",
        status: "budget_exceeded",
        created_at: 1,
        thread_id: "thread-child",
        parent_thread_id: "thread-parent",
      }],
      "thread-parent",
    );

    expect(activeThreadBudgetExceededNotice("thread-parent", [], tree)).toBeNull();
    expect(isSubagentBudgetExceededSystemContent(
      "Spawned thread `thread-child` (subagent task `task-child`) exhausted its execution budget and reported back.",
    )).toBe(true);
    expect(isThreadBudgetExceededSystemContent(
      "Spawned thread `thread-child` exhausted its execution budget and reported back.",
    )).toBe(false);
  });

  it("clears once the daemon extends the budget, and returns if the child runs out again", () => {
    const threadId = "thread-child";
    const exceeded = {
      role: "system" as const,
      content: `Task budget exceeded for this thread.\n\nThread \`${threadId}\` exhausted its execution budget and is now locked for further operator messages.`,
    };
    const extended = {
      role: "system" as const,
      content: "Execution budget extended by 18000 visible output tokens.\n\nReason: finish the frontend\nNew ceiling: 36000 tokens.",
    };

    expect(activeThreadBudgetExceededNotice(threadId, [exceeded, extended], null)).toBeNull();
    expect(activeThreadBudgetExceededNotice(threadId, [exceeded, extended, exceeded], null))
      .toBe(formatThreadBudgetExceededNotice(threadId));
  });
});
