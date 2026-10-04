import { create } from "zustand";

const MAX_OPEN_SESSIONS = 12;

type OpenSessionTabsState = {
  ids: string[];
  remember: (id: string) => void;
  close: (id: string) => string | null;
};

export type SessionCloseFollowUp =
  | { kind: "follow"; id: string }
  | { kind: "clear" }
  | { kind: "stay" };

/** Last open tab clears the thread. An active tab with neighbors follows the neighbor. */
export function sessionCloseFollowUp(input: {
  closedId: string;
  activeThreadId: string | null;
  activeDaemonThreadId: string | null;
  nextOpenId: string | null;
  openCount: number;
}): SessionCloseFollowUp {
  if (input.openCount === 0) return { kind: "clear" };
  const closedActive = input.closedId === input.activeThreadId
    || (input.activeDaemonThreadId != null && input.closedId === input.activeDaemonThreadId);
  if (closedActive && input.nextOpenId) return { kind: "follow", id: input.nextOpenId };
  return { kind: "stay" };
}

/** New sessions append. Selecting one leaves the existing order alone. */
export function rememberOpenSessionId(ids: readonly string[], id: string, max = MAX_OPEN_SESSIONS): readonly string[] {
  const trimmed = id.trim();
  if (!trimmed || ids.includes(trimmed)) return ids;
  const next = [...ids, trimmed];
  return next.length <= max ? next : next.slice(next.length - max);
}

export const useOpenSessionTabs = create<OpenSessionTabsState>((set, get) => ({
  ids: [],
  remember: (id) => {
    set((state) => {
      const ids = rememberOpenSessionId(state.ids, id);
      return ids === state.ids ? state : { ids: [...ids] };
    });
  },
  close: (id) => {
    const current = get().ids;
    const index = current.indexOf(id);
    const ids = current.filter((item) => item !== id);
    set({ ids });
    if (index < 0 || ids.length === 0) return null;
    return ids[Math.min(index, ids.length - 1)] ?? null;
  },
}));
