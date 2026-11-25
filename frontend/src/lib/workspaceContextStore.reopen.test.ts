import { describe, expect, it } from "vitest";
import { withoutReopenedEditors, type ThreadWorkspaceContext } from "./workspaceContextStore";

const context = (overrides: Partial<ThreadWorkspaceContext>): ThreadWorkspaceContext => ({
  root: "/repo",
  activeFile: null,
  selection: null,
  attachedFiles: [],
  openFiles: [],
  updatedAt: 5,
  isolateAgentTasks: false,
  isolatedWorktreeStates: {},
  pinnedFiles: [],
  ...overrides,
});

describe("Reopen previous editors: off", () => {
  it("starts with no editors except pinned ones, focusing a pinned file if the last active one was closed", () => {
    const restored = withoutReopenedEditors(context({ openFiles: ["a.ts", "b.ts", "c.ts"], pinnedFiles: ["c.ts"], activeFile: "a.ts" }));
    expect(restored.openFiles).toEqual(["c.ts"]);
    expect(restored.activeFile).toBe("c.ts");
  });

  it("keeps the timestamp so the daemon's equally old copy cannot bring the closed tabs back", () => {
    expect(withoutReopenedEditors(context({ openFiles: ["a.ts"], activeFile: "a.ts" })).updatedAt).toBe(5);
  });
});
