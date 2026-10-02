import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { normalizeBridgePayload } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { findThreadByAuthoritativeIdentity } from "@/components/agent-chat-panel/runtime/threadListQueries";
import { getAgentBridge } from "@/lib/agentDaemonConfig";
import { PRIMARY_AGENT_NAME } from "@/lib/agentNames";
import { buildHydratedRemoteThread, useAgentStore, type AgentThread, type RemoteAgentThreadRecord } from "@/lib/agentStore";
import { beginThreadLoading } from "./threadLoadingStore";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";

export function openListedThread(runtime: AgentChatPanelRuntimeValue, listed: AgentThread): Promise<boolean> {
  const localId = ensureListedThread(listed);
  return openThreadTarget(runtime, localId);
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

export async function openThreadTarget(runtime: AgentChatPanelRuntimeValue, targetThreadId: string): Promise<boolean> {
  const target = targetThreadId.trim();
  if (!target) return false;

  const local = findThreadByAuthoritativeIdentity(runtime.threads, target)
    ?? findThreadByAuthoritativeIdentity(useAgentStore.getState().threads, target);
  if (local) {
    runtime.openThread(local.id);
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
