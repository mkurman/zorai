import { describe, expect, it } from "vitest";
import { checkpointRestorePlan, toMillis } from "./threadCheckpoint";

describe("thread checkpoints", () => {
  const T = 1_790_000_000_000;
  const changes = [
    { path: "src/a.ts", operation_id: "op1", updated_at: T + 1_000 },
    { path: "src/a.ts", operation_id: "op3", updated_at: T + 3_000 },
    { path: "src/a.ts", operation_id: "op2", updated_at: T + 2_000 },
    { path: "src/b.ts", operation_id: "op4", updated_at: T + 4_000 },
    { path: "notes.md", operation_id: null, updated_at: T + 5_000 },
  ];

  it("restores each file from its first agent edit after the checkpoint, undoing every later edit too", () => {
    expect(checkpointRestorePlan(changes, T + 1_500)).toEqual([
      { path: "src/a.ts", operationId: "op2", repoRoot: null },
      { path: "src/b.ts", operationId: "op4", repoRoot: null },
    ]);
  });

  it("leaves files the agent changed before the checkpoint alone", () => {
    expect(checkpointRestorePlan(changes, T + 3_500).map((step) => step.path)).toEqual(["src/b.ts"]);
  });

  it("compares second and millisecond timestamps correctly", () => {
    expect(toMillis(1_700_000_000)).toBe(1_700_000_000_000);
    expect(toMillis(1_700_000_000_000)).toBe(1_700_000_000_000);
  });
});
