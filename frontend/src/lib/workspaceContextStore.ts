import { create } from "zustand";
import { readPersistedJson, scheduleJsonWrite } from "./persistence";

const WORKSPACE_CONTEXT_FILE = "thread-workspace-context.json";

export type WorkspaceSelection = {
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
};

export type ThreadWorkspaceContext = {
  root: string;
  activeFile: string | null;
  selection: WorkspaceSelection | null;
  attachedFiles: string[];
  openFiles: string[];
  updatedAt: number;
  isolateAgentTasks: boolean;
  isolatedWorktreeStates: Record<string, "awaiting_review" | "retained" | "integrated" | "rejected">;
  pinnedFiles: string[];
};

type WorkspaceContextState = {
  hydrated: boolean;
  byThreadId: Record<string, ThreadWorkspaceContext>;
  hydrate: () => Promise<void>;
  bindRoot: (threadId: string, root: string) => void;
  clearRoot: (threadId: string) => void;
  setActiveFile: (threadId: string, filePath: string | null) => void;
  setSelection: (threadId: string, selection: WorkspaceSelection | null) => void;
  toggleAttachedFile: (threadId: string, filePath: string) => void;
  closeFile: (threadId: string, filePath: string) => void;
  togglePinnedFile: (threadId: string, filePath: string) => void;
  moveOpenFile: (threadId: string, filePath: string, direction: -1 | 1) => void;
  setIsolateAgentTasks: (threadId: string, enabled: boolean) => void;
  setIsolatedWorktreeState: (threadId: string, worktreePath: string, state: "awaiting_review" | "retained" | "integrated" | "rejected") => void;
};

export function toDaemonWorkspaceContext(context: ThreadWorkspaceContext) {
  return {
    root: context.root,
    active_file: context.activeFile,
    selection: context.selection ? {
      start_line: context.selection.startLine,
      start_column: context.selection.startColumn,
      end_line: context.selection.endLine,
      end_column: context.selection.endColumn,
    } : null,
    attached_files: context.attachedFiles,
    open_files: context.openFiles,
    updated_at: context.updatedAt,
    isolate_agent_tasks: context.isolateAgentTasks,
    isolated_worktree_states: context.isolatedWorktreeStates,
  };
}

function persist(byThreadId: Record<string, ThreadWorkspaceContext>) {
  scheduleJsonWrite(WORKSPACE_CONTEXT_FILE, { byThreadId }, 150);
}

function updateContext(
  state: WorkspaceContextState,
  threadId: string,
  updater: (current: ThreadWorkspaceContext) => ThreadWorkspaceContext,
) {
  const current = state.byThreadId[threadId];
  if (!current) return state;
  const byThreadId = { ...state.byThreadId, [threadId]: updater(current) };
  persist(byThreadId);
  return { ...state, byThreadId };
}

export const useWorkspaceContextStore = create<WorkspaceContextState>((set, get) => ({
  hydrated: false,
  byThreadId: {},
  hydrate: async () => {
    if (get().hydrated) return;
    const saved = await readPersistedJson<{ byThreadId?: Record<string, ThreadWorkspaceContext> }>(WORKSPACE_CONTEXT_FILE);
    const byThreadId = Object.fromEntries(Object.entries(saved?.byThreadId ?? {}).map(([threadId, context]) => [threadId, {
      ...context,
      isolateAgentTasks: context.isolateAgentTasks ?? false,
      isolatedWorktreeStates: context.isolatedWorktreeStates ?? {},
      pinnedFiles: context.pinnedFiles ?? [],
    }]));
    set({ hydrated: true, byThreadId });
  },
  bindRoot: (threadId, root) => set((state) => {
    const previous = state.byThreadId[threadId];
    const byThreadId = {
      ...state.byThreadId,
      [threadId]: previous?.root === root
        ? previous
        : {
          root,
          activeFile: null,
          selection: null,
          attachedFiles: [],
          openFiles: [],
          updatedAt: Date.now(),
          isolateAgentTasks: true,
          isolatedWorktreeStates: {},
          pinnedFiles: [],
        },
    };
    persist(byThreadId);
    return { byThreadId };
  }),
  clearRoot: (threadId) => set((state) => {
    if (!state.byThreadId[threadId]) return state;
    const { [threadId]: _removed, ...byThreadId } = state.byThreadId;
    persist(byThreadId);
    return { byThreadId };
  }),
  setActiveFile: (threadId, filePath) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    activeFile: filePath,
    openFiles: filePath && !current.openFiles.includes(filePath) ? [...current.openFiles, filePath] : current.openFiles,
    updatedAt: Date.now(),
  }))),
  setSelection: (threadId, selection) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    selection,
    updatedAt: Date.now(),
  }))),
  toggleAttachedFile: (threadId, filePath) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    attachedFiles: current.attachedFiles.includes(filePath)
      ? current.attachedFiles.filter((entry) => entry !== filePath)
      : [...current.attachedFiles, filePath],
    updatedAt: Date.now(),
  }))),
  togglePinnedFile: (threadId, filePath) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    pinnedFiles: current.pinnedFiles.includes(filePath)
      ? current.pinnedFiles.filter((entry) => entry !== filePath)
      : [...current.pinnedFiles, filePath],
    updatedAt: Date.now(),
  }))),
  moveOpenFile: (threadId, filePath, direction) => set((state) => updateContext(state, threadId, (current) => {
    const index = current.openFiles.indexOf(filePath);
    if (index < 0) return current;
    const target = Math.max(0, Math.min(current.openFiles.length - 1, index + direction));
    if (target === index) return current;
    const openFiles = [...current.openFiles];
    [openFiles[index], openFiles[target]] = [openFiles[target], openFiles[index]];
    return { ...current, openFiles, updatedAt: Date.now() };
  })),
  setIsolatedWorktreeState: (threadId, worktreePath, lifecycle) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    isolatedWorktreeStates: { ...current.isolatedWorktreeStates, [worktreePath]: lifecycle },
    updatedAt: Date.now(),
  }))),
  setIsolateAgentTasks: (threadId, enabled) => set((state) => updateContext(state, threadId, (current) => ({
    ...current,
    isolateAgentTasks: enabled,
    updatedAt: Date.now(),
  }))),
  closeFile: (threadId, filePath) => set((state) => updateContext(state, threadId, (current) => {
    if (current.pinnedFiles.includes(filePath)) return current;
    const openFiles = current.openFiles.filter((entry) => entry !== filePath);
    return {
      ...current,
      openFiles,
      activeFile: current.activeFile === filePath ? openFiles[openFiles.length - 1] ?? null : current.activeFile,
      selection: current.activeFile === filePath ? null : current.selection,
      pinnedFiles: current.pinnedFiles.filter((entry) => entry !== filePath),
      updatedAt: Date.now(),
    };
  })),
}));
