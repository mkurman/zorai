import { describe, expect, it } from "vitest";
import { pendingReviewFiles, reviewKey } from "./agentDiffReviewStore";

describe("agent diff review", () => {
  const entries = [
    { path: "src/a.ts", operation_id: "op2", updated_at: 20 },
    { path: "src/a.ts", operation_id: "op1", updated_at: 10 },
    { path: "src/b.ts", operation_id: "op3", updated_at: 30 },
    { path: "notes.md", operation_id: null, updated_at: 40 },
    { path: "img.png", operation_id: "op4", updated_at: 50, is_text: false },
  ];

  it("diffs each file against its state before the agent's first edit, so later edits do not hide earlier ones", () => {
    const pending = pendingReviewFiles(entries, []);
    expect(pending.map((file) => file.path)).toEqual(["src/b.ts", "src/a.ts"]);
    expect(pending[1]).toMatchObject({ baselineOperationId: "op1", operationIds: ["op1", "op2"] });
  });

  it("skips changes without a snapshot and binary files, which cannot be shown as a text diff", () => {
    expect(pendingReviewFiles(entries, []).map((file) => file.path)).not.toContain("notes.md");
    expect(pendingReviewFiles(entries, []).map((file) => file.path)).not.toContain("img.png");
  });

  it("does not show a change again once it was kept or undone", () => {
    const pending = pendingReviewFiles(entries, [reviewKey("op1", "src/a.ts"), reviewKey("op2", "src/a.ts")]);
    expect(pending.map((file) => file.path)).toEqual(["src/b.ts"]);
  });

  it("a newer agent edit to an already reviewed file is reviewed from that edit on", () => {
    const pending = pendingReviewFiles(entries, [reviewKey("op1", "src/a.ts")]);
    expect(pending.find((file) => file.path === "src/a.ts")?.baselineOperationId).toBe("op2");
  });
});
