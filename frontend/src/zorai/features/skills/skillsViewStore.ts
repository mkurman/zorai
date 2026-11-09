import { create } from "zustand";

export type SkillEditorMode = "preview" | "editor";

export type OpenSkillDocument = {
  path: string;
  mode: SkillEditorMode;
  draft: boolean;
};

export type SkillSyncResult = { ok: boolean; output: string };

type SkillsViewState = {
  selectedDir: string;
  open: OpenSkillDocument | null;
  refreshToken: number;
  syncing: boolean;
  syncResult: SkillSyncResult | null;
  selectDir: (dir: string) => void;
  openDocument: (path: string, mode: SkillEditorMode) => void;
  openDraft: (path: string) => void;
  markDraftSaved: (path: string) => void;
  setMode: (mode: SkillEditorMode) => void;
  close: () => void;
  refresh: () => void;
  setSyncing: (syncing: boolean) => void;
  setSyncResult: (result: SkillSyncResult | null) => void;
};

export const useSkillsViewStore = create<SkillsViewState>((set) => ({
  selectedDir: "",
  open: null,
  refreshToken: 0,
  syncing: false,
  syncResult: null,
  selectDir: (dir) => set({ selectedDir: dir }),
  openDocument: (path, mode) => set({ open: { path, mode, draft: false } }),
  openDraft: (path) => set({ open: { path, mode: "editor", draft: true } }),
  markDraftSaved: (path) =>
    set((state) => (state.open?.path === path ? { open: { ...state.open, draft: false } } : {})),
  setMode: (mode) => set((state) => (state.open ? { open: { ...state.open, mode } } : {})),
  close: () => set({ open: null }),
  refresh: () => set((state) => ({ refreshToken: state.refreshToken + 1 })),
  setSyncing: (syncing) => set({ syncing }),
  setSyncResult: (syncResult) => set({ syncResult }),
}));

export function parentDir(path: string): string {
  const parts = path.split("/");
  parts.pop();
  return parts.join("/");
}

export function joinSkillPath(dir: string, name: string): string {
  const cleanName = name.trim().replace(/^\/+|\/+$/g, "");
  return dir ? `${dir}/${cleanName}` : cleanName;
}

export function isValidSkillName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed !== "" && trimmed !== "." && trimmed !== ".." && !/[\\/]/.test(trimmed);
}
