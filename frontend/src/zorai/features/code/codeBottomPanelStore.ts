import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type CodeBottomPanelTab = "output" | "terminal";

export const CODE_PANEL_MIN_HEIGHT = 120;
export const CODE_PANEL_DEFAULT_HEIGHT = 260;

type CodeBottomPanelState = {
  open: boolean;
  tab: CodeBottomPanelTab;
  height: number;
  toggle: (tab?: CodeBottomPanelTab) => void;
  show: (tab: CodeBottomPanelTab) => void;
  close: () => void;
  setHeight: (height: number) => void;
};

export function nextCodePanelState(state: { open: boolean; tab: CodeBottomPanelTab }, tab?: CodeBottomPanelTab): { open: boolean; tab: CodeBottomPanelTab } {
  if (!tab) return { open: !state.open, tab: state.tab };
  if (state.open && state.tab === tab) return { open: false, tab };
  return { open: true, tab };
}

export function clampCodePanelHeight(height: number, available: number): number {
  const max = Math.max(CODE_PANEL_MIN_HEIGHT, available - CODE_PANEL_MIN_HEIGHT);
  return Math.round(Math.min(max, Math.max(CODE_PANEL_MIN_HEIGHT, height)));
}

export const useCodeBottomPanelStore = create<CodeBottomPanelState>()(persist(
  (set) => ({
    open: false,
    tab: "terminal",
    height: CODE_PANEL_DEFAULT_HEIGHT,
    toggle: (tab) => set((state) => nextCodePanelState(state, tab)),
    show: (tab) => set({ open: true, tab }),
    close: () => set({ open: false }),
    setHeight: (height) => set({ height }),
  }),
  {
    name: "zorai-code-bottom-panel",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ open: state.open, tab: state.tab, height: state.height }),
    merge: (persisted, current) => {
      const saved = (persisted ?? {}) as Partial<CodeBottomPanelState>;
      return { ...current, ...saved, tab: saved.tab === "output" ? "output" : "terminal" };
    },
  },
));
