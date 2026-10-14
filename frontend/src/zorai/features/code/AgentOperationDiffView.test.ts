import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ bridge: {} as Record<string, unknown> }));
vi.mock("@/lib/bridge", () => ({ getBridge: () => mocks.bridge }));

import { loadAgentOperationDiff } from "./AgentOperationDiffView";

const target = { operationId: "call_1", path: "knowledge/README.md", tool: "append_to_file" };

describe("loadAgentOperationDiff", () => {
  it("diffs the retained pre-operation bytes against the file the daemon resolved, not the relative label", async () => {
    const readFsText = vi.fn(async () => "one\ntwo\n");
    mocks.bridge = {
      agentGetFileOperationBefore: vi.fn(async () => ({ operation_id: "call_1", path: target.path, resolved_path: "/repo/knowledge/README.md", available: true, before_existed: true, binary: false, content: "one\n", error: null })),
      readFsText,
    };
    expect(await loadAgentOperationDiff(target)).toEqual({ status: "ready", before: "one\n", current: "one\ntwo\n", resolvedPath: "/repo/knowledge/README.md", created: false, deleted: false });
    expect(readFsText).toHaveBeenCalledWith("/repo/knowledge/README.md");
  });

  it("reports why there is nothing to diff instead of showing an empty editor", async () => {
    mocks.bridge = {
      agentGetFileOperationBefore: vi.fn(async () => ({ operation_id: "call_1", path: target.path, resolved_path: null, available: false, before_existed: false, binary: false, content: null, error: "No retained snapshot exists for this operation." })),
      readFsText: vi.fn(),
    };
    expect(await loadAgentOperationDiff(target)).toEqual({ status: "error", message: "No retained snapshot exists for this operation." });
  });

  it("marks files the operation created and files deleted since", async () => {
    mocks.bridge = {
      agentGetFileOperationBefore: vi.fn(async () => ({ operation_id: "call_1", path: target.path, resolved_path: "/repo/new.md", available: true, before_existed: false, binary: false, content: "", error: null })),
      readFsText: vi.fn(async () => null),
    };
    expect(await loadAgentOperationDiff(target)).toMatchObject({ status: "ready", created: true, deleted: true, before: "", current: "" });
  });
});
