import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceExplorerTreeView, WorkspaceExplorerTree } from "./WorkspaceExplorerTree";
import { createWorkspaceExplorerController, createWorkspaceExplorerLoader, type WorkspaceExplorerSnapshot } from "./workspaceExplorerController";

const directory = { name: "src", path: "src", isDirectory: true } as ZoraiWorkspaceEntry;
const nestedDirectory = { name: "components", path: "src/components", isDirectory: true } as ZoraiWorkspaceEntry;
const file = { name: "App.tsx", path: "App.tsx", isDirectory: false } as ZoraiWorkspaceEntry;
const nestedFile = { name: "Button.tsx", path: "src/components/Button.tsx", isDirectory: false } as ZoraiWorkspaceEntry;

const emptySnapshot: WorkspaceExplorerSnapshot = {
  expandedPaths: new Set(),
  childrenByPath: new Map(),
  loadingPaths: new Set(),
  errorByPath: new Map(),
};

describe("WorkspaceExplorerTree", () => {
  it("renders accessible tree items, typed files, and SVG folder chevrons", () => {
    const html = renderToStaticMarkup(<WorkspaceExplorerTreeView entries={[directory, file]} status={new Map([["App.tsx", "M"]])} onOpen={() => undefined} snapshot={emptySnapshot} onToggle={async () => undefined} />);

    expect(html).toContain('role="tree"');
    expect(html).toContain('role="treeitem"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("zorai-code-folder-chevron");
    expect(html).toContain("type-react");
    expect(html).toContain("zorai-workspace-git-marker");
  });

  it("mutes gitignored entries but still lists them so they stay openable", () => {
    const ignored = { name: "generated", path: "generated", isDirectory: true, isGitIgnored: true } as ZoraiWorkspaceEntry;
    const html = renderToStaticMarkup(<WorkspaceExplorerTreeView entries={[ignored, directory]} status={new Map()} onOpen={() => undefined} snapshot={emptySnapshot} onToggle={async () => undefined} />);

    expect(html.match(/zorai-workspace-tree-row--ignored/g)).toHaveLength(1);
    expect(html).toContain(">generated<");
  });

  it("retains expanded descendants and reloads each after a refresh token transaction", async () => {
    const loads = vi.fn(async (path: string) => path === "src" ? [nestedDirectory] : [nestedFile]);
    const controller = createWorkspaceExplorerController(loads);

    await controller.toggle("src");
    await controller.toggle("src/components");
    expect([...controller.getSnapshot().expandedPaths]).toEqual(["src", "src/components"]);

    await controller.refreshExpanded();

    expect(loads.mock.calls).toEqual([
      ["src"],
      ["src/components"],
      ["src"],
      ["src/components"],
    ]);
    expect([...controller.getSnapshot().expandedPaths]).toEqual(["src", "src/components"]);
    expect(controller.getSnapshot().childrenByPath.get("src/components")).toEqual([nestedFile]);

    const html = renderToStaticMarkup(<WorkspaceExplorerTreeView entries={[directory]} status={new Map()} onOpen={() => undefined} snapshot={controller.getSnapshot()} onToggle={controller.toggle} />);
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("Button.tsx");
  });

  it("keeps the failed directory collapsed, surfaces its error, and preserves other expansion", async () => {
    const loads = vi.fn(async (path: string) => {
      if (path === "broken") throw new Error("IPC workspaceListDirectory rejected");
      if (path === "src") return [nestedDirectory];
      return [nestedFile];
    });
    const controller = createWorkspaceExplorerController(loads);

    await controller.toggle("src");
    await controller.toggle("src/components");

    await expect(controller.toggle("broken")).resolves.toBeUndefined();
    const failed = controller.getSnapshot();
    expect(failed.expandedPaths.has("broken")).toBe(false);
    expect(failed.errorByPath.get("broken")).toBe("IPC workspaceListDirectory rejected");
    expect(failed.loadingPaths.has("broken")).toBe(false);
    expect(failed.childrenByPath.has("broken")).toBe(false);
    expect(failed.expandedPaths.has("src/components")).toBe(true);
  });

  it("keeps prior children visible when a refresh load is rejected", async () => {
    let fail = false;
    const loads = vi.fn(async () => {
      if (fail) throw new Error("refresh rejected");
      return [nestedFile];
    });
    const controller = createWorkspaceExplorerController(loads);

    await controller.toggle("src/components");
    expect(controller.getSnapshot().childrenByPath.get("src/components")).toEqual([nestedFile]);

    fail = true;
    await controller.refreshExpanded();

    const failed = controller.getSnapshot();
    expect(failed.childrenByPath.get("src/components")).toEqual([nestedFile]);
    expect(failed.errorByPath.get("src/components")).toBe("refresh rejected");
    expect([...failed.expandedPaths]).toEqual(["src/components"]);
  });

  it("notifies subscribers exactly once per published snapshot transition", async () => {
    const listener = vi.fn();
    const controller = createWorkspaceExplorerController(async () => [nestedFile]);
    const unsubscribe = controller.subscribe(listener);

    await controller.toggle("src/components");
    unsubscribe();
    await controller.refreshExpanded();

    expect(listener).toHaveBeenCalledTimes(4);
  });

  it("routes directory loads through the workspace bridge loader", async () => {
    const bridge = { workspaceListDirectory: vi.fn(async () => [nestedFile]) };
    const load = createWorkspaceExplorerLoader(bridge as never, "/workspace");
    const missing = createWorkspaceExplorerLoader({}, "/workspace");
    const absent = createWorkspaceExplorerLoader(null, "/workspace");

    await expect(load("src/components")).resolves.toEqual([nestedFile]);
    expect(bridge.workspaceListDirectory).toHaveBeenCalledWith("/workspace", "src/components");
    await expect(missing("src/components")).resolves.toEqual([]);
    await expect(absent("src/components")).resolves.toEqual([]);
  });

  it("exposes a disabled state on the live tree while a directory is loading", () => {
    const loadingSnapshot: WorkspaceExplorerSnapshot = {
      expandedPaths: new Set<string>(),
      childrenByPath: new Map<string, ZoraiWorkspaceEntry[]>(),
      loadingPaths: new Set(["src"]),
      errorByPath: new Map<string, string>(),
    };
    const html = renderToStaticMarkup(<WorkspaceExplorerTreeView entries={[directory]} status={new Map()} onOpen={() => undefined} snapshot={loadingSnapshot} onToggle={async () => undefined} />);

    expect(html).toContain("zorai-workspace-tree-loading");
    expect(html).toContain("Loading…");
  });

  it("renders the connected tree through the bridge-backed controller factory", () => {
    const html = renderToStaticMarkup(<WorkspaceExplorerTree root="/workspace" entries={[directory, file]} status={new Map()} onOpen={() => undefined} refreshToken={0} />);
    expect(html).toContain('role="tree"');
    expect(html).toContain("App.tsx");
    expect(html).toContain('aria-expanded="false"');
  });
});
