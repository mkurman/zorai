import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import type { InboxFilter } from "./inboxModel";

type InboxReadState = {
  baselineAt: number;
  threadReadAt: Record<string, number>;
  threadArchivedAt: Record<string, number>;
  markThreadRead: (threadKey: string, at: number) => void;
  markThreadUnread: (threadKey: string) => void;
  archiveThread: (threadKey: string, at: number) => void;
};

const MAX_TRACKED = 500;

function capped(record: Record<string, number>): Record<string, number> {
  const entries = Object.entries(record);
  if (entries.length <= MAX_TRACKED) return record;
  return Object.fromEntries(entries.sort((a, b) => b[1] - a[1]).slice(0, MAX_TRACKED));
}

export const useInboxStore = create<InboxReadState>()(persist(
  (set) => ({
    baselineAt: Date.now(),
    threadReadAt: {},
    threadArchivedAt: {},
    markThreadRead: (threadKey, at) => set((state) => (
      (state.threadReadAt[threadKey] ?? 0) >= at ? state : { threadReadAt: capped({ ...state.threadReadAt, [threadKey]: at }) }
    )),
    markThreadUnread: (threadKey) => set((state) => ({ threadReadAt: { ...state.threadReadAt, [threadKey]: 0 } })),
    archiveThread: (threadKey, at) => set((state) => ({
      threadReadAt: capped({ ...state.threadReadAt, [threadKey]: Math.max(at, state.threadReadAt[threadKey] ?? 0) }),
      threadArchivedAt: capped({ ...state.threadArchivedAt, [threadKey]: at }),
    })),
  }),
  {
    name: "zorai.inbox",
    storage: createJSONStorage(() => localStorage),
    partialize: (state) => ({
      baselineAt: state.baselineAt,
      threadReadAt: state.threadReadAt,
      threadArchivedAt: state.threadArchivedAt,
    }),
  },
));

export type ActivityTab = "inbox" | "timeline" | "planner";

type InboxViewState = {
  tab: ActivityTab;
  filter: InboxFilter;
  setTab: (tab: ActivityTab) => void;
  setFilter: (filter: InboxFilter) => void;
};

export const useInboxViewStore = create<InboxViewState>((set) => ({
  tab: "inbox",
  filter: "all",
  setTab: (tab) => set({ tab }),
  setFilter: (filter) => set({ filter }),
}));
