import { create } from "zustand";
import { isGoalRunActive, type GoalRun, type GoalRunStatus } from "@/lib/goalRuns";

export type GoalFilterId = "all" | "active" | "awaiting_review" | "completed" | "failed";
export type GoalSortId = "newest" | "attention" | "duration";
export type GoalStatusTone = "info" | "warning" | "success" | "danger" | "muted";

export type GoalSelectionState = {
  selectedRunId: string | null;
  selectedRun: GoalRun | null;
  setSelectedRun: (run: GoalRun | null) => void;
};

export const useGoalSelectionStore = create<GoalSelectionState>((set) => ({
  selectedRunId: null,
  selectedRun: null,
  setSelectedRun: (run) => set({ selectedRunId: run?.id ?? null, selectedRun: run }),
}));

export const GOAL_SELECT_EVENT = "zorai-goal-select";

export function dispatchGoalSelect(goalRunId: string, openWorkspace = false): void {
  window.dispatchEvent(new CustomEvent(GOAL_SELECT_EVENT, { detail: { goalRunId, openWorkspace } }));
}

export function goalStatusTone(status: GoalRunStatus): GoalStatusTone {
  switch (status) {
    case "running":
    case "planning":
    case "queued":
      return "info";
    case "awaiting_review":
    case "awaiting_approval":
    case "paused":
      return "warning";
    case "completed":
      return "success";
    case "failed":
      return "danger";
    default:
      return "muted";
  }
}

export function isGoalAwaitingReview(run: Pick<GoalRun, "status">): boolean {
  return run.status === "awaiting_review" || run.status === "awaiting_approval";
}

export function filterGoalRunsByTab(runs: GoalRun[], filter: GoalFilterId): GoalRun[] {
  switch (filter) {
    case "active":
      return runs.filter(isGoalRunActive);
    case "awaiting_review":
      return runs.filter(isGoalAwaitingReview);
    case "completed":
      return runs.filter((run) => run.status === "completed");
    case "failed":
      return runs.filter((run) => run.status === "failed");
    default:
      return runs;
  }
}

function goalAttentionRank(status: GoalRunStatus): number {
  if (status === "awaiting_review" || status === "awaiting_approval") return 0;
  if (status === "running" || status === "planning" || status === "queued") return 1;
  if (status === "paused") return 2;
  if (status === "failed") return 3;
  if (status === "completed") return 4;
  return 5;
}

export function searchAndFilterGoalRuns(
  runs: GoalRun[],
  options: { filter: GoalFilterId; query: string; sort: GoalSortId },
): GoalRun[] {
  const byTab = filterGoalRunsByTab(runs, options.filter);
  const needle = options.query.trim().toLowerCase();
  const matched = needle
    ? byTab.filter((run) => {
      const haystack = [
        run.title,
        run.goal,
        run.status,
        run.id,
        run.result,
        run.error,
        run.last_error,
        run.plan_summary,
      ]
        .filter((part): part is string => typeof part === "string" && part.length > 0)
        .join(" ")
        .toLowerCase();
      return haystack.includes(needle);
    })
    : byTab;

  const sorted = [...matched];
  if (options.sort === "attention") {
    sorted.sort((a, b) => goalAttentionRank(a.status) - goalAttentionRank(b.status) || b.created_at - a.created_at);
  } else if (options.sort === "duration") {
    sorted.sort((a, b) => (b.duration_ms ?? 0) - (a.duration_ms ?? 0) || b.created_at - a.created_at);
  } else {
    sorted.sort((a, b) => b.created_at - a.created_at);
  }
  return sorted;
}

export function groupGoalRunsForSupervision(runs: GoalRun[]): {
  awaitingReview: GoalRun[];
  live: GoalRun[];
  history: GoalRun[];
} {
  const awaitingReview: GoalRun[] = [];
  const live: GoalRun[] = [];
  const history: GoalRun[] = [];

  for (const run of runs) {
    if (isGoalAwaitingReview(run)) {
      awaitingReview.push(run);
    } else if (isGoalRunActive(run)) {
      live.push(run);
    } else {
      history.push(run);
    }
  }

  return { awaitingReview, live, history };
}

const KNOWN_UNIT_SUFFIX = /^(?:h|hr|hrs|m|min|mins|s|sec|secs|ms|us|ns|k|M|B|GB|MB|KB|TB|px|em|rem|st|nd|rd|th|x)$/i;

/**
 * Repairs missing word boundaries caused by stripped inline markdown tokens
 * (e.g. "at least20.0", "STILL0attempted", "cumulative$3", "required50fresh")
 * while keeping paths, identifiers, version tags (v46, op179), and units intact.
 */
export function normalizeGoalText(input: string): string {
  if (!input) return "";
  return input
    .split(/(\s+)/)
    .map((segment) => {
      if (!segment || /^\s+$/.test(segment)) return segment;
      if (segment.includes("/") || segment.includes("\\") || segment.includes("://") || segment.includes("_")) {
        return segment;
      }
      let next = segment.replace(/([a-zA-Z]{4,})(\$\d)/g, "$1 $2");
      next = next.replace(/([a-zA-Z]{4,})(\d+(?:\.\d+)?)/g, "$1 $2");
      next = next.replace(/(\d+(?:\.\d+)?)([a-zA-Z]{3,})/g, (match, num: string, word: string) => (
        KNOWN_UNIT_SUFFIX.test(word) ? match : `${num} ${word}`
      ));
      next = next.replace(/,([a-zA-Z0-9])/g, ", $1");
      return next;
    })
    .join("");
}

export function firstLinePreview(input: string, maxChars = 160): string {
  const normalized = normalizeGoalText(input).replace(/\s+/g, " ").trim();
  if (normalized.length <= maxChars) return normalized;
  return `${normalized.slice(0, maxChars - 1).trimEnd()}…`;
}
