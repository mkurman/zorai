import {
  AGENT_ACTIVE_THREAD_FILE,
  AGENT_CHAT_FILE,
  AGENT_DAEMON_THREAD_MAP_FILE,
  buildHydratedRemoteThread,
  type AgentChatState,
  deserializeMessage,
  deserializeThread,
  getAgentDbApi,
  readPersistedJson,
  serializeMessage,
  serializeThread,
  shouldPersistHistory,
  syncChatCounters,
} from "./history";
import { getBridge } from "../bridge";
import {
  DEFAULT_AGENT_SETTINGS,
  type DiskAgentSettings,
  looksLikeDaemonAgentConfig,
  normalizeAgentSettingsFromSource,
} from "./settings";
import { useAgentStore } from "./store";
import { getDaemonAgentConfig } from "../daemonConfig";

export function retainActiveThreadMessages<T>(
  messages: Record<string, T[]>,
  activeThreadId: string | null,
): Record<string, T[]> {
  if (!activeThreadId) return {};
  const active = messages[activeThreadId];
  return active ? { [activeThreadId]: active } : {};
}

export async function hydrateAgentStore(): Promise<void> {
  try {
    await hydrateAgentStoreInner();
  } finally {
    await useAgentStore.getState().refreshSubAgents();
  }
}

async function hydrateAgentStoreInner(): Promise<void> {
  const bridge = getBridge();
  let configuredBackend = DEFAULT_AGENT_SETTINGS.agent_backend;
  let agentSettingsHydrated = false;

  if (bridge?.agentGetConfig) {
    const daemonState = await getDaemonAgentConfig();
    if (looksLikeDaemonAgentConfig(daemonState)) {
      const merged = normalizeAgentSettingsFromSource(daemonState as DiskAgentSettings);
      configuredBackend = merged.agent_backend;
      useAgentStore.setState({
        agentSettings: merged,
        agentSettingsDirty: false,
      });
      agentSettingsHydrated = true;
    }
  } else {
    agentSettingsHydrated = true;
  }

  useAgentStore.setState({ agentSettingsHydrated });

  if (!shouldPersistHistory(configuredBackend)) {
    const zorai = getBridge();
    if (zorai?.agentListThreads) {
      const remoteThreads = await zorai.agentListThreads().catch(() => []);
      if (Array.isArray(remoteThreads) && remoteThreads.length > 0) {
        const messages: AgentChatState["messages"] = {};
        const threads = [];
        for (const remoteThread of remoteThreads) {
          const hydrated = buildHydratedRemoteThread(
            remoteThread ?? {},
            useAgentStore.getState().agentSettings.agent_name,
          );
          if (!hydrated) {
            continue;
          }
          threads.push(hydrated.thread);
          messages[hydrated.thread.id] = hydrated.messages;
        }
        if (threads.length > 0) {
          const sortedThreads = threads.sort((left, right) => right.updatedAt - left.updatedAt);
          const activeThreadId = sortedThreads[0]?.id ?? null;
          const hydrated: AgentChatState = {
            threads: sortedThreads,
            messages: retainActiveThreadMessages(messages, activeThreadId),
            todos: {},
            activeThreadId,
          };
          syncChatCounters(hydrated);
          useAgentStore.setState(hydrated);
        }
      }
    }
    return;
  }

  const api = getAgentDbApi();
  const daemonThreadMap = await readPersistedJson<Record<string, string>>(AGENT_DAEMON_THREAD_MAP_FILE) ?? {};
  const savedActiveThread = await readPersistedJson<{ activeThreadId: string | null }>(AGENT_ACTIVE_THREAD_FILE);
  const dbThreads = await api?.dbListThreads?.();
  if (Array.isArray(dbThreads) && dbThreads.length > 0) {
    const hydratedThreads = dbThreads.map((thread) => ({
      ...deserializeThread(thread),
      daemonThreadId: daemonThreadMap[thread.id] ?? null,
    }));
    const savedId = savedActiveThread?.activeThreadId;
    const restoredId = (savedId && hydratedThreads.some((thread) => thread.id === savedId))
      ? savedId
      : (hydratedThreads.length > 0
        ? hydratedThreads.reduce((left, right) => (left.updatedAt >= right.updatedAt ? left : right)).id
        : null);
    const messages: AgentChatState["messages"] = {};
    if (restoredId) {
      const threadMessages = await api?.dbListMessages?.(restoredId, 500) ?? [];
      messages[restoredId] = threadMessages.map(deserializeMessage);
      const active = hydratedThreads.find((thread) => thread.id === restoredId);
      const loaded = messages[restoredId];
      if (active && loaded && loaded.length > 0) {
        active.lastMessagePreview = loaded[loaded.length - 1]?.content?.slice(0, 100) ?? active.lastMessagePreview;
      }
    }
    const hydrated: AgentChatState = {
      threads: hydratedThreads,
      messages,
      todos: {},
      activeThreadId: restoredId,
    };
    syncChatCounters(hydrated);
    useAgentStore.setState(hydrated);
    return;
  }

  const legacyChat = await readPersistedJson<AgentChatState>(AGENT_CHAT_FILE);
  if (!legacyChat || !Array.isArray(legacyChat.threads) || typeof legacyChat.messages !== "object") {
    return;
  }

  const hydrated: AgentChatState = {
    threads: legacyChat.threads.map((thread) => ({
      ...thread,
      daemonThreadId: daemonThreadMap[thread.id] ?? thread.daemonThreadId ?? null,
    })),
    messages: retainActiveThreadMessages(legacyChat.messages, legacyChat.activeThreadId ?? null),
    todos: {},
    activeThreadId: legacyChat.activeThreadId ?? null,
  };
  syncChatCounters(hydrated);
  useAgentStore.setState(hydrated);

  for (const thread of hydrated.threads) {
    await api?.dbCreateThread?.(serializeThread(thread));
    for (const message of hydrated.messages[thread.id] ?? []) {
      await api?.dbAddMessage?.(serializeMessage(message));
    }
  }
}
