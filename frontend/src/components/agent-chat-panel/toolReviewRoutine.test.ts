import { describe, expect, it } from "vitest";
import { isRoutineToolReview } from "./toolReviewPresentation";

describe("routine tool reviews", () => {
  it("hides plain allow decisions so every safe call is not shown as Flagged", () => {
    expect(isRoutineToolReview({ weles_reviewed: true, verdict: "allow", reasons: [] })).toBe(true);
    expect(isRoutineToolReview({ weles_reviewed: false, verdict: "flag_only", reasons: ["allow_direct: low-risk tool call"] })).toBe(true);
  });

  it("keeps real warnings and blocks visible", () => {
    expect(isRoutineToolReview({ weles_reviewed: true, verdict: "flag_only", reasons: ["writes outside workspace"] })).toBe(false);
    expect(isRoutineToolReview({ weles_reviewed: true, verdict: "block", reasons: ["allow_direct"] })).toBe(false);
    expect(isRoutineToolReview({ weles_reviewed: true, verdict: "flag_only", reasons: [] })).toBe(false);
  });
});
