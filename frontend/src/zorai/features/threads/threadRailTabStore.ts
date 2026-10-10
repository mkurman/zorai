import { create } from "zustand";
import type { ThreadFilterTab } from "./threadFilterModel";

type ThreadRailTabState = {
  tab: ThreadFilterTab;
  setTab: (tab: ThreadFilterTab) => void;
};

export const useThreadRailTabStore = create<ThreadRailTabState>((set) => ({
  tab: "svarog",
  setTab: (tab) => set({ tab }),
}));
