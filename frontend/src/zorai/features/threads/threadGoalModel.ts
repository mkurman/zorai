import { isGoalRunTerminal, type GoalRun, type GoalRunControlAction } from "@/lib/goalRuns";

export function goalRunForThread(goalRuns: readonly GoalRun[], threadIds: ReadonlyArray<string | null | undefined>): GoalRun | null {
  const ids = new Set(threadIds.filter((id): id is string => Boolean(id)));
  if (ids.size === 0) return null;
  const matches = goalRuns.filter((goal) => [goal.thread_id, goal.root_thread_id, goal.active_thread_id, ...(goal.execution_thread_ids ?? [])]
    .some((id) => Boolean(id) && ids.has(id as string)));
  if (matches.length === 0) return null;
  return [...matches].sort((left, right) => (
    Number(isGoalRunTerminal(left)) - Number(isGoalRunTerminal(right))
    || (right.started_at ?? right.created_at) - (left.started_at ?? left.created_at)
  ))[0];
}

export type GoalControlOption = { action: GoalRunControlAction; label: string; title: string; danger?: boolean };

export function goalControlOptions(goal: Pick<GoalRun, "status">): GoalControlOption[] {
  if (isGoalRunTerminal(goal.status)) return [];
  const cancel: GoalControlOption = { action: "cancel", label: "Cancel", title: "Stop the goal for good and cancel its workers", danger: true };
  if (goal.status === "paused") {
    return [{ action: "resume", label: "Resume", title: "Continue the goal and deliver anything queued while it was paused" }, cancel];
  }
  return [{ action: "pause", label: "Pause", title: "Stop starting new work until you resume it" }, cancel];
}
