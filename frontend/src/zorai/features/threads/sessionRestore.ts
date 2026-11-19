import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { useOpenSessionTabs } from "./openSessionTabs";
import { mapSplitPaneIds, splitOrNull, useThreadSplitStore, type ThreadSplitNode } from "./threadSplitModel";

export const LAST_SESSION_STORAGE_KEY = "zorai-last-session";

export type SavedSession = { tabs: string[]; active: string | null; split?: ThreadSplitNode };

type SessionStorage = Pick<Storage, "getItem" | "setItem">;

function daemonIdFor(threads: readonly AgentThread[], id: string | null): string | null {
  if (!id) return null;
  const thread = threads.find((item) => item.id === id || item.daemonThreadId === id);
  return thread?.daemonThreadId?.trim() || null;
}

export function savedSessionFor(threads: readonly AgentThread[], tabIds: readonly string[], activeId: string | null, split: ThreadSplitNode | null = null): SavedSession {
  const tabs: string[] = [];
  for (const id of tabIds) {
    const daemonId = daemonIdFor(threads, id);
    if (daemonId && !tabs.includes(daemonId)) tabs.push(daemonId);
  }
  const savedSplit = splitOrNull(mapSplitPaneIds(split, (id) => daemonIdFor(threads, id)));
  return { tabs, active: daemonIdFor(threads, activeId), split: savedSplit ?? undefined };
}

function isSplitNode(value: unknown): value is ThreadSplitNode {
  if (!value || typeof value !== "object") return false;
  const node = value as Partial<{ kind: unknown; id: unknown; direction: unknown; children: unknown }>;
  if (node.kind === "pane") return typeof node.id === "string" && node.id.trim() !== "";
  return node.kind === "split"
    && (node.direction === "row" || node.direction === "column")
    && Array.isArray(node.children)
    && node.children.every(isSplitNode);
}

export function restoredSplitFor(threads: readonly AgentThread[], split: ThreadSplitNode | undefined): ThreadSplitNode | null {
  return splitOrNull(mapSplitPaneIds(split ?? null, (daemonId) => threads.find((thread) => thread.daemonThreadId === daemonId)?.id ?? null));
}

export function readSavedSession(storage: SessionStorage | undefined): SavedSession {
  try {
    const parsed = JSON.parse(storage?.getItem(LAST_SESSION_STORAGE_KEY) ?? "null") as Partial<SavedSession> | null;
    return {
      tabs: Array.isArray(parsed?.tabs) ? parsed.tabs.filter((id): id is string => typeof id === "string" && id.trim() !== "") : [],
      active: typeof parsed?.active === "string" && parsed.active.trim() ? parsed.active : null,
      split: isSplitNode(parsed?.split) ? parsed.split : undefined,
    };
  } catch {
    return { tabs: [], active: null };
  }
}

let threadsViewThreadId: string | null = null;
let restoring = true;
let writeSession: (() => void) | null = null;

export function rememberThreadsViewThread(id: string | null): void {
  if (restoring) return;
  threadsViewThreadId = id;
  writeSession?.();
}

export function finishSessionRestore(id: string | null): void {
  restoring = false;
  rememberThreadsViewThread(id);
}

export function restoreSavedLayout(saved: SavedSession): void {
  const threads = useAgentStore.getState().threads;
  const known = saved.tabs.filter((id) => threads.some((thread) => thread.daemonThreadId === id));
  if (known.length > 0 && useOpenSessionTabs.getState().ids.length === 0) useOpenSessionTabs.setState({ ids: known });
  const split = restoredSplitFor(threads, saved.split);
  if (split && !useThreadSplitStore.getState().root) useThreadSplitStore.setState({ root: split });
}

export function whenThreadsLoaded(callback: () => void, timeoutMs = 20_000): void {
  if (useAgentStore.getState().threads.length > 0) {
    callback();
    return;
  }
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    unsubscribe();
    clearTimeout(timer);
    callback();
  };
  const unsubscribe = useAgentStore.subscribe((state) => {
    if (state.threads.length > 0) finish();
  });
  const timer = setTimeout(finish, timeoutMs);
}

export function startSessionPersistence(storage: SessionStorage | undefined = typeof localStorage === "undefined" ? undefined : localStorage): SavedSession {
  const saved = readSavedSession(storage);
  if (!storage || writeSession) return saved;
  let last = JSON.stringify(saved);
  writeSession = () => {
    if (restoring || useAgentStore.getState().threads.length === 0) return;
    const next = JSON.stringify(savedSessionFor(useAgentStore.getState().threads, useOpenSessionTabs.getState().ids, threadsViewThreadId, useThreadSplitStore.getState().root));
    if (next === last) return;
    last = next;
    storage.setItem(LAST_SESSION_STORAGE_KEY, next);
  };
  useOpenSessionTabs.subscribe(() => writeSession?.());
  useThreadSplitStore.subscribe((state, previous) => {
    if (state.root !== previous.root) writeSession?.();
  });
  useAgentStore.subscribe((state, previous) => {
    if (state.threads !== previous.threads) writeSession?.();
  });
  return saved;
}
