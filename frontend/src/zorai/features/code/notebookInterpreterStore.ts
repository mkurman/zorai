import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

type NotebookInterpreterState = {
  byRoot: Record<string, string>;
  setInterpreter: (root: string, python: string | null) => void;
};

export const useNotebookInterpreterStore = create<NotebookInterpreterState>()(persist(
  (set) => ({
    byRoot: {},
    setInterpreter: (root, python) => set((state) => {
      const byRoot = { ...state.byRoot };
      if (python) byRoot[root] = python;
      else delete byRoot[root];
      return { byRoot };
    }),
  }),
  {
    name: "zorai-notebook-interpreters",
    storage: createJSONStorage(() => (typeof localStorage !== "undefined" ? localStorage : { getItem: () => null, setItem: () => undefined, removeItem: () => undefined })),
    partialize: (state) => ({ byRoot: state.byRoot }),
  },
));
