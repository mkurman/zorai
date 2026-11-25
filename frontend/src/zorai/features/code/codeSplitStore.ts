import { create } from "zustand";
import {
  insertSplitPane,
  insertSplitPaneAtEdge,
  removeSplitPaneNode,
  replaceSplitPaneNode,
  setSplitSizesAt,
  splitLayoutTree,
  splitOrNull,
  splitPaneIds,
  type ThreadSplitDropZone,
  type ThreadSplitLayout,
  type ThreadSplitNode,
} from "@/zorai/features/threads/threadSplitModel";

export const CODE_FILE_DRAG_TYPE = "application/x-zorai-code-file";

export function startCodeFileDrag(event: { dataTransfer: DataTransfer }, path: string): void {
  event.dataTransfer.setData(CODE_FILE_DRAG_TYPE, path);
  event.dataTransfer.setData("text/plain", path);
  event.dataTransfer.effectAllowed = "copy";
}

type CodeSplitState = {
  root: ThreadSplitNode | null;
  setLayout: (paths: string[], layout: ThreadSplitLayout) => void;
  insert: (targetPath: string, path: string, zone: ThreadSplitDropZone) => void;
  insertAtEdge: (path: string, zone: ThreadSplitDropZone) => void;
  replacePane: (path: string, nextPath: string) => void;
  setSizes: (treePath: number[], sizes: number[]) => void;
  removePane: (path: string) => void;
  clear: () => void;
};

export const useCodeSplitStore = create<CodeSplitState>((set) => ({
  root: null,
  setLayout: (paths, layout) => set({ root: splitLayoutTree(layout, paths) }),
  insert: (targetPath, path, zone) => set((state) => ({ root: splitOrNull(insertSplitPane(state.root, targetPath, path, zone)) })),
  insertAtEdge: (path, zone) => set((state) => ({ root: splitOrNull(insertSplitPaneAtEdge(state.root, path, zone)) })),
  replacePane: (path, nextPath) => set((state) => {
    const ids = splitPaneIds(state.root);
    if (!state.root || !ids.includes(path) || ids.includes(nextPath)) return state;
    return { root: replaceSplitPaneNode(state.root, path, nextPath) };
  }),
  setSizes: (treePath, sizes) => set((state) => ({ root: setSplitSizesAt(state.root, treePath, sizes) })),
  removePane: (path) => set((state) => ({ root: splitOrNull(removeSplitPaneNode(state.root, path)) })),
  clear: () => set({ root: null }),
}));
