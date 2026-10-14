import { describe, expect, it } from "vitest";
import type { AgentTodoItem } from "@/lib/agentStore";
import type { AgentRun } from "@/lib/agentRuns";
import type { SpawnedAgentTree } from "@/lib/spawnedAgentTree";
import type { WorkContextEntry } from "@/lib/agentWorkContext";
import {
  compactSessionHasContent,
  countDiffStats,
  flattenSpawnedRuns,
  gitStatusMatchesPath,
  isRelativeWorkspacePath,
  isUntrackedGitStatus,
  pendingWorkContextEntries,
  rejectUsesOperationSnapshot,
  sameFilesystemPath,
  todoProgress,
  untrackedContentStats,
  workContextFileName,
  workContextRelativePath,
} from "./compactSessionModel";

function run(id: string, extras: Partial<AgentRun> = {}): AgentRun {
  return {
    id,
    task_id: id,
    kind: "subagent",
    classification: "coding",
    title: id,
    description: "",
    status: "running",
    priority: "normal",
    progress: 0,
    created_at: 1,
    source: "daemon",
    ...extras,
  };
}

describe("compact code-agent session bar", () => {
  it("keeps git hunk lookups relative to the repo so +/- counts the changed file, not an absolute path", () => {
    expect(workContextRelativePath("/repo/src/lib.rs", "/repo")).toBe("src/lib.rs");
    expect(workContextFileName("/repo/src/lib.rs")).toBe("lib.rs");
  });

  it("counts added and removed lines from a unified diff without treating headers as edits", () => {
    const stats = countDiffStats("diff --git a/a.ts b/a.ts\n--- a/a.ts\n+++ b/a.ts\n@@ -1,2 +1,3 @@\n-old\n+new\n+extra\n");
    expect(stats.additions).toBe(2);
    expect(stats.deletions).toBe(1);
  });

  it("hides the slim bar when the thread has no files, todos, or spawned agents", () => {
    expect(compactSessionHasContent([], [], [])).toBe(false);
    expect(compactSessionHasContent([{ path: "a.ts", source: "agent", isText: true, updatedAt: 1 }], [], [])).toBe(true);
    expect(todoProgress([
      { id: "1", content: "one", status: "completed", position: 0 },
      { id: "2", content: "two", status: "pending", position: 1 },
    ] as AgentTodoItem[])).toEqual({ done: 1, total: 2 });
  });

  it("lists spawned children and not the current thread's own run", () => {
    const child = run("child");
    const tree: SpawnedAgentTree<AgentRun> = {
      activeThreadId: "parent-thread",
      anchor: {
        item: run("parent", { thread_id: "parent-thread" }),
        children: [{ item: child, children: [], openable: true, live: true }],
        openable: false,
        live: true,
      },
      roots: [],
    };
    expect(flattenSpawnedRuns(tree).map((item) => item.id)).toEqual(["child"]);
  });

  it("reverts from an operation snapshot when the work context recorded one", () => {
    expect(rejectUsesOperationSnapshot({ operationId: "op-1" })).toBe(true);
    expect(rejectUsesOperationSnapshot({ operationId: null })).toBe(false);
  });

  it("counts an untracked file as added lines and keeps workspace-relative paths openable in the editor", () => {
    expect(untrackedContentStats("one\ntwo\nthree\n")).toEqual({ additions: 3, deletions: 0 });
    expect(untrackedContentStats("")).toEqual({ additions: 0, deletions: 0 });
    expect(isUntrackedGitStatus({ indexStatus: "?", worktreeStatus: "?", previousPath: null })).toBe(true);
    expect(isUntrackedGitStatus({ indexStatus: "M", worktreeStatus: " " })).toBe(false);
    expect(gitStatusMatchesPath("frontend/src/a.ts", "src/a.ts")).toBe(true);
    expect(isRelativeWorkspacePath("frontend/src/a.ts")).toBe(true);
    expect(isRelativeWorkspacePath("/repo/frontend/src/a.ts")).toBe(false);
    expect(sameFilesystemPath("/repo/", "/repo")).toBe(true);
  });
});

describe("pendingWorkContextEntries", () => {
  const entry = (path: string): WorkContextEntry => ({ path, kind: "repo_change", source: "apply_patch", updatedAt: 1 } as WorkContextEntry);

  it("only offers Keep/Reject for files that still have unstaged changes in the repo", () => {
    const entries = [entry("/repo/src/a.ts"), entry("/repo/src/b.ts"), entry("/home/me/.zorai/threads/t/artifacts/spec.md")];
    const statuses = { "/repo": [
      { path: "src/a.ts", worktreeStatus: "M" },
      { path: "src/b.ts", worktreeStatus: " " },
    ] };
    expect(pendingWorkContextEntries(entries, statuses, "/repo").map((item) => item.path)).toEqual(["/repo/src/a.ts"]);
  });

  it("has nothing to review once every change is staged", () => {
    expect(pendingWorkContextEntries([entry("/repo/a.ts")], { "/repo": [{ path: "a.ts", worktreeStatus: "" }] }, "/repo")).toEqual([]);
  });
});
