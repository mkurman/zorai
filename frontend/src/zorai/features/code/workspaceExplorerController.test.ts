import { describe, expect, it } from "vitest";
import { workspaceExplorerControllerFor } from "./workspaceExplorerController";

const entry = (path: string, isDirectory = false) => ({ name: path.split("/").pop() ?? path, path, isDirectory, isSymbolicLink: false, sizeBytes: 1, modifiedAt: 1 }) as ZoraiWorkspaceEntry;

describe("cached explorer controller", () => {
  it("keeps the expanded tree when the Code tab is left and reopened", async () => {
    const bridge = { workspaceListDirectory: async (_root: string, dir: string) => dir === "src" ? [entry("src/a.ts")] : [entry("src", true)] };
    const first = workspaceExplorerControllerFor(bridge, "/repo-cache");
    await first.toggle("src");
    const reopened = workspaceExplorerControllerFor(bridge, "/repo-cache");
    expect(reopened).toBe(first);
    expect(reopened.getSnapshot().expandedPaths.has("src")).toBe(true);
    expect(reopened.getSnapshot().childrenByPath.get("src")?.map((item) => item.path)).toEqual(["src/a.ts"]);
  });

  it("refreshes expanded folders in place without flashing a loading row", async () => {
    let listing = [entry("lib/a.ts")];
    const bridge = { workspaceListDirectory: async (_root: string, dir: string) => dir === "lib" ? listing : [entry("lib", true)] };
    const controller = workspaceExplorerControllerFor(bridge, "/repo-refresh");
    await controller.toggle("lib");
    const sawLoading: boolean[] = [];
    const unsubscribe = controller.subscribe(() => sawLoading.push(controller.getSnapshot().loadingPaths.has("lib")));
    listing = [entry("lib/a.ts"), entry("lib/b.ts")];
    await controller.refreshExpanded();
    unsubscribe();
    expect(sawLoading).not.toContain(true);
    expect(controller.getSnapshot().childrenByPath.get("lib")?.map((item) => item.path)).toEqual(["lib/a.ts", "lib/b.ts"]);
  });
});
