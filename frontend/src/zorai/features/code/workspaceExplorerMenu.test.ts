import { describe, expect, it, vi } from "vitest";
import { EXPLORER_DELETE_KEY, EXPLORER_RENAME_KEY, workspaceExplorerMenuItems, type WorkspaceExplorerMenuActions } from "./workspaceExplorerMenu";

function actions(): WorkspaceExplorerMenuActions & { [K in keyof WorkspaceExplorerMenuActions]: ReturnType<typeof vi.fn> } {
  return {
    isAttached: vi.fn(() => false), open: vi.fn(), toggleAttached: vi.fn(), create: vi.fn(), copy: vi.fn(),
    reveal: vi.fn(), openExternal: vi.fn(), openTerminal: vi.fn(), rename: vi.fn(), remove: vi.fn(), refresh: vi.fn(),
  };
}

const select = (items: ReturnType<typeof workspaceExplorerMenuItems>, key: string) => items.find((item) => item.key === key)?.onSelect?.();
const file = { name: "App.tsx", path: "src/App.tsx", isDirectory: false } as ZoraiWorkspaceEntry;
const folder = { name: "src", path: "src", isDirectory: true } as ZoraiWorkspaceEntry;

describe("workspaceExplorerMenuItems", () => {
  it("acts on the right-clicked file rather than whichever file is open in the editor", () => {
    const spy = actions();
    const items = workspaceExplorerMenuItems("/repo/", file, spy);
    select(items, "rename");
    select(items, "delete");
    select(items, "copy-path");
    select(items, "open-external");
    select(items, "terminal");
    expect(spy.rename).toHaveBeenCalledWith("src/App.tsx");
    expect(spy.remove).toHaveBeenCalledWith("src/App.tsx", false);
    expect(spy.copy).toHaveBeenCalledWith("/repo/src/App.tsx");
    expect(spy.openExternal).toHaveBeenCalledWith("/repo/src/App.tsx");
    expect(spy.openTerminal).toHaveBeenCalledWith("/repo/src");
    expect(items.some((item) => item.key === "new-file")).toBe(false);
  });

  it("creates new paths inside the right-clicked folder and deletes folders recursively", () => {
    const spy = actions();
    const items = workspaceExplorerMenuItems("/repo", folder, spy);
    select(items, "new-file");
    select(items, "delete");
    select(items, "terminal");
    expect(spy.create).toHaveBeenCalledWith("file", "src/");
    expect(spy.openTerminal).toHaveBeenCalledWith("/repo/src");
    expect(spy.remove).toHaveBeenCalledWith("src", true);
    expect(items.some((item) => item.key === "open" || item.key === "attach")).toBe(false);
  });

  it("offers workspace-level actions on empty space and never a rename or delete of the workspace root", () => {
    const spy = actions();
    const items = workspaceExplorerMenuItems("/repo", null, spy);
    select(items, "new-folder");
    select(items, "reveal");
    select(items, "terminal");
    expect(spy.create).toHaveBeenCalledWith("directory", "");
    expect(spy.openTerminal).toHaveBeenCalledWith("/repo");
    expect(spy.reveal).toHaveBeenCalledWith("/repo");
    expect(items.find((item) => item.key === "copy-relative")?.disabled).toBe(true);
    expect(items.some((item) => item.key === "rename" || item.key === "delete")).toBe(false);
  });

  it("labels rename and delete with the keys the explorer rows actually handle", () => {
    const items = workspaceExplorerMenuItems("/repo", file, actions());
    expect(items.find((item) => item.key === "rename")?.shortcut).toBe(EXPLORER_RENAME_KEY);
    expect(items.find((item) => item.key === "delete")?.shortcut).toBe(EXPLORER_DELETE_KEY);
  });
});
