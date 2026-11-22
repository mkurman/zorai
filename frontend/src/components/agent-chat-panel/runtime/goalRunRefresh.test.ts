import { beforeEach, describe, expect, it, vi } from "vitest";
import { normalizeGoalRun, type GoalRun } from "@/lib/goalRuns";
import { handleGoalRunEvent, handleTodoUpdateEvent } from "./daemonEventHandlers";

const goalRunsApi = vi.hoisted(() => ({
  fetchGoalRun: vi.fn(),
  fetchGoalRuns: vi.fn(),
}));

vi.mock("@/lib/goalRuns", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/goalRuns")>()),
  fetchGoalRun: goalRunsApi.fetchGoalRun,
  fetchGoalRuns: goalRunsApi.fetchGoalRuns,
}));

vi.mock("@/lib/agentTodos", () => ({
  fetchThreadTodos: vi.fn(async () => []),
}));

function run(id: string, goal: string, createdAt: number): GoalRun {
  const normalized = normalizeGoalRun({ id, goal, title: goal, status: "running", created_at: createdAt });
  if (!normalized) throw new Error("test goal run must normalize");
  return normalized;
}

function goalRunsState(initial: GoalRun[]) {
  let current = initial;
  const setGoalRunsForTrace = vi.fn((update: GoalRun[] | ((value: GoalRun[]) => GoalRun[])) => {
    current = typeof update === "function" ? update(current) : update;
  });
  return { setGoalRunsForTrace, get: () => current };
}

function todoEvent(goalRunId: string) {
  return {
    event: { thread_id: "daemon-thread", goal_run_id: goalRunId, items: [{ id: "t1", content: "step", status: "in_progress" }] },
    daemonLocalThreadRef: { current: null },
    daemonThreadIdRef: { current: null },
    setDaemonTodosByThread: vi.fn(),
    setThreadTodos: vi.fn(),
  };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("goal run refresh on daemon events", () => {
  beforeEach(() => {
    goalRunsApi.fetchGoalRun.mockReset();
    goalRunsApi.fetchGoalRuns.mockReset();
  });

  it("a todo update reloads only its own goal and keeps the other goals' objects untouched", async () => {
    const other = run("goal-other", "other goal", 1);
    const state = goalRunsState([run("goal-a", "old text", 2), other]);
    goalRunsApi.fetchGoalRun.mockResolvedValue(run("goal-a", "new text", 2));

    handleTodoUpdateEvent({ ...todoEvent("goal-a"), setGoalRunsForTrace: state.setGoalRunsForTrace });
    await flush();

    expect(goalRunsApi.fetchGoalRun).toHaveBeenCalledWith("goal-a");
    expect(goalRunsApi.fetchGoalRuns).not.toHaveBeenCalled();
    expect(state.get().map((entry) => entry.goal)).toEqual(["new text", "other goal"]);
    expect(state.get()[1]).toBe(other);
  });

  it("a slower earlier reload cannot overwrite the goal with stale data", async () => {
    const state = goalRunsState([run("goal-a", "initial", 1)]);
    let resolveStale: (value: GoalRun) => void = () => {};
    goalRunsApi.fetchGoalRun
      .mockImplementationOnce(() => new Promise<GoalRun>((resolve) => { resolveStale = resolve; }))
      .mockResolvedValueOnce(run("goal-a", "fresh", 1));

    handleTodoUpdateEvent({ ...todoEvent("goal-a"), setGoalRunsForTrace: state.setGoalRunsForTrace });
    handleTodoUpdateEvent({ ...todoEvent("goal-a"), setGoalRunsForTrace: state.setGoalRunsForTrace });
    await flush();
    resolveStale(run("goal-a", "stale", 1));
    await flush();

    expect(state.get().map((entry) => entry.goal)).toEqual(["fresh"]);
  });

  it("falls back to the full list when the single goal cannot be loaded, so the trace never silently goes stale", async () => {
    const state = goalRunsState([run("goal-a", "initial", 1)]);
    goalRunsApi.fetchGoalRun.mockResolvedValue(null);
    goalRunsApi.fetchGoalRuns.mockResolvedValue([run("goal-a", "from list", 1)]);

    handleTodoUpdateEvent({ ...todoEvent("goal-a"), setGoalRunsForTrace: state.setGoalRunsForTrace });
    await flush();
    await flush();

    expect(state.get().map((entry) => entry.goal)).toEqual(["from list"]);
  });

  it("a goal update event applies the goal it carries without asking the daemon again", () => {
    const state = goalRunsState([run("goal-old", "older goal", 1)]);

    handleGoalRunEvent({
      event: { goal_run: { id: "goal-new", goal: "new goal", title: "new goal", status: "running", created_at: 5 } },
      activePaneId: null,
      activeWorkspace: null,
      addNotification: vi.fn(),
      cleanupGoalRunWorkspace: vi.fn(),
      setGoalRunsForTrace: state.setGoalRunsForTrace,
    });

    expect(goalRunsApi.fetchGoalRun).not.toHaveBeenCalled();
    expect(goalRunsApi.fetchGoalRuns).not.toHaveBeenCalled();
    expect(state.get().map((entry) => entry.id)).toEqual(["goal-new", "goal-old"]);
  });
});
