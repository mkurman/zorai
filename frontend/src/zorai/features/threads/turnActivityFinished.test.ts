import { describe, expect, it } from "vitest";
import type { ToolEventGroup } from "@/components/agent-chat-panel/chat-view/types";
import { finishedTurnStats } from "./ThreadTurnActivityList";
import { finishedTurnSummaries, finishedTurnSummary } from "./turnActivitySummary";

describe("finished turn summary", () => {
  const group = (toolName: string, args: Record<string, unknown> = {}) => ({ toolName, toolArguments: JSON.stringify(args) }) as ToolEventGroup;
  const groups = [
    group("bash", { command: "ls" }),
    group("bash", { command: "pwd" }),
    group("read_file", { path: "a.py" }),
    group("read_file", { path: "a.py" }),
    group("read_file", { path: "b.py" }),
    group("edit_file", { path: "a.py" }),
  ];

  it("offers varied summaries built from what the turn actually did, counting distinct files", () => {
    expect(finishedTurnSummaries(groups, 4 * 60_000, "Edit a.py")).toEqual([
      expect.stringMatching(/^Worked for /),
      "Ran 2 commands",
      "Read 2 files",
      "Edited 1 file",
      "Edit a.py",
    ]);
  });

  it("keeps the same summary for the same turn so headers do not shuffle on every render or restart", () => {
    const first = finishedTurnSummary("turn-42", groups, 60_000, "Edit a.py");
    expect(finishedTurnSummary("turn-42", groups, 60_000, "Edit a.py")).toBe(first);
    const picks = new Set(["a", "b", "c", "d", "e", "f", "g", "h"].map((key) => finishedTurnSummary(key, groups, 60_000, "Edit a.py")));
    expect(picks.size).toBeGreaterThan(1);
  });

  it("falls back to 'Worked' when there is nothing to summarize", () => {
    expect(finishedTurnSummary("x", [], 300, null)).toBe("Worked");
  });

  it("counts steps and surfaces failures, since a failed step is the one thing worth noticing", () => {
    expect(finishedTurnStats(12, 0, 3)).toBe("12 steps");
    expect(finishedTurnStats(1, 0, 1)).toBe("1 step");
    expect(finishedTurnStats(5, 2, 2)).toBe("5 steps · 2 failed");
    expect(finishedTurnStats(0, 0, 2)).toBe("2 steps");
  });
});
