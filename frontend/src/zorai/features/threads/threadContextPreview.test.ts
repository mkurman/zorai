import { describe, expect, it } from "vitest";

import type { WorkContextEntry } from "@/lib/agentWorkContext";
import { previewRequestsForWorkContextEntry, threadContextEntryKey, isMarkdownPath } from "./threadContextPreview";

function entry(overrides: Partial<WorkContextEntry>): WorkContextEntry {
  return {
    path: overrides.path ?? "src/App.tsx",
    kind: overrides.kind ?? "repo_change",
    source: overrides.source ?? "daemon",
    isText: overrides.isText ?? true,
    updatedAt: overrides.updatedAt ?? 1,
    ...overrides,
  };
}

describe("thread context preview helpers", () => {
  it("requests file preview plus optional git diff for repo-backed work context entries", () => {
    expect(previewRequestsForWorkContextEntry(entry({ repoRoot: "/repo", path: "src/App.tsx" }))).toEqual([
      {
        type: "file-preview",
        path: "/repo/src/App.tsx",
      },
      {
        type: "git-diff",
        repoRoot: "/repo",
        filePath: "src/App.tsx",
      },
    ]);
  });

  it("requests plain file preview for artifact entries without a repo root", () => {
    expect(previewRequestsForWorkContextEntry(entry({ repoRoot: null, path: "/tmp/result.txt" }))).toEqual([{
      type: "file-preview",
      path: "/tmp/result.txt",
    }]);
  });

  it("keeps entries with the same path but different repo roots distinct", () => {
    expect(threadContextEntryKey(entry({ source: "daemon", repoRoot: "/repo-a", path: "README.md" })))
      .not.toBe(threadContextEntryKey(entry({ source: "daemon", repoRoot: "/repo-b", path: "README.md" })));
  });

  it("keeps separate operations on the same file distinct, as the daemon records one entry per operation", () => {
    const path = "/home/u/.zorai/threads/t/artifacts/specs/run-analysis.md";
    expect(threadContextEntryKey(entry({ source: "append_to_file", repoRoot: null, path, operationId: "op-1" })))
      .not.toBe(threadContextEntryKey(entry({ source: "append_to_file", repoRoot: null, path, operationId: "op-2" })));
  });

  it("treats markdown extensions the same way TUI file preview does", () => {
    expect(isMarkdownPath("docs/README.md")).toBe(true);
    expect(isMarkdownPath("notes.MARKDOWN")).toBe(true);
    expect(isMarkdownPath("page.mdx")).toBe(true);
    expect(isMarkdownPath("src/App.tsx")).toBe(false);
    expect(isMarkdownPath("/tmp/result.txt")).toBe(false);
  });
});
