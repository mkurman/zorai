import { create } from "zustand";

export type AgentChangeEntry = { path: string; operation_id?: string | null; updated_at: number; is_text?: boolean };

export type PendingReviewFile = { path: string; baselineOperationId: string; operationIds: string[]; updatedAt: number };

const STORAGE_KEY = "zorai.code.agentReviewDecided";
const MAX_REMEMBERED = 2000;

function readDecided(): Record<string, string[]> {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Record<string, string[]>;
  } catch {
    return {};
  }
}

export function pendingReviewFiles(entries: AgentChangeEntry[], decided: string[]): PendingReviewFile[] {
  const decidedKeys = new Set(decided);
  const byPath = new Map<string, AgentChangeEntry[]>();
  for (const entry of entries) {
    if (!entry.operation_id || entry.is_text === false || decidedKeys.has(reviewKey(entry.operation_id, entry.path))) continue;
    byPath.set(entry.path, [...(byPath.get(entry.path) ?? []), entry]);
  }
  return [...byPath.entries()]
    .map(([path, changes]) => {
      const ordered = [...changes].sort((left, right) => left.updated_at - right.updated_at);
      return {
        path,
        baselineOperationId: ordered[0].operation_id as string,
        operationIds: [...new Set(ordered.map((change) => change.operation_id as string))],
        updatedAt: ordered[ordered.length - 1].updated_at,
      };
    })
    .sort((left, right) => right.updatedAt - left.updatedAt);
}

export function reviewKey(operationId: string, path: string) {
  return `${operationId}\u0000${path}`;
}

type AgentDiffReviewState = {
  decided: Record<string, string[]>;
  markDecided: (threadId: string, keys: string[]) => void;
};

export const useAgentDiffReviewStore = create<AgentDiffReviewState>((set) => ({
  decided: readDecided(),
  markDecided: (threadId, keys) => set((state) => {
    const merged = [...new Set([...(state.decided[threadId] ?? []), ...keys])].slice(-MAX_REMEMBERED);
    const decided = { ...state.decided, [threadId]: merged };
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(decided));
    } catch {
      return { decided };
    }
    return { decided };
  }),
}));
