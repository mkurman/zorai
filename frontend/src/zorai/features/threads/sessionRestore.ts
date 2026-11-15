import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { useOpenSessionTabs } from "./openSessionTabs";

export const LAST_SESSION_STORAGE_KEY = "zorai-last-session";

export type SavedSession = { tabs: string[]; active: string | null };

type SessionStorage = Pick<Storage, "getItem" | "setItem">;

function daemonIdFor(threads: readonly AgentThread[], id: string | null): string | null {
  if (!id) return null;
  const thread = threads.find((item) => item.id === id || item.daemonThreadId === id);
  return thread?.daemonThreadId?.trim() || null;
}

export function savedSessionFor(threads: readonly AgentThread[], tabIds: readonly string[], activeId: string | null): SavedSession {
  const tabs: string[] = [];
  for (const id of tabIds) {
    const daemonId = daemonIdFor(threads, id);
    if (daemonId && !tabs.includes(daemonId)) tabs.push(daemonId);
  }
  return { tabs, active: daemonIdFor(threads, activeId) };
}

export function readSavedSession(storage: SessionStorage | undefined): SavedSession {
  try {
    const parsed = JSON.parse(storage?.getItem(LAST_SESSION_STORAGE_KEY) ?? "null") as Partial<SavedSession> | null;
    return {
      tabs: Array.isArray(parsed?.tabs) ? parsed.tabs.filter((id): id is string => typeof id === "string" && id.trim() !== "") : [],
      active: typeof parsed?.active === "string" && parsed.active.trim() ? parsed.active : null,
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

export function startSessionPersistence(storage: SessionStorage | undefined = typeof localStorage === "undefined" ? undefined : localStorage): SavedSession {
  const saved = readSavedSession(storage);
  if (!storage || writeSession) return saved;
  const threads = useAgentStore.getState().threads;
  const known = saved.tabs.filter((id) => threads.some((thread) => thread.daemonThreadId === id));
  if (known.length > 0 && useOpenSessionTabs.getState().ids.length === 0) useOpenSessionTabs.setState({ ids: known });
  let last = JSON.stringify(saved);
  writeSession = () => {
    if (restoring) return;
    const next = JSON.stringify(savedSessionFor(useAgentStore.getState().threads, useOpenSessionTabs.getState().ids, threadsViewThreadId));
    if (next === last) return;
    last = next;
    storage.setItem(LAST_SESSION_STORAGE_KEY, next);
  };
  useOpenSessionTabs.subscribe(() => writeSession?.());
  useAgentStore.subscribe((state, previous) => {
    if (state.threads !== previous.threads) writeSession?.();
  });
  return saved;
}
