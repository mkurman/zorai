import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { fixedThreadTabs, type ThreadFilterTab } from "./threadFilterModel";

type ThreadRailTabState = {
  tab: ThreadFilterTab;
  favorites: ThreadFilterTab[];
  setTab: (tab: ThreadFilterTab) => void;
  toggleFavorite: (tab: ThreadFilterTab) => void;
};

export const useThreadRailTabStore = create<ThreadRailTabState>()(
  persist(
    (set) => ({
      tab: "svarog",
      favorites: fixedThreadTabs.map((item) => item.id),
      setTab: (tab) => set({ tab }),
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
      partialize: (state) => ({ favorites: state.favorites }),
    },
  ),
);
