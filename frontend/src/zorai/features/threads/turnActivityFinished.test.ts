import { describe, expect, it } from "vitest";
import { finishedTurnStats, finishedTurnTitle } from "./ThreadTurnActivityList";

describe("finished turn summary", () => {
  it("collapses a finished turn into one quiet 'Worked for' line instead of the last tool name", () => {
    expect(finishedTurnTitle(4 * 60_000)).toMatch(/^Worked for /);
    expect(finishedTurnTitle(300)).toBe("Worked");
  });

  it("counts steps and surfaces failures, since a failed step is the one thing worth noticing", () => {
    expect(finishedTurnStats(12, 0, 3)).toBe("12 steps");
    expect(finishedTurnStats(1, 0, 1)).toBe("1 step");
    expect(finishedTurnStats(5, 2, 2)).toBe("5 steps · 2 failed");
    expect(finishedTurnStats(0, 0, 2)).toBe("2 steps");
  });
});
