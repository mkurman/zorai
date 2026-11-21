import { describe, expect, it } from "vitest";
import type { GoalRun } from "@/lib/goalRuns";
import {
  filterGoalRunsByTab,
  firstLinePreview,
  goalStatusTone,
  groupGoalRunsForSupervision,
  normalizeGoalText,
  searchAndFilterGoalRuns,
} from "./goalPresentation";

function makeRun(id: string, status: GoalRun["status"]): GoalRun {
  return {
    id,
    title: `Goal ${id}`,
    goal: `Goal ${id}`,
    status,
    created_at: 1,
    replan_count: 0,
  };
}

describe("goalPresentation", () => {
  it("repairs stripped inline spacing without corrupting paths, versions, or units", () => {
    expect(normalizeGoalText("at least20.0 hours (72000seconds)")).toBe("at least 20.0 hours (72000 seconds)");
    expect(normalizeGoalText("fresh qualification is STILL0attempted")).toBe("fresh qualification is STILL 0 attempted");
    expect(normalizeGoalText("cumulative$3 bounds persisted in /home/user/.zorai/v46/op179")).toBe("cumulative $3 bounds persisted in /home/user/.zorai/v46/op179");
    expect(normalizeGoalText("train 20h on v46 with 60M tokens")).toBe("train 20h on v46 with 60M tokens");
  });

  it("clamps long rejection verdicts into a single-line preview", () => {
    const long = `HARD_REJECT ${"a".repeat(200)}`;
    const preview = firstLinePreview(long, 60);
    expect(preview.length).toBeLessThanOrEqual(60);
    expect(preview.endsWith("…")).toBe(true);
  });

  it("maps statuses to semantic tones and separates review, live, and history groups", () => {
    expect(goalStatusTone("running")).toBe("info");
    expect(goalStatusTone("awaiting_review")).toBe("warning");
    expect(goalStatusTone("completed")).toBe("success");
    expect(goalStatusTone("failed")).toBe("danger");

    const runs = [
      makeRun("1", "running"),
      makeRun("2", "awaiting_review"),
      makeRun("3", "failed"),
      makeRun("4", "completed"),
    ];
    const groups = groupGoalRunsForSupervision(runs);
    expect(groups.awaitingReview.map((r) => r.id)).toEqual(["2"]);
    expect(groups.live.map((r) => r.id)).toEqual(["1"]);
    expect(groups.history.map((r) => r.id)).toEqual(["3", "4"]);

    expect(filterGoalRunsByTab(runs, "failed").map((r) => r.id)).toEqual(["3"]);
    expect(filterGoalRunsByTab(runs, "awaiting_review").map((r) => r.id)).toEqual(["2"]);
  });

  it("searches across title/error/goal and sorts by attention or duration", () => {
    const runs: GoalRun[] = [
      { ...makeRun("a", "completed"), title: "Alpha release", duration_ms: 1200, created_at: 10 },
      { ...makeRun("b", "awaiting_review"), title: "Beta audit", duration_ms: 500, created_at: 20 },
      { ...makeRun("c", "failed"), title: "Gamma check", error: "alpha timeout", duration_ms: 9000, created_at: 30 },
    ];
    expect(searchAndFilterGoalRuns(runs, { filter: "all", query: "alpha", sort: "newest" }).map((r) => r.id)).toEqual(["c", "a"]);
    expect(searchAndFilterGoalRuns(runs, { filter: "all", query: "", sort: "attention" }).map((r) => r.id)).toEqual(["b", "c", "a"]);
    expect(searchAndFilterGoalRuns(runs, { filter: "all", query: "", sort: "duration" }).map((r) => r.id)).toEqual(["c", "a", "b"]);
  });
});
