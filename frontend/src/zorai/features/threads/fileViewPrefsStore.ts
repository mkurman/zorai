import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type FileViewMode = "list" | "tree";

type FileViewPrefsState = {
  mode: FileViewMode;
  setMode: (mode: FileViewMode) => void;
};

export const useFileViewPrefsStore = create<FileViewPrefsState>()(
  persist(
    (set) => ({
      mode: "list",
      setMode: (mode) => set({ mode }),
    }),
    {
      name: "zorai-file-view-mode",
      version: 1,
      storage: createJSONStorage(() => localStorage),
    },
  ),
);
