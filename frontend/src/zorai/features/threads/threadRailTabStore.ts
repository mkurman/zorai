import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { DEFAULT_THREAD_DATE_FILTER, fixedThreadTabs, type DateFilterId, type ThreadFilterTab } from "./threadFilterModel";

type ThreadRailTabState = {
  tab: ThreadFilterTab;
  favorites: ThreadFilterTab[];
  filtersOpen: boolean;
  dateFilter: DateFilterId;
  fromDate: string;
  toDate: string;
  setTab: (tab: ThreadFilterTab) => void;
  toggleFavorite: (tab: ThreadFilterTab) => void;
  setFiltersOpen: (open: boolean | ((current: boolean) => boolean)) => void;
  setDateFilter: (dateFilter: DateFilterId) => void;
  setFromDate: (fromDate: string) => void;
  setToDate: (toDate: string) => void;
};

export function persistedThreadRailState(state: ThreadRailTabState) {
  return {
    tab: state.tab,
    favorites: state.favorites,
    filtersOpen: state.filtersOpen,
    dateFilter: state.dateFilter,
    fromDate: state.fromDate,
    toDate: state.toDate,
  };
}

export const useThreadRailTabStore = create<ThreadRailTabState>()(
  persist(
    (set) => ({
      tab: "svarog",
      favorites: fixedThreadTabs.map((item) => item.id),
      filtersOpen: false,
      dateFilter: DEFAULT_THREAD_DATE_FILTER,
      fromDate: "",
      toDate: "",
      setTab: (tab) => set({ tab }),
      setFiltersOpen: (open) => set((state) => ({ filtersOpen: typeof open === "function" ? open(state.filtersOpen) : open })),
      setDateFilter: (dateFilter) => set({ dateFilter }),
      setFromDate: (fromDate) => set({ fromDate }),
      setToDate: (toDate) => set({ toDate }),
      toggleFavorite: (tab) => set((state) => ({
        favorites: state.favorites.includes(tab)
          ? state.favorites.filter((id) => id !== tab)
          : [...state.favorites, tab],
      })),
    }),
    {
      name: "zorai-thread-rail-tabs",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: persistedThreadRailState,
    },
  ),
);
