import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspaceContextStore } from "./workspaceContextStore";

describe("moveOpenFile", () => {
  beforeEach(() => {
    useWorkspaceContextStore.setState({ byThreadId: {} });
    const store = useWorkspaceContextStore.getState();
    store.bindRoot("t", "/repo");
    for (const file of ["a.ts", "b.ts", "c.ts"]) useWorkspaceContextStore.getState().setActiveFile("t", file);
  });

  const openFiles = () => useWorkspaceContextStore.getState().byThreadId.t.openFiles;

  it("drops a dragged editor tab on either side of the target, like session tabs", () => {
    useWorkspaceContextStore.getState().moveOpenFile("t", "c.ts", "a.ts", false);
    expect(openFiles()).toEqual(["c.ts", "a.ts", "b.ts"]);
    useWorkspaceContextStore.getState().moveOpenFile("t", "c.ts", "b.ts", true);
    expect(openFiles()).toEqual(["a.ts", "b.ts", "c.ts"]);
  });

  it("ignores drops onto itself or onto tabs that are not open", () => {
    const before = openFiles();
    useWorkspaceContextStore.getState().moveOpenFile("t", "a.ts", "a.ts", true);
    useWorkspaceContextStore.getState().moveOpenFile("t", "a.ts", "zzz.ts", true);
    expect(openFiles()).toBe(before);
  });
});
