import { describe, expect, it, vi } from "vitest";
import {
  collectWorkspaceFiles,
  getCachedWorkspaceFiles,
  getWorkspaceFiles,
  invalidateWorkspaceFileIndex,
} from "./workspaceFileIndex";

describe("workspaceFileIndex", () => {
  it("collects files breadth-first and respects ignored dirs from the bridge", async () => {
    const bridge = {
      workspaceListDirectory: vi.fn(async (root: string, relativePath: string) => {
        if (relativePath === "") return [
          { name: "src", path: "src", isDirectory: true, isSymbolicLink: false, sizeBytes: null, modifiedAt: null },
          { name: "README.md", path: "README.md", isDirectory: false, isSymbolicLink: false, sizeBytes: 10, modifiedAt: 1 },
          { name: "nested.txt", path: "nested.txt", isDirectory: false, isSymbolicLink: false, sizeBytes: 10, modifiedAt: 1 },
        ] as ZoraiWorkspaceEntry[];
        if (relativePath === "src") return [
          { name: "threads", path: "src/threads.ts", isDirectory: false, isSymbolicLink: false, sizeBytes: 10, modifiedAt: 1 },
          { name: "sub", path: "src/sub", isDirectory: true, isSymbolicLink: false, sizeBytes: null, modifiedAt: null },
        ] as ZoraiWorkspaceEntry[];
        if (relativePath === "src/sub") return [
          { name: "deep.ts", path: "src/sub/deep.ts", isDirectory: false, isSymbolicLink: false, sizeBytes: 10, modifiedAt: 1 },
        ] as ZoraiWorkspaceEntry[];
        return [] as ZoraiWorkspaceEntry[];
      }),
    };

    const files = await collectWorkspaceFiles(bridge, "/workspace");
    expect(files).toEqual(["README.md", "nested.txt", "src/threads.ts", "src/sub/deep.ts"]);
    expect(bridge.workspaceListDirectory).toHaveBeenCalledWith("/workspace", "");
    expect(bridge.workspaceListDirectory).toHaveBeenCalledWith("/workspace", "src");
  });

  it("bounds the walk and is cancellable", async () => {
    const bridge = {
      workspaceListDirectory: vi.fn(async () => [{ name: "a.ts", path: "a.ts", isDirectory: false, isSymbolicLink: false, sizeBytes: 1, modifiedAt: 1 }] as ZoraiWorkspaceEntry[]),
    };
    const ac = new AbortController();
    ac.abort();
    const files = await collectWorkspaceFiles(bridge, "/workspace", { signal: ac.signal });
    expect(files).toHaveLength(0);
  });

  it("caches per root and invalidates", async () => {
    const root = `/tmp/zorai-test-${Date.now()}`;
    invalidateWorkspaceFileIndex(root);
    expect(getCachedWorkspaceFiles(root)).toBeNull();
    const bridge = {
      workspaceListDirectory: vi.fn(async (r: string, dir: string) => (dir === "" ? [{ name: "a.ts", path: "a.ts", isDirectory: false, isSymbolicLink: false, sizeBytes: 1, modifiedAt: 1 }] as ZoraiWorkspaceEntry[] : [] as ZoraiWorkspaceEntry[])),
    };
    const first = await getWorkspaceFiles(bridge, root);
    expect(first).toEqual(["a.ts"]);
    expect(getCachedWorkspaceFiles(root)).toEqual(["a.ts"]);
    invalidateWorkspaceFileIndex(root);
    expect(getCachedWorkspaceFiles(root)).toBeNull();
    const second = await getWorkspaceFiles(bridge, root, { force: true });
    expect(second).toEqual(["a.ts"]);
  });

  it("indexes nested folders even when an earlier caller was cancelled, as React does when it remounts an effect", async () => {
    const root = `/tmp/zorai-nested-${Date.now()}`;
    const entry = (path: string, isDirectory: boolean) => ({ name: path.split("/").pop() ?? path, path, isDirectory, isSymbolicLink: false, sizeBytes: 1, modifiedAt: 1 }) as ZoraiWorkspaceEntry;
    const bridge = {
      workspaceListDirectory: vi.fn(async (_root: string, dir: string) => dir === "" ? [entry("README.md", false), entry("data", true)] : dir === "data" ? [entry("data/questions.jsonl", false)] : []),
    };
    const first = getWorkspaceFiles(bridge, root);
    const second = getWorkspaceFiles(bridge, root);
    expect(await first).toEqual(["README.md", "data/questions.jsonl"]);
    expect(await second).toEqual(["README.md", "data/questions.jsonl"]);
  });
});
