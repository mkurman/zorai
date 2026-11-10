import { create } from "zustand";
import { useAgentStore, type AgentThread } from "./agentStore";
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
  adoptRoot: (threadId: string, root: string) => void;
  clearRoot: (threadId: string) => void;
  setActiveFile: (threadId: string, filePath: string | null) => void;
  setSelection: (threadId: string, selection: WorkspaceSelection | null) => void;
  toggleAttachedFile: (threadId: string, filePath: string) => void;
  closeFile: (threadId: string, filePath: string) => void;
  togglePinnedFile: (threadId: string, filePath: string) => void;
  moveOpenFile: (threadId: string, filePath: string, targetPath: string, placeAfter: boolean) => void;
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

let unmatchedByDaemonThreadId: Record<string, ThreadWorkspaceContext> = {};
let lastPersisted = "";
let unsubscribeThreads: (() => void) | null = null;

function daemonThreadIdsByThreadId(threads: AgentThread[]): Map<string, string> {
  const ids = new Map<string, string>();
  for (const thread of threads) {
    const daemonThreadId = thread.daemonThreadId?.trim();
    if (daemonThreadId) ids.set(thread.id, daemonThreadId);
  }
  return ids;
}

function contextsByDaemonThreadId(byThreadId: Record<string, ThreadWorkspaceContext>) {
  const daemonThreadIds = daemonThreadIdsByThreadId(useAgentStore.getState().threads);
  const byDaemonThreadId = { ...unmatchedByDaemonThreadId };
  for (const [threadId, context] of Object.entries(byThreadId)) {
    const daemonThreadId = daemonThreadIds.get(threadId);
    if (daemonThreadId) byDaemonThreadId[daemonThreadId] = context;
  }
  return byDaemonThreadId;
}

function persist(byThreadId: Record<string, ThreadWorkspaceContext>) {
  const byDaemonThreadId = contextsByDaemonThreadId(byThreadId);
  lastPersisted = JSON.stringify(byDaemonThreadId);
  scheduleJsonWrite(WORKSPACE_CONTEXT_FILE, { byDaemonThreadId }, 150);
}

function attachSavedContexts(threads: AgentThread[]) {
  const attached: Record<string, ThreadWorkspaceContext> = {};
  for (const [threadId, daemonThreadId] of daemonThreadIdsByThreadId(threads)) {
    const saved = unmatchedByDaemonThreadId[daemonThreadId];
    if (!saved) continue;
    attached[threadId] = saved;
    const { [daemonThreadId]: _matched, ...rest } = unmatchedByDaemonThreadId;
    unmatchedByDaemonThreadId = rest;
  }
  if (Object.keys(attached).length === 0) return;
  useWorkspaceContextStore.setState((state) => {
    const byThreadId = { ...state.byThreadId };
    for (const [threadId, saved] of Object.entries(attached)) {
      const current = byThreadId[threadId];
      byThreadId[threadId] = current && current.updatedAt > saved.updatedAt ? current : saved;
    }
    return { byThreadId };
  });
}

function syncWithThreads(threads: AgentThread[]) {
  attachSavedContexts(threads);
  const { byThreadId } = useWorkspaceContextStore.getState();
  if (JSON.stringify(contextsByDaemonThreadId(byThreadId)) !== lastPersisted) persist(byThreadId);
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
    const saved = await readPersistedJson<{ byDaemonThreadId?: Record<string, ThreadWorkspaceContext> }>(WORKSPACE_CONTEXT_FILE);
    unmatchedByDaemonThreadId = Object.fromEntries(Object.entries(saved?.byDaemonThreadId ?? {}).map(([daemonThreadId, context]) => [daemonThreadId, {
      ...context,
      isolateAgentTasks: context.isolateAgentTasks ?? false,
      isolatedWorktreeStates: context.isolatedWorktreeStates ?? {},
      pinnedFiles: context.pinnedFiles ?? [],
    }]));
    lastPersisted = JSON.stringify(contextsByDaemonThreadId(get().byThreadId));
    set({ hydrated: true });
    syncWithThreads(useAgentStore.getState().threads);
    unsubscribeThreads ??= useAgentStore.subscribe((state, previous) => {
      if (state.threads !== previous.threads) syncWithThreads(state.threads);
    });
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
  adoptRoot: (threadId, root) => set((state) => {
    if (state.byThreadId[threadId]) return state;
    const template = Object.values(state.byThreadId)
      .filter((context) => context.root === root)
      .sort((left, right) => right.updatedAt - left.updatedAt)[0];
    const byThreadId = {
      ...state.byThreadId,
      [threadId]: {
        root,
        activeFile: template?.activeFile ?? null,
        selection: null,
        attachedFiles: [],
        openFiles: template?.openFiles ?? [],
        updatedAt: 0,
        isolateAgentTasks: template?.isolateAgentTasks ?? true,
        isolatedWorktreeStates: {},
        pinnedFiles: template?.pinnedFiles ?? [],
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
  moveOpenFile: (threadId, filePath, targetPath, placeAfter) => set((state) => updateContext(state, threadId, (current) => {
    if (filePath === targetPath || !current.openFiles.includes(filePath) || !current.openFiles.includes(targetPath)) return current;
    const rest = current.openFiles.filter((entry) => entry !== filePath);
    const targetIndex = rest.indexOf(targetPath) + (placeAfter ? 1 : 0);
    const openFiles = [...rest.slice(0, targetIndex), filePath, ...rest.slice(targetIndex)];
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
