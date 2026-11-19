import { create } from "zustand";
import { createStore } from "zustand/vanilla";
import {
  createJSONStorage,
  persist,
  type PersistOptions,
  type StateStorage,
} from "zustand/middleware";

export const THREAD_READ_STATE_STORE_NAME = "zorai-thread-read-state";
export const THREAD_READ_STATE_STORE_VERSION = 1;

export type ThreadReadIdentity = {
  id?: string | null;
  daemonThreadId?: string | null;
} | null | undefined;

export type ThreadReadState = {
  /**
   * Maps a stable read key to the timestamp of the newest message/activity
   * the operator has seen for that thread. Keys are `daemon:<daemonThreadId>`
   * once the thread is daemon-linked and `local:<localThreadId>` before
   * daemon linkage.
   */
  lastReadAtByThread: Record<string, number>;
};

export type ThreadReadActions = {
  /**
   * Mark a thread read at `timestamp`. Monotonic per key: an older timestamp
   * never overwrites a newer one, so a concurrently arriving completion is
   * not silently marked read.
   */
  markRead: (key: string, timestamp: number) => void;
  markUnread: (key: string) => void;
  /** Read timestamp for a key, or null when unknown/invalid. */
  lastReadAt: (key: string) => number | null;
  /**
   * Move a local read timestamp onto the daemon key once linkage appears,
   * keeping the maximum of both timestamps and removing the obsolete local
   * key. No-op when the source key is absent or either key is blank.
   */
  migrateThreadKey: (fromKey: string, toKey: string) => void;
  /**
   * Drop read entries whose key is not among `knownKeys`. Pruning touches
   * only this renderer-side map, never daemon threads or messages.
   */
  prune: (knownKeys: readonly string[]) => void;
  /** Replace persisted read state from an explicit hydrate source. */
  hydrate: (state: Partial<ThreadReadState>) => void;
};

export type ThreadReadStateStore = ThreadReadState & ThreadReadActions;

function trimToKey(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed || null;
}

function isValidReadTimestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

/**
 * Stable read key for a thread. Daemon identity is preferred so read state
 * survives local-id churn; before daemon linkage the local id is used as a
 * temporary key and later migrated via `migrateThreadKey`.
 */
export function threadReadKey(thread: ThreadReadIdentity): string | null {
  if (!thread) return null;
  const daemonId = trimToKey(thread.daemonThreadId);
  if (daemonId) return `daemon:${daemonId}`;
  const localId = trimToKey(thread.id);
  return localId ? `local:${localId}` : null;
}

/**
 * Normalize a persisted read-state map: trim keys, drop blank keys, keep only
 * finite non-negative timestamps, and ignore everything non-object.
 */
export function normalizeThreadReadState(input: unknown): ThreadReadState {
  const raw = (input ?? {}) as Partial<ThreadReadState>;
  const lastReadAtByThread: Record<string, number> = {};
  const rawMap = raw.lastReadAtByThread;
  if (rawMap && typeof rawMap === "object") {
    for (const [key, value] of Object.entries(rawMap)) {
      const trimmedKey = trimToKey(key);
      if (trimmedKey && isValidReadTimestamp(value)) {
        lastReadAtByThread[trimmedKey] = Math.max(
          lastReadAtByThread[trimmedKey] ?? Number.NEGATIVE_INFINITY,
          value,
        );
      }
    }
  }
  return { lastReadAtByThread };
}

/**
 * Explicit persist migration. Zustand invokes this only when the stored
 * version differs from `THREAD_READ_STATE_STORE_VERSION`; older and unknown
 * payloads are normalized into the current shape (unknown fields dropped,
 * keys trimmed, invalid timestamps ignored).
 */
export function migrateThreadReadState(persistedState: unknown, _version: number): ThreadReadState {
  return normalizeThreadReadState(persistedState);
}

function createThreadReadActions(
  set: (partial: Partial<ThreadReadState>) => void,
  get: () => ThreadReadStateStore,
): ThreadReadActions {
  return {
    markRead(key, timestamp) {
      const trimmedKey = trimToKey(key);
      if (!trimmedKey || !isValidReadTimestamp(timestamp)) return;
      const current = get().lastReadAtByThread[trimmedKey] ?? Number.NEGATIVE_INFINITY;
      if (timestamp <= current) return;
      set({
        lastReadAtByThread: {
          ...get().lastReadAtByThread,
          [trimmedKey]: timestamp,
        },
      });
    },

    markUnread(key) {
      const trimmedKey = trimToKey(key);
      if (!trimmedKey) return;
      set({ lastReadAtByThread: { ...get().lastReadAtByThread, [trimmedKey]: 0 } });
    },

    lastReadAt(key) {
      const trimmedKey = trimToKey(key);
      if (!trimmedKey) return null;
      return get().lastReadAtByThread[trimmedKey] ?? null;
    },

    migrateThreadKey(fromKey, toKey) {
      const trimmedFrom = trimToKey(fromKey);
      const trimmedTo = trimToKey(toKey);
      if (!trimmedFrom || !trimmedTo || trimmedFrom === trimmedTo) return;
      const { lastReadAtByThread } = get();
      const fromTimestamp = lastReadAtByThread[trimmedFrom];
      if (fromTimestamp === undefined) return;
      const next = { ...lastReadAtByThread };
      delete next[trimmedFrom];
      next[trimmedTo] = Math.max(next[trimmedTo] ?? Number.NEGATIVE_INFINITY, fromTimestamp);
      set({ lastReadAtByThread: next });
    },

    prune(knownKeys) {
      const known = new Set<string>();
      for (const key of knownKeys ?? []) {
        const trimmedKey = trimToKey(key);
        if (trimmedKey) known.add(trimmedKey);
      }
      const { lastReadAtByThread } = get();
      const next: Record<string, number> = {};
      let changed = false;
      for (const [key, value] of Object.entries(lastReadAtByThread)) {
        if (known.has(key)) {
          next[key] = value;
        } else {
          changed = true;
        }
      }
      if (changed) set({ lastReadAtByThread: next });
    },

    hydrate(state) {
      set(normalizeThreadReadState(state));
    },
  };
}

function createInitialThreadReadState(): ThreadReadState {
  return { lastReadAtByThread: {} };
}

/**
 * Single persist config builder shared by the vanilla factory and the exported
 * global hook so name/version/partialize/merge/migrate cannot drift apart.
 */
function createThreadReadPersistConfig(
  getStorage: () => StateStorage,
): PersistOptions<ThreadReadStateStore, ThreadReadState> {
  return {
    name: THREAD_READ_STATE_STORE_NAME,
    version: THREAD_READ_STATE_STORE_VERSION,
    storage: createJSONStorage<ThreadReadState>(getStorage),
    partialize: (state: ThreadReadStateStore): ThreadReadState => ({
      lastReadAtByThread: state.lastReadAtByThread,
    }),
    merge: (persisted: unknown, current: ThreadReadStateStore): ThreadReadStateStore => ({
      ...current,
      ...normalizeThreadReadState(persisted),
    }),
    migrate: migrateThreadReadState,
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
export function createThreadReadStateStore(storage?: StateStorage) {
  const resolvedStorage = storage ?? createRawMemoryStorage();
  return createStore<ThreadReadStateStore>()(
    persist(
      (set, get) => ({
        ...createInitialThreadReadState(),
        ...createThreadReadActions(set, get),
      }),
      createThreadReadPersistConfig(() => resolvedStorage),
    ),
  );
}

export const useThreadReadStateStore = create<ThreadReadStateStore>()(
  persist(
    (set, get) => ({
      ...createInitialThreadReadState(),
      ...createThreadReadActions(set, get),
    }),
    createThreadReadPersistConfig(() =>
      typeof localStorage !== "undefined" ? localStorage : createRawMemoryStorage(),
    ),
  ),
);
