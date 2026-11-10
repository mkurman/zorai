import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { displayRootName } from "./codeEmptyStateModel";
import type { CodeOpenWorkspaceBoundary } from "./CodeView";

function readSource(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

const sampleRoot = {
  root: "/work/a",
  name: "a",
  gitRoot: null,
  isGitRepository: false,
} as const;

describe("CodeView pure helpers", () => {
  it("derives a display name from the canonical root path", () => {
    expect(displayRootName("/work/example")).toBe("example");
    expect(displayRootName("C:\\work\\app")).toBe("app");
    expect(displayRootName("/mnt/e/gitlab/it/zorai/src")).toBe("src");
  });

  it("falls back for null and empty roots", () => {
    expect(displayRootName(null)).toBe("No repository open.");
    expect(displayRootName("   ")).toBe("No repository open.");
  });

  it("keeps the GLM lifecycle boundary typed and callable", () => {
    const boundary: CodeOpenWorkspaceBoundary = (root, source) => {
      expect(root.root).toBe("/work/a");
      expect(source).toMatch(/^(picker|manual)$/);
    };
    boundary(sampleRoot, "picker");
  });
});

describe("CodeView controller stability contract", () => {
  const source = readSource("./CodeView.tsx");
  const emptyStateSource = readSource("./CodeEmptyState.tsx");

  it("uses a stable module-level default for onOpenWorkspace instead of an inline arrow", () => {
    expect(source).toContain("const NOOP_OPEN_WORKSPACE");
    expect(source).toContain("onOpenWorkspace = NOOP_OPEN_WORKSPACE");
    expect(source).not.toContain("onOpenWorkspace = () => {}");
  });

  it("stabilizes handleRootSelected with useCallback so parent rerenders reuse the controller", () => {
    expect(source).toContain("useCallback");
    expect(source).toContain("const handleRootSelected = useCallback");
    expect(source).toContain("onRootSelected={handleRootSelected}");
    expect(source).toContain("onOpenWorkspace(root, source)");
  });

  it("keeps an in-flight native picker bound across runtime provider rerenders", () => {
    expect(source).toContain("const runtimeRef = useRef(runtime)");
    expect(source).toContain("runtimeRef.current = runtime");
    expect(source).toContain("const activeRuntime = runtimeRef.current");
    expect(source).toContain("openThreadTarget(activeRuntime, mappedDaemonThreadId)");
    expect(source).toContain("[bindRoot]");
    expect(source).not.toContain("[bindRoot, runtime]");
  });

  it("keeps the secure nested picker envelope by passing through only validated roots", () => {
    expect(source).toContain("setLastRoot(root.root)");
    expect(source).toContain("setBoundRoot(root.root)");
    expect(emptyStateSource).toContain("onRootSelected");
    expect(emptyStateSource).toContain("createMemoizedCodeEmptyStateController");
    expect(emptyStateSource).toContain("NOOP_ROOT_SELECTED");
  });
});

describe("CodeRail action surface", () => {
  const source = readSource("./CodeView.tsx");

  it("renders open file, open folder, and recent buttons inside zorai-code-project", () => {
    expect(source).toContain('className="zorai-code-project"');
    expect(source).toContain('className="zorai-code-project-actions"');
    expect(source).toContain('aria-label="Open file"');
    expect(source).toContain('aria-label="Open folder"');
    expect(source).toContain('aria-label="Open recent folder"');
  });

  it("emits rail actions through the codeRailActions bus", () => {
    expect(source).toContain('emitCodeRailAction({ kind: "open-file" })');
    expect(source).toContain('emitCodeRailAction({ kind: "open-folder" })');
    expect(source).toContain('emitCodeRailAction({ kind: "open-recent", root })');
  });

  it("subscribes CodeView to the bus so rail buttons reach the mounted Code view", () => {
    expect(source).toContain("subscribeCodeRailActions(");
    expect(source).toContain('action.kind === "open-folder"');
    expect(source).toContain('action.kind === "open-recent"');
    expect(source).toContain('action.kind === "open-file"');
  });

  it("uses the existing workspaceSelectFolder flow for open-folder", () => {
    expect(source).toContain("bridge.workspaceSelectFolder()");
    expect(source).toContain('handleRootSelected(selection.root, "picker")');
  });

  it("uses the existing workspaceOpen re-validation for open-recent", () => {
    expect(source).toContain("bridge.workspaceOpen(action.root)");
    expect(source).toContain('handleRootSelected(validated, "picker")');
  });

  it("requests external file edits via workspaceEditorRequestStore with the external flag", () => {
    expect(source).toContain('requestFileView(localId, selection.path, "edit", { external: true })');
  });

  it("lists the recent-folder history from codeWorkspaceBindingStore, not only roots that still have project threads", () => {
    expect(source).toContain("useCodeWorkspaceBindingStore((state) => state.recentRoots)");
    expect(source).not.toContain("Object.keys(threadsByRoot)");
    expect(source).toContain("root === lastRoot");
  });

  it("exposes closeRoot for pruning recent entries", () => {
    expect(source).toContain("closeRoot(root)");
  });
});
