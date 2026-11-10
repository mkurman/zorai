import { create } from "zustand";
import { createStore } from "zustand/vanilla";
import {
  createJSONStorage,
  persist,
  type PersistOptions,
  type StateStorage,
} from "zustand/middleware";

export const CODE_WORKSPACE_BINDING_STORE_NAME = "zorai-code-workspace-bindings";
export const CODE_WORKSPACE_BINDING_STORE_VERSION = 2;

/**
 * Upper bound on remembered project threads per root. The history menu is a
 * working set, not an archive; daemon threads stay reachable through the
 * Threads surface even after they fall off this list.
 */
export const CODE_PROJECT_THREADS_PER_ROOT_MAX = 40;
export const CODE_RECENT_ROOTS_MAX = 30;

export type CodeWorkspaceBindings = {
  /** Canonical validated workspace root that was most recently opened. */
  lastRoot: string | null;
  /**
   * Project thread ids per canonical workspace root, newest first. Ids may be
   * daemon thread ids or renderer-local ids for threads that have not been
   * promoted to daemon threads yet; the daemon id is recorded once assigned.
   */
  threadsByRoot: Record<string, string[]>;
  recentRoots: string[];
};

export type CodeWorkspaceBindingActions = {
  /** Record a thread as a project thread for a root (newest first, deduped). */
  recordProjectThread: (root: string, threadId: string) => void;
  /** Remove one thread from a root's project-thread list without touching others. */
  forgetProjectThread: (root: string, threadId: string) => void;
  /** Remove the project-thread list for a root without touching the threads. */
  removeRootBinding: (root: string) => void;
  /** Close a root: drop its project-thread list and clear lastRoot when it matches. */
  closeRoot: (root: string) => void;
  setLastRoot: (root: string | null) => void;
  /** Most recently recorded project thread id for a root, if any. */
  threadForRoot: (root: string) => string | null;
  /** Copy of the recorded project thread ids for a root, newest first. */
  projectThreadIdsForRoot: (root: string) => string[];
  /** Replace persisted bindings from an explicit hydrate source. */
  hydrate: (bindings: Partial<CodeWorkspaceBindings>) => void;
};

export type CodeWorkspaceBindingStore = CodeWorkspaceBindings & CodeWorkspaceBindingActions;

/**
 * Trim whitespace from a persisted root key. Renderer-side normalization is
 * intentionally trim + structure only: filesystem canonicalization
 * (`realpath`) is owned by the main-process `workspaceService.openWorkspace`,
 * which validates the directory and returns the canonical root. The renderer
 * must not pretend to filesystem-canonicalize.
 */
export function isCodeProjectThread(
  projectThreadIds: string[],
  thread: { id: string; daemonThreadId?: string | null } | undefined,
): boolean {
  if (!thread) return false;
  return projectThreadIds.includes(thread.id)
    || Boolean(thread.daemonThreadId && projectThreadIds.includes(thread.daemonThreadId));
}

export function trimCodeWorkspaceRoot(root: unknown): string | null {
  if (typeof root !== "string") return null;
  const trimmed = root.trim();
  return trimmed || null;
}

function normalizeThreadIds(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const ids: string[] = [];
  for (const entry of value) {
    if (typeof entry !== "string") continue;
    const id = entry.trim();
    if (!id || seen.has(id)) continue;
    seen.add(id);
    ids.push(id);
    if (ids.length >= CODE_PROJECT_THREADS_PER_ROOT_MAX) break;
  }
  return ids;
}

/**
 * Normalize any persisted or injected binding payload into the current shape.
 * Accepts the v2 `threadsByRoot` lists and folds the v1 single-thread
 * `threadByRoot` mapping into them so upgraded installs keep their canonical
 * project thread per root. Unknown fields are dropped, roots are trimmed,
 * malformed ids are ignored, duplicates collapse, and lists cap at
 * CODE_PROJECT_THREADS_PER_ROOT_MAX.
 */
export function normalizeCodeWorkspaceBindings(input: unknown): CodeWorkspaceBindings {
  const raw = (input ?? {}) as Partial<CodeWorkspaceBindings> & { threadByRoot?: unknown };
  const threadsByRoot: Record<string, string[]> = {};

  if (raw.threadsByRoot && typeof raw.threadsByRoot === "object") {
    for (const [root, ids] of Object.entries(raw.threadsByRoot)) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      const normalized = normalizeThreadIds(ids);
      if (trimmedRoot && normalized.length > 0) threadsByRoot[trimmedRoot] = normalized;
    }
  }

  if (raw.threadByRoot && typeof raw.threadByRoot === "object") {
    for (const [root, threadId] of Object.entries(raw.threadByRoot)) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot || typeof threadId !== "string" || !threadId.trim()) continue;
      const id = threadId.trim();
      const existing = threadsByRoot[trimmedRoot] ?? [];
      if (!existing.includes(id)) threadsByRoot[trimmedRoot] = [id, ...existing];
    }
  }

  const lastRoot = trimCodeWorkspaceRoot(raw.lastRoot);
  const recentSource = Array.isArray(raw.recentRoots) ? raw.recentRoots : [lastRoot, ...Object.keys(threadsByRoot)];
  const recentRoots: string[] = [];
  for (const entry of recentSource) {
    const root = trimCodeWorkspaceRoot(entry);
    if (root && !recentRoots.includes(root)) recentRoots.push(root);
    if (recentRoots.length >= CODE_RECENT_ROOTS_MAX) break;
  }

  return {
    lastRoot,
    threadsByRoot,
    recentRoots,
  };
}

/**
 * Explicit persist migration. Zustand invokes this when the stored version
 * differs from `CODE_WORKSPACE_BINDING_STORE_VERSION`; v1 payloads (single
 * `threadByRoot` mapping) and older/unknown payloads are normalized into the
 * current list shape. A future version bump folds its own transform in here.
 */
export function migrateCodeWorkspaceBindings(
  persistedState: unknown,
  _version: number,
): CodeWorkspaceBindings {
  return normalizeCodeWorkspaceBindings(persistedState);
}

function createCodeWorkspaceBindingActions(
  set: (partial: Partial<CodeWorkspaceBindings>) => void,
  get: () => CodeWorkspaceBindingStore,
): CodeWorkspaceBindingActions {
  return {
    recordProjectThread(root, threadId) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      const trimmedThreadId = typeof threadId === "string" ? threadId.trim() : "";
      if (!trimmedRoot || !trimmedThreadId) return;
      const { threadsByRoot } = get();
      const existing = threadsByRoot[trimmedRoot] ?? [];
      if (existing[0] === trimmedThreadId) return;
      const next = [
        trimmedThreadId,
        ...existing.filter((id) => id !== trimmedThreadId),
      ].slice(0, CODE_PROJECT_THREADS_PER_ROOT_MAX);
      set({
        threadsByRoot: {
          ...threadsByRoot,
          [trimmedRoot]: next,
        },
      });
    },

    forgetProjectThread(root, threadId) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      const trimmedThreadId = typeof threadId === "string" ? threadId.trim() : "";
      if (!trimmedRoot || !trimmedThreadId) return;
      const { threadsByRoot } = get();
      const existing = threadsByRoot[trimmedRoot];
      if (!existing || !existing.includes(trimmedThreadId)) return;
      const next = { ...threadsByRoot };
      const filtered = existing.filter((id) => id !== trimmedThreadId);
      if (filtered.length > 0) {
        next[trimmedRoot] = filtered;
      } else {
        delete next[trimmedRoot];
      }
      set({ threadsByRoot: next });
    },

    removeRootBinding(root) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot) return;
      const { threadsByRoot } = get();
      if (!(trimmedRoot in threadsByRoot)) return;
      const next = { ...threadsByRoot };
      delete next[trimmedRoot];
      set({ threadsByRoot: next });
    },

    closeRoot(root) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot) return;
      const { threadsByRoot, lastRoot, recentRoots } = get();
      const next = { ...threadsByRoot };
      delete next[trimmedRoot];
      set({
        threadsByRoot: next,
        lastRoot: lastRoot === trimmedRoot ? null : lastRoot,
        recentRoots: recentRoots.filter((entry) => entry !== trimmedRoot),
      });
    },

    setLastRoot(root) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot) {
        set({ lastRoot: null });
        return;
      }
      const recentRoots = [trimmedRoot, ...get().recentRoots.filter((entry) => entry !== trimmedRoot)].slice(0, CODE_RECENT_ROOTS_MAX);
      set({ lastRoot: trimmedRoot, recentRoots });
    },

    threadForRoot(root) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot) return null;
      return get().threadsByRoot[trimmedRoot]?.[0] ?? null;
    },

    projectThreadIdsForRoot(root) {
      const trimmedRoot = trimCodeWorkspaceRoot(root);
      if (!trimmedRoot) return [];
      return [...(get().threadsByRoot[trimmedRoot] ?? [])];
    },

    hydrate(bindings) {
      const normalized = normalizeCodeWorkspaceBindings(bindings);
      set({
        lastRoot: normalized.lastRoot,
        threadsByRoot: normalized.threadsByRoot,
        recentRoots: normalized.recentRoots,
      });
    },
  };
}

function createInitialCodeWorkspaceBindings(): CodeWorkspaceBindings {
  return { lastRoot: null, threadsByRoot: {}, recentRoots: [] };
}

/**
 * Single persist config builder shared by the vanilla factory and the exported
 * global hook so name/version/partialize/merge/migrate cannot drift apart.
 */
function createCodeWorkspaceBindingPersistConfig(
  getStorage: () => StateStorage,
): PersistOptions<CodeWorkspaceBindingStore, CodeWorkspaceBindings> {
  return {
    name: CODE_WORKSPACE_BINDING_STORE_NAME,
    version: CODE_WORKSPACE_BINDING_STORE_VERSION,
    storage: createJSONStorage<CodeWorkspaceBindings>(getStorage),
    partialize: (state: CodeWorkspaceBindingStore): CodeWorkspaceBindings => ({
      lastRoot: state.lastRoot,
      threadsByRoot: state.threadsByRoot,
      recentRoots: state.recentRoots,
    }),
    merge: (persisted: unknown, current: CodeWorkspaceBindingStore): CodeWorkspaceBindingStore => ({
      ...current,
      ...normalizeCodeWorkspaceBindings(persisted),
    }),
    migrate: migrateCodeWorkspaceBindings,
  };
}

function createRawMemoryStorage(): StateStorage {
  const values = new Map<string, string>();
  return {
    getItem: (name) => values.get(name) ?? null,
    setItem: (name, value) => {
      values.set(name, value);
    },
    removeItem: (name) => {
      values.delete(name);
    },
  };
}

/**
 * Vanilla store factory used by tests and explicit hydration flows. Without an
 * explicit storage it always uses a fresh isolated in-memory storage so the
 * factory never writes renderer-localStorage; only the exported global hook
 * below persists through localStorage.
 */
export function createCodeWorkspaceBindingStore(storage?: StateStorage) {
  const resolvedStorage = storage ?? createRawMemoryStorage();
  return createStore<CodeWorkspaceBindingStore>()(
    persist(
      (set, get) => ({
        ...createInitialCodeWorkspaceBindings(),
        ...createCodeWorkspaceBindingActions(set, get),
      }),
      createCodeWorkspaceBindingPersistConfig(() => resolvedStorage),
    ),
  );
}

export const useCodeWorkspaceBindingStore = create<CodeWorkspaceBindingStore>()(
  persist(
    (set, get) => ({
      ...createInitialCodeWorkspaceBindings(),
      ...createCodeWorkspaceBindingActions(set, get),
    }),
    createCodeWorkspaceBindingPersistConfig(() =>
      typeof localStorage !== "undefined" ? localStorage : createRawMemoryStorage(),
    ),
  ),
);
