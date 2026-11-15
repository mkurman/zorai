import { describe, expect, it } from "vitest";
import type { GoalRun } from "@/lib/goalRuns";
import { goalControlOptions, goalRunForThread } from "./threadGoalModel";

const goal = (id: string, status: GoalRun["status"], created_at: number, threads: Partial<GoalRun>) => ({ id, title: id, goal: id, status, created_at, replan_count: 0, ...threads }) as GoalRun;

describe("thread goal", () => {
  it("finds the goal a thread works for, preferring the live goal over an older finished one", () => {
    const goals = [
      goal("old", "completed", 1, { thread_id: "goal:a" }),
      goal("live", "paused", 2, { execution_thread_ids: ["goal:a"] }),
      goal("other", "running", 3, { thread_id: "goal:b" }),
    ];
    expect(goalRunForThread(goals, ["goal:a", "local-1"])?.id).toBe("live");
    expect(goalRunForThread(goals, ["thread-plain"])).toBeNull();
    expect(goalRunForThread(goals, [null, undefined])).toBeNull();
  });

  it("offers resume only for a paused goal and no controls once it has ended", () => {
    expect(goalControlOptions({ status: "paused" }).map((option) => option.action)).toEqual(["resume", "cancel"]);
    expect(goalControlOptions({ status: "running" }).map((option) => option.action)).toEqual(["pause", "cancel"]);
    expect(goalControlOptions({ status: "cancelled" })).toEqual([]);
  });
});
