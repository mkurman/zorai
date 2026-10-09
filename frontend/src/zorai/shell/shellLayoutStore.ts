import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export const SHELL_RAIL_DEFAULT_WIDTH = 284;
export const SHELL_RAIL_MIN_WIDTH = 180;
export const SHELL_CONTEXT_DEFAULT_WIDTH = 318;
export const SHELL_CONTEXT_MIN_WIDTH = 260;

type ShellLayoutStore = {
  railWidths: Record<string, number>;
  contextWidths: Record<string, number>;
  setRailWidth: (view: string, width: number) => void;
  setContextWidth: (view: string, width: number) => void;
  resetRailWidth: (view: string) => void;
  resetContextWidth: (view: string) => void;
};

function without(widths: Record<string, number>, view: string): Record<string, number> {
  const { [view]: _removed, ...rest } = widths;
  return rest;
}

export const useShellLayoutStore = create<ShellLayoutStore>()(
  persist(
    (set) => ({
      railWidths: {},
      contextWidths: {},
      setRailWidth: (view, width) => set((state) => ({
        railWidths: { ...state.railWidths, [view]: Math.max(SHELL_RAIL_MIN_WIDTH, Math.round(width)) },
      })),
      setContextWidth: (view, width) => set((state) => ({
        contextWidths: { ...state.contextWidths, [view]: Math.max(SHELL_CONTEXT_MIN_WIDTH, Math.round(width)) },
      })),
      resetRailWidth: (view) => set((state) => ({ railWidths: without(state.railWidths, view) })),
      resetContextWidth: (view) => set((state) => ({ contextWidths: without(state.contextWidths, view) })),
    }),
    {
      name: "zorai-shell-layout",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ railWidths: state.railWidths, contextWidths: state.contextWidths }),
    },
  ),
);
