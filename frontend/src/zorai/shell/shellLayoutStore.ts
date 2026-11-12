import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export const SHELL_RAIL_DEFAULT_WIDTH = 284;
export const SHELL_RAIL_MIN_WIDTH = 180;
export const SHELL_CONTEXT_DEFAULT_WIDTH = 318;
export const SHELL_CONTEXT_MIN_WIDTH = 260;

type OpenUpdate = boolean | ((current: boolean) => boolean);

type ShellLayoutStore = {
  railOpen: boolean;
  contextOpen: boolean;
  codeContextAutoOpened: boolean;
  codeContextOpen: boolean | null;
  otherContextOpen: boolean | null;
  lastWorkView: string | null;
  setRailOpen: (open: OpenUpdate) => void;
  setContextOpen: (open: OpenUpdate) => void;
  markCodeContextAutoOpened: () => void;
  crossCodeBoundary: (enteringCode: boolean) => void;
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
      railOpen: true,
      contextOpen: false,
      codeContextAutoOpened: false,
      codeContextOpen: null,
      otherContextOpen: null,
      lastWorkView: null,
      setRailOpen: (open) => set((state) => ({ railOpen: typeof open === "function" ? open(state.railOpen) : open })),
      setContextOpen: (open) => set((state) => ({ contextOpen: typeof open === "function" ? open(state.contextOpen) : open })),
      markCodeContextAutoOpened: () => set({ codeContextAutoOpened: true }),
      crossCodeBoundary: (enteringCode) => set((state) => enteringCode
        ? { otherContextOpen: state.contextOpen, contextOpen: state.codeContextOpen ?? state.contextOpen }
        : { codeContextOpen: state.contextOpen, contextOpen: state.otherContextOpen ?? false }),
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
