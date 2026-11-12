import { describe, expect, it, vi } from "vitest";
import { loadWorkspaceRootState, runWorkspacePathMutation } from "./workspaceRefresh";
import { runWorkspaceGitMutation } from "./workspaceGitActions";

const overview = { stagedFiles: 1, unstagedFiles: 2 } as ZoraiWorkspaceGitOverview;

describe("workspace refresh transaction", () => {
  it.each(["create", "rename", "delete"])("coordinates a %s mutation with one full root refresh", async (operation) => {
    const events: string[] = [];
    const result = await runWorkspacePathMutation(async () => {
      events.push(`mutate:${operation}`);
      return operation;
    }, async () => { events.push("refresh:root+git+explorer"); });

    expect(result).toBe(operation);
    expect(events).toEqual([`mutate:${operation}`, "refresh:root+git+explorer"]);
  });

  it.each(["file", "hunk", "discard"])("refreshes Git status, overview counts, and Explorer state after a %s action", async (action) => {
    const refreshBridge = {
      workspaceListDirectory: vi.fn(async () => []),
      workspaceGitStatus: vi.fn(async () => []),
      workspaceGitOverview: vi.fn(async () => overview),
      workspaceGitListWorktrees: vi.fn(async () => []),
      workspaceGitHistory: vi.fn(async () => []),
      workspaceGitConflicts: vi.fn(async () => []),
    };
    let explorerRefreshToken = 0;

    await runWorkspaceGitMutation(
      async () => action,
      async () => {
        await loadWorkspaceRootState(refreshBridge as never, "/workspace");
        explorerRefreshToken += 1;
      },
    );

    expect(refreshBridge.workspaceGitStatus).toHaveBeenCalledOnce();
    expect(refreshBridge.workspaceGitOverview).toHaveBeenCalledOnce();
    expect(refreshBridge.workspaceListDirectory).toHaveBeenCalledOnce();
    expect(explorerRefreshToken).toBe(1);
  });

  it("loads Explorer, Git status, and overview counts as one refresh snapshot", async () => {
    const bridge = {
      workspaceListDirectory: vi.fn(async () => [{ path: "src", name: "src", isDirectory: true }]),
      workspaceGitStatus: vi.fn(async () => [{ path: "a.ts", indexStatus: " ", worktreeStatus: "M" }]),
      workspaceGitOverview: vi.fn(async () => overview),
      workspaceGitListWorktrees: vi.fn(async () => []),
      workspaceGitHistory: vi.fn(async () => []),
      workspaceGitConflicts: vi.fn(async () => []),
    };

    const state = await loadWorkspaceRootState(bridge as never, "/workspace");

    expect(state.entries).toHaveLength(1);
    expect(state.statuses).toHaveLength(1);
    expect(state.overview).toBe(overview);
    expect(bridge.workspaceListDirectory).toHaveBeenCalledWith("/workspace", "");
    expect(bridge.workspaceGitStatus).toHaveBeenCalledWith("/workspace");
    expect(bridge.workspaceGitOverview).toHaveBeenCalledWith("/workspace");
  });

  it("fails the load when the file list cannot be read, but reports Git failures without hiding the files", async () => {
    const failing = {
      workspaceListDirectory: vi.fn(async () => { throw new Error("workspace IPC rejected"); }),
      workspaceGitStatus: vi.fn(async () => []),
      workspaceGitOverview: vi.fn(async () => overview),
      workspaceGitListWorktrees: vi.fn(async () => []),
      workspaceGitHistory: vi.fn(async () => []),
      workspaceGitConflicts: vi.fn(async () => []),
    };

    await expect(loadWorkspaceRootState(failing as never, "/workspace")).rejects.toThrow("workspace IPC rejected");

    const failingOverview = {
      workspaceListDirectory: vi.fn(async () => []),
      workspaceGitStatus: vi.fn(async () => []),
      workspaceGitOverview: vi.fn(async () => { throw new Error("git status failed"); }),
      workspaceGitListWorktrees: vi.fn(async () => []),
      workspaceGitHistory: vi.fn(async () => []),
      workspaceGitConflicts: vi.fn(async () => []),
    };

    const partial = await loadWorkspaceRootState({ ...failingOverview, workspaceListDirectory: vi.fn(async () => [{ path: "a.ts", name: "a.ts", isDirectory: false }]) } as never, "/workspace");
    expect(partial.entries).toEqual([{ path: "a.ts", name: "a.ts", isDirectory: false }]);
    expect(partial.overview).toBeNull();
    expect(partial.gitError).toBe("git status failed");
  });

  it("tolerates optional Git bridge channels being absent without failing the Explorer load", async () => {
    const minimal = { workspaceListDirectory: vi.fn(async () => [{ path: "a.ts", name: "a.ts", isDirectory: false }]) };

    const state = await loadWorkspaceRootState(minimal as never, "/workspace");

    expect(state.entries).toHaveLength(1);
    expect(state.statuses).toEqual([]);
    expect(state.overview).toBeNull();
    expect(state.worktrees).toEqual([]);
    expect(state.history).toEqual([]);
    expect(state.conflicts).toEqual([]);
  });

  it("does not run the root refresh when the path mutation itself is rejected", async () => {
    const refresh = vi.fn(async () => undefined);

    await expect(runWorkspacePathMutation(async () => { throw new Error("create rejected"); }, refresh)).rejects.toThrow("create rejected");
    expect(refresh).not.toHaveBeenCalled();
  });
});

describe("workspace root state cache", () => {
  it("returns the last loaded state for a folder and merges git-only updates into it", async () => {
    const { cachedWorkspaceRootState, rememberWorkspaceRootState } = await import("./workspaceRefresh");
    expect(cachedWorkspaceRootState("/cache-root")).toBeNull();
    rememberWorkspaceRootState("/cache-root", { entries: [], statuses: [], overview: null, worktrees: [], history: [{ hash: "h", shortHash: "h", author: "a", date: "", subject: "s" }], conflicts: [] });
    rememberWorkspaceRootState("/cache-root", { statuses: [{ path: "x", index: "M", worktree: " " } as unknown as ZoraiWorkspaceGitStatus] });
    const cached = cachedWorkspaceRootState("/cache-root");
    expect(cached?.history).toHaveLength(1);
    expect(cached?.statuses).toHaveLength(1);
    expect(cachedWorkspaceRootState(null)).toBeNull();
  });
});
