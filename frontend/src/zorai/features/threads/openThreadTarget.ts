import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { normalizeBridgePayload } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { findThreadByAuthoritativeIdentity } from "@/components/agent-chat-panel/runtime/threadListQueries";
import { getAgentBridge } from "@/lib/agentDaemonConfig";
import { PRIMARY_AGENT_NAME } from "@/lib/agentNames";
import { buildHydratedRemoteThread, useAgentStore, type AgentThread, type RemoteAgentThreadRecord } from "@/lib/agentStore";
import { beginThreadLoading } from "./threadLoadingStore";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";
import { nextSplitCandidate, splitPaneIds, useThreadSplitStore, type ThreadSplitDropZone } from "./threadSplitModel";
import { useOpenSessionTabs } from "./openSessionTabs";

export function openListedThread(runtime: AgentChatPanelRuntimeValue, listed: AgentThread): Promise<boolean> {
  const localId = ensureListedThread(listed);
  return openThreadTarget(runtime, localId);
}

export function splitCandidateFor(runtime: AgentChatPanelRuntimeValue): string | null {
  const threads = useAgentStore.getState().threads;
  const localId = (id: string) => findThreadByAuthoritativeIdentity(threads, id)?.id ?? null;
  return nextSplitCandidate({
    activeId: runtime.activeThread?.id ?? null,
    shownIds: splitPaneIds(useThreadSplitStore.getState().root),
    openTabIds: useOpenSessionTabs.getState().ids.map(localId).filter((id): id is string => Boolean(id)),
    recentIds: [...runtime.filteredThreads].sort((left, right) => right.updatedAt - left.updatedAt).map((thread) => thread.id),
  });
}

export function openListedThreadInSplit(runtime: AgentChatPanelRuntimeValue, listed: AgentThread, zone: ThreadSplitDropZone): void {
  const anchorId = runtime.activeThread?.id ?? null;
  const localId = ensureListedThread(listed);
  if (anchorId && anchorId !== localId) useThreadSplitStore.getState().insert(anchorId, localId, zone);
  runtime.openThread(localId, { reuseLoaded: true });
}

function ensureListedThread(listed: AgentThread): string {
  const daemonThreadId = listed.daemonThreadId?.trim() || listed.id;
  const existing = findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, daemonThreadId)
    ?? findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, listed.id);
  if (existing) return existing.id;

  const thread: AgentThread = {
    ...listed,
    daemonThreadId,
  };
  useAgentStore.setState((state) => {
    if (state.threads.some((entry) => entry.id === thread.id || entry.daemonThreadId === daemonThreadId)) {
      return {};
    }
    return {
      threads: [thread, ...state.threads],
      messages: {
        ...state.messages,
        [thread.id]: state.messages[thread.id] ?? [],
      },
      todos: {
        ...state.todos,
        [thread.id]: state.todos[thread.id] ?? [],
      },
    };
  });
  return thread.id;
}

export async function openThreadTarget(runtime: AgentChatPanelRuntimeValue, targetThreadId: string, options?: { reuseLoaded?: boolean }): Promise<boolean> {
  const target = targetThreadId.trim();
  if (!target) return false;

  const local = findThreadByAuthoritativeIdentity(runtime.threads, target)
    ?? findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, target);
  if (local) {
    runtime.openThread(local.id, options);
    return true;
  }

  const finishLoading = beginThreadLoading(target);
  try {
    const remoteThread = await fetchDaemonThread(target);
    const raced = findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, target);
    if (raced) {
      runtime.openThread(raced.id);
      return true;
    }
    if (!remoteThread) return false;

    const hydrated = buildHydratedRemoteThread(
      remoteThread,
      useAgentStore.getState().agentSettings.agent_name || PRIMARY_AGENT_NAME,
    );
    if (!hydrated?.thread.daemonThreadId) return false;

    insertHydratedThread(hydrated);
    runtime.openThread(hydrated.thread.id);
    return true;
  } finally {
    finishLoading();
  }
}

async function fetchDaemonThread(daemonThreadId: string): Promise<RemoteAgentThreadRecord | null> {
  const zorai = getAgentBridge();
  if (!zorai?.agentGetThread) return null;

  const remotePayload = await zorai.agentGetThread(daemonThreadId, {
    messageLimit: resolveReactChatHistoryMessageLimit(
      useAgentStore.getState().agentSettings.react_chat_history_page_size,
    ),
    messageOffset: 0,
  }).catch(() => null);
  const remoteThread = normalizeBridgePayload(remotePayload);
  if (!remoteThread || typeof remoteThread !== "object") return null;
  const id = (remoteThread as { id?: unknown }).id;
  return typeof id === "string" && id.trim() ? remoteThread as RemoteAgentThreadRecord : null;
}

function insertHydratedThread(hydrated: ReturnType<typeof buildHydratedRemoteThread>): void {
  if (!hydrated) return;
  const daemonThreadId = hydrated.thread.daemonThreadId;
  if (!daemonThreadId) return;

  useAgentStore.setState((state) => {
    if (state.threads.some((thread) => thread.daemonThreadId === daemonThreadId)) {
      return {};
    }
    return {
      threads: [...state.threads, hydrated.thread].sort((left, right) => right.updatedAt - left.updatedAt),
      messages: {
        ...state.messages,
        [hydrated.thread.id]: hydrated.messages,
      },
      todos: {
        ...state.todos,
        [hydrated.thread.id]: state.todos[hydrated.thread.id] ?? [],
      },
    };
  });
}
