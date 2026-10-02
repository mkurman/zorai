import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { abortThreadStream, buildHydratedRemoteThread, useAgentStore } from "@/lib/agentStore";
import { getAgentDbApi } from "@/lib/agentStore/history";
import { getAgentBridge, shouldUseDaemonRuntime } from "@/lib/agentDaemonConfig";
import { fetchAgentRuns, isSubagentRun, type AgentRun } from "@/lib/agentRuns";
import { fetchThreadTodos } from "@/lib/agentTodos";
import { beginThreadLoadingFor } from "@/zorai/features/threads/threadLoadingStore";
import { isLeadOnlyPersona, LEAD_PERSONA_SPAWN_ERROR } from "@/zorai/features/threads/leadPersonas";
import { sameAgentRunSnapshot } from "@/zorai/features/threads/sessionCooperation";
import {
  composerDraftIsImageCommand,
  useComposerDraftStore,
  writeComposerDraftInput,
} from "@/zorai/features/threads/composerDraftStore";
import {
  draftThreadForOwnerSnapshot,
  snapshotThreadOwnerRuntimeProfile,
} from "@/zorai/features/threads/threadOwnerRuntime";
import {
  clearThreadRetryStatus,
  suppressThreadRetryStatus,
} from "@/zorai/features/threads/threadRetryStatus";
import { resolveReactChatHistoryMessageLimit } from "@/lib/chatHistoryPageSize";
import { deriveSpawnedAgentTree } from "@/lib/spawnedAgentTree";
import type { SpawnedAgentTree } from "@/lib/spawnedAgentTree";
import { getTerminalController } from "@/lib/terminalRegistry";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useNotificationStore } from "@/lib/notificationStore";
import { useSnippetStore } from "@/lib/snippetStore";
import { useTranscriptStore } from "@/lib/transcriptStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentThread, AgentTodoItem } from "@/lib/agentStore";
import { isGatewayAgentThread, isInternalAgentThread } from "@/lib/agentStore";
import type { GoalRun } from "@/lib/goalRuns";
import type { Workspace } from "@/lib/types";
import type { WelesHealthState } from "@/lib/agentStore/types";
import { useDaemonAgentActions } from "./useDaemonAgentActions";
import { useDaemonAgentEvents } from "./useDaemonAgentEvents";
import {
  hydrateDaemonThreadIntoLocalState,
  beginThreadHistoryReplace,
  loadDaemonThreadPageIntoLocalState,
  reloadDaemonThreadIntoLocalState,
  resolveAbsoluteMessageIndex,
  resolveDaemonOwnedThreadId,
  threadHistoryReplaceEpoch,
  trimDaemonThreadMessagesToLatestWindow,
} from "./daemonHelpers";
import {
  beginProgrammaticThreadHistoryScroll,
  endProgrammaticThreadHistoryScroll,
  resolveOlderThreadPageMessageOffset,
  setFollowThreadHistoryBottom,
  shouldFollowThreadHistoryBottom,
} from "./threadHistoryScroll";
import { useLegacyAgentMessaging } from "./useLegacyAgentMessaging";
import { finalizeStreamingAssistantMessages, threadTurnIsActive } from "./threadTurnState";
import { beginThreadStopBarrier, completeThreadStopBarrier } from "./threadStopBarrier";
import type {
  AgentChatPanelRuntimeValue,
  AgentChatPanelView,
} from "./types";
import { createThreadCollaborationActions } from "./threadCollaborationActions";
import { fetchHydratedRemoteThreads, findThreadByAuthoritativeIdentity } from "./threadListQueries";
import { mergeRemoteThreadProfile } from "@/lib/agentStore/threadProfileMerge";
import {
  pinnedMessageBudgetChars,
  sumMessageContentChars,
} from "@/lib/agent-client/pinnedMessageBudget";

const EMPTY_MESSAGES: ReturnType<typeof useAgentStore.getState>["messages"][string] = [];
const EMPTY_TODOS: ReturnType<typeof useAgentStore.getState>["todos"][string] = [];

type SpawnedAgentNavigationState = {
  tree: SpawnedAgentTree<AgentRun> | null;
  canGoBackThread: boolean;
  threadNavigationDepth: number;
  backThreadTitle: string | null;
};

type RemoteAgentThread = {
  id: string;
  title: string;
  messages: unknown[];
};

type PendingSpawnedThreadHydration = {
  promise: Promise<string | null>;
};

const pendingSpawnedThreadHydrations = new Map<string, PendingSpawnedThreadHydration>();

export function resetPendingSpawnedThreadHydrationsForTest(): void {
  pendingSpawnedThreadHydrations.clear();
}

function filterThreadsForSearch(
  threads: AgentThread[],
  searchQuery: string,
): AgentThread[] {
  if (!searchQuery) {
    return threads;
  }
  const lower = searchQuery.toLowerCase();
  return threads.filter(
    (thread) =>
      thread.title.toLowerCase().includes(lower)
      || thread.lastMessagePreview.toLowerCase().includes(lower),
  );
}

export function filterThreadsForBrowserView(
  threads: AgentThread[],
  view: AgentChatPanelView,
): AgentThread[] {
  switch (view) {
    case "internal":
      return threads.filter((thread) => isInternalAgentThread(thread));
    case "gateway":
      return threads.filter((thread) => isGatewayAgentThread(thread));
    case "threads":
    default:
      return threads;
  }
}

export function buildAgentChatPanelTabItems({
  threads,
  pinnedMessageCount,
  scopedCognitiveEventCount,
  usageMessageCount,
}: {
  threads: AgentThread[];
  pinnedMessageCount: number;
  scopedCognitiveEventCount: number;
  usageMessageCount: number;
}): Array<{ id: AgentChatPanelView; label: string; count: number | null }> {
  const internalThreadCount = threads.filter((thread) => isInternalAgentThread(thread)).length;
  const gatewayThreadCount = threads.filter((thread) => isGatewayAgentThread(thread)).length;

  return [
    { id: "threads", label: "Threads", count: threads.length },
    { id: "chat", label: "Chat", count: null },
    ...(pinnedMessageCount > 0 ? [{ id: "pinned" as const, label: "Pinned", count: pinnedMessageCount }] : []),
    { id: "trace", label: "Trace", count: scopedCognitiveEventCount },
    { id: "usage", label: "Usage", count: usageMessageCount },
    { id: "context", label: "Context", count: null },
    { id: "graph", label: "Graph", count: null },
    { id: "coding-agents", label: "Coding Agents", count: null },
    { id: "ai-training", label: "AI Training", count: null },
    { id: "tasks", label: "Tasks", count: null },
    { id: "internal", label: "Internal", count: internalThreadCount },
    { id: "gateway", label: "Gateway", count: gatewayThreadCount },
    { id: "subagents", label: "Subagents", count: null },
  ];
}

function findLocalThreadByDaemonThreadId(
  threads: AgentThread[],
  daemonThreadId: string,
): AgentThread | undefined {
  return threads.find((thread) => thread.daemonThreadId === daemonThreadId);
}

export function deriveSpawnedAgentNavigationState({
  activeThread,
  threads,
  threadHistoryStack,
  runs,
}: {
  activeThread: AgentThread | undefined;
  threads: AgentThread[];
  threadHistoryStack: string[];
  runs: AgentRun[];
}): SpawnedAgentNavigationState {
  const activeDaemonThreadId = activeThread?.daemonThreadId ?? null;
  const backThreadId = threadHistoryStack[threadHistoryStack.length - 1] ?? null;
  const backThreadTitle = backThreadId
    ? threads.find((thread) => thread.id === backThreadId)?.title ?? null
    : null;

  return {
    tree: deriveSpawnedAgentTree(runs, activeDaemonThreadId),
    canGoBackThread: threadHistoryStack.length > 0,
    threadNavigationDepth: threadHistoryStack.length,
    backThreadTitle,
  };
}

export async function openSpawnedAgentThreadFromRun({
  activeThreadId,
  threads,
  workspaces,
  run,
  messageLimit,
  getRemoteThread,
  fetchThreadTodos: fetchThreadTodosForThread,
  createThread,
  addMessage,
  setThreadDaemonId,
  setThreadTodos,
  openSpawnedThread,
}: {
  activeThreadId: string | null;
  threads: AgentThread[];
  workspaces: Workspace[];
  run: AgentRun;
  messageLimit: number | null;
  getRemoteThread?: (
    threadId: string,
    options: { messageLimit: number | null },
  ) => Promise<{ id: string; title: string; messages: unknown[] } | null>;
  fetchThreadTodos: (threadId: string) => Promise<AgentTodoItem[]>;
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  openSpawnedThread: ReturnType<typeof useAgentStore.getState>["openSpawnedThread"];
}): Promise<boolean> {
  if (!activeThreadId || !run.thread_id) {
    return false;
  }

  const completeSpawnedThreadOpen = async (
    pendingHydration: PendingSpawnedThreadHydration,
  ) => {
    const localThreadId = await pendingHydration.promise;
    if (!localThreadId) {
      return false;
    }
    const currentActiveThreadId = useAgentStore.getState().activeThreadId;
    if (currentActiveThreadId === localThreadId) {
      return true;
    }
    if (currentActiveThreadId !== activeThreadId) {
      return false;
    }
    openSpawnedThread(activeThreadId, localThreadId);
    return true;
  };

  const pendingHydration = pendingSpawnedThreadHydrations.get(run.thread_id);
  if (pendingHydration) {
    return completeSpawnedThreadOpen(pendingHydration);
  }

  const existingThread = findLocalThreadByDaemonThreadId(threads, run.thread_id);
  if (existingThread) {
    if (existingThread.id === activeThreadId) {
      return false;
    }
    const existingMessages = useAgentStore.getState().messages[existingThread.id] ?? [];
    if (existingMessages.length === 0 && getRemoteThread) {
      const remoteThread = await getRemoteThread(run.thread_id, { messageLimit });
      const hydrated = remoteThread
        ? buildHydratedRemoteThread(remoteThread as any, existingThread.agent_name || "assistant")
        : null;
      if (hydrated) {
        const hydratedMessages = hydrated.messages.map((message) => ({
          ...message,
          threadId: existingThread.id,
        }));
        useAgentStore.setState((state) => ({
          threads: state.threads.map((thread) => thread.id === existingThread.id ? {
            ...thread,
            ...hydrated.thread,
            id: existingThread.id,
            daemonThreadId: run.thread_id,
            workspaceId: thread.workspaceId,
            surfaceId: thread.surfaceId,
            paneId: thread.paneId,
          } : thread),
          messages: {
            ...state.messages,
            [existingThread.id]: hydratedMessages,
          },
        }));
        const todos = await fetchThreadTodosForThread(run.thread_id).catch(() => []);
        setThreadTodos(existingThread.id, todos);
      }
    }
    openSpawnedThread(activeThreadId, existingThread.id);
    return true;
  }

  if (!getRemoteThread) {
    return false;
  }

  let resolveHydration!: (threadId: string | null) => void;
  let rejectHydration!: (reason?: unknown) => void;
  const hydratePromise = new Promise<string | null>((resolve, reject) => {
    resolveHydration = resolve;
    rejectHydration = reject;
  });

  const pendingEntry: PendingSpawnedThreadHydration = { promise: hydratePromise };
  pendingSpawnedThreadHydrations.set(run.thread_id, pendingEntry);

  void (async () => {
    try {
      const remoteThread = await getRemoteThread(run.thread_id!, { messageLimit });
      if (!remoteThread) {
        resolveHydration(null);
        return;
      }

      const preservedSelection = {
        activeThreadId: useAgentStore.getState().activeThreadId,
        threadHistoryStack: [...useAgentStore.getState().threadHistoryStack],
      };
      const localThreadId = await hydrateDaemonThreadIntoLocalState({
        sessionId: run.session_id,
        fallbackTitle: run.title,
        workspaces,
        remoteThread: remoteThread as any,
        fetchThreadTodos: fetchThreadTodosForThread,
        createThread,
        addMessage,
        setThreadDaemonId,
        setThreadTodos,
        onThreadReady: () => {
          useAgentStore.setState({
            activeThreadId: preservedSelection.activeThreadId,
            threadHistoryStack: preservedSelection.threadHistoryStack,
          });
        },
      });
      resolveHydration(localThreadId);
    } catch (error) {
      rejectHydration(error);
    } finally {
      if (pendingSpawnedThreadHydrations.get(run.thread_id!) === pendingEntry) {
        pendingSpawnedThreadHydrations.delete(run.thread_id!);
      }
    }
  })();

  try {
    return completeSpawnedThreadOpen(pendingEntry);
  } finally {
    // Cleanup happens in the async hydration runner once the promise settles.
  }
}

export function useAgentChatPanelProviderValue(): {
  isOpen: boolean;
  value: AgentChatPanelRuntimeValue;
} {
  const isOpen = useWorkspaceStore((state) => state.agentPanelOpen);
  const togglePanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  const activePaneId = useWorkspaceStore((state) => state.activePaneId());
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const workspaces = useWorkspaceStore((state) => state.workspaces);

  const threads = useAgentStore((state) => state.threads);
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const threadHistoryStack = useAgentStore((state) => state.threadHistoryStack);
  const storeCreateThread = useAgentStore((state) => state.createThread);
  const deleteThread = useAgentStore((state) => state.deleteThread);
  const storeSetActiveThread = useAgentStore((state) => state.setActiveThread);
  const storeOpenSpawnedThread = useAgentStore((state) => state.openSpawnedThread);
  const storeGoBackThread = useAgentStore((state) => state.goBackThread);
  const setActiveThread = useCallback((id: string | null) => {
    if (id && id !== useAgentStore.getState().activeThreadId) {
      beginThreadHistoryReplace(id);
    }
    storeSetActiveThread(id);
  }, [storeSetActiveThread]);
  const openSpawnedThreadInStore = useCallback((fromThreadId: string, toThreadId: string) => {
    if (toThreadId !== useAgentStore.getState().activeThreadId) {
      beginThreadHistoryReplace(toThreadId);
    }
    storeOpenSpawnedThread(fromThreadId, toThreadId);
  }, [storeOpenSpawnedThread]);
  const goBackThread = useCallback(() => {
    const state = useAgentStore.getState();
    for (let index = state.threadHistoryStack.length - 1; index >= 0; index -= 1) {
      const previousId = state.threadHistoryStack[index];
      if (previousId && state.threads.some((thread) => thread.id === previousId)) {
        if (previousId !== state.activeThreadId) {
          beginThreadHistoryReplace(previousId);
        }
        break;
      }
    }
    storeGoBackThread();
  }, [storeGoBackThread]);
  const addMessage = useAgentStore((state) => state.addMessage);
  const deleteMessageFromStore = useAgentStore((state) => state.deleteMessage);
  const updateLastAssistantMessage = useAgentStore((state) => state.updateLastAssistantMessage);
  const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
  const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const searchQuery = useAgentStore((state) => state.searchQuery);
  const setSearchQuery = useAgentStore((state) => state.setSearchQuery);
  const storeMessages = useAgentStore((state) => activeThreadId ? state.messages[activeThreadId] : undefined);
  const storeTodos = useAgentStore((state) => activeThreadId ? state.todos[activeThreadId] : undefined);
  const allMessagesByThread = useAgentStore((state) => state.messages);
  const activeThread = threads.find((thread) => thread.id === activeThreadId);
  const activeDaemonThreadId = activeThread?.daemonThreadId ?? null;
  const activeDaemonThreadIdRef = useRef<string | null>(activeDaemonThreadId);
  const operationalEvents = useAgentMissionStore((state) => state.operationalEvents);
  const cognitiveEvents = useAgentMissionStore((state) => state.cognitiveEvents);
  const contextSnapshots = useAgentMissionStore((state) => state.contextSnapshots);
  const approvals = useAgentMissionStore((state) => state.approvals);
  const memory = useAgentMissionStore((state) => state.memory);
  const updateMemory = useAgentMissionStore((state) => state.updateMemory);
  const historySummary = useAgentMissionStore((state) => state.historySummary);
  const historyHits = useAgentMissionStore((state) => state.historyHits);
  const symbolHits = useAgentMissionStore((state) => state.symbolHits);
  const snippets = useSnippetStore((state) => state.snippets);
  const transcripts = useTranscriptStore((state) => state.transcripts);
  const addNotification = useNotificationStore((state) => state.addNotification);

  const [view, setView] = useState<AgentChatPanelView>("threads");
  const [chatBackView, setChatBackView] = useState<AgentChatPanelView>("threads");
  const [historyQuery, setHistoryQuery] = useState("");
  const [symbolQuery, setSymbolQuery] = useState("");
  const [daemonTodosByThread, setDaemonTodosByThread] = useState<Record<string, AgentTodoItem[]>>({});
  const [goalRunsForTrace, setGoalRunsForTrace] = useState<GoalRun[]>([]);
  const [spawnedAgentRuns, setSpawnedAgentRuns] = useState<AgentRun[]>([]);
  const [latestDivergentSessionId, setLatestDivergentSessionId] = useState<string | null>(null);
  const [welesHealth, setWelesHealth] = useState<WelesHealthState | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const daemonThreadIdRef = useRef<string | null>(null);
  const daemonLocalThreadRef = useRef<string | null>(null);
  const pendingGatewayMessagesRef = useRef<Array<{ role: "user"; content: string; inputTokens: number; outputTokens: number; totalTokens: number; isCompactionSummary: boolean }>>([]);
  const goalRunWorkspacesRef = useRef<Record<string, string>>({});
  const latestLoadedThreadIdRef = useRef<string | null>(null);

  const createThread = useCallback((opts: Parameters<typeof storeCreateThread>[0]) => {
    const state = useAgentStore.getState();
    const profile = snapshotThreadOwnerRuntimeProfile(
      draftThreadForOwnerSnapshot(
        { agentId: opts.agentId, agentName: opts.agentName },
        state.agentSettings.agent_name,
      ),
      state.subAgents,
      state.agentSettings,
      state.conciergeConfig,
    );
    const id = storeCreateThread({ ...opts, ...profile });
    if (opts.activate !== false) {
      daemonLocalThreadRef.current = id;
      daemonThreadIdRef.current = null;
      latestLoadedThreadIdRef.current = id;
    }
    return id;
  }, [storeCreateThread]);

  useDaemonAgentEvents({
    agentBackend: agentSettings.agent_backend,
    activePaneId,
    activeThread,
    activeThreadId,
    activeWorkspace,
    addMessage,
    createThread,
    setActiveThread,
    setThreadDaemonId,
    setThreadTodos,
    updateLastAssistantMessage,
    addNotification,
    daemonThreadIdRef,
    daemonLocalThreadRef,
    pendingGatewayMessagesRef,
    goalRunWorkspacesRef,
    setDaemonTodosByThread,
    setGoalRunsForTrace,
    setChatBackView,
    setLatestDivergentSessionId,
    setView,
    setWelesHealth,
  });

  const resolveTargetDaemonThreadId = useCallback((threadId?: string | null) => {
    const targetThreadId = threadId ?? activeThreadId;
    if (!targetThreadId) return null;
    return resolveDaemonOwnedThreadId({
      threads: useAgentStore.getState().threads,
      threadId: targetThreadId,
      activeThreadId,
      activeDaemonThreadId: daemonThreadIdRef.current,
    });
  }, [activeThreadId]);

  const stopStreaming = useCallback((threadId?: string | null, daemonThreadIdOverride?: string | null) => {
    const targetThreadId = threadId ?? activeThreadId;
    if (!targetThreadId && !daemonThreadIdOverride) return;

    const resolvedDaemonThreadId = targetThreadId
      ? resolveTargetDaemonThreadId(targetThreadId)
      : null;
    const daemonThreadId = daemonThreadIdOverride?.trim() || resolvedDaemonThreadId;
    if (daemonThreadId) {
      suppressThreadRetryStatus(daemonThreadId);
    }

    if (shouldUseDaemonRuntime(agentSettings.agent_backend)) {
      const zorai = getAgentBridge();
      if (daemonThreadId && zorai?.agentStopStream) {
        if (targetThreadId) beginThreadStopBarrier(targetThreadId);
        void zorai.agentStopStream(daemonThreadId).then((result) => {
          const accepted = result && typeof result === "object" && "ok" in result
            ? result.ok !== false
            : result !== false;
          if (!accepted && targetThreadId) completeThreadStopBarrier(targetThreadId);
        }).catch(() => {
          if (targetThreadId) completeThreadStopBarrier(targetThreadId);
        });
      }
    }

    if (!targetThreadId) return;

    abortThreadStream(targetThreadId);
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    const threadMessages = useAgentStore.getState().getThreadMessages(targetThreadId);
    const lastMessage = threadMessages[threadMessages.length - 1];
    if (lastMessage?.role === "assistant" && lastMessage.isStreaming) {
      updateLastAssistantMessage(targetThreadId, lastMessage.content || "(stopped)", false);
    }
    finalizeStreamingAssistantMessages(targetThreadId);
    useAgentMissionStore.getState().setSharedCursorMode("idle");
  }, [activeThreadId, agentSettings.agent_backend, resolveTargetDaemonThreadId, updateLastAssistantMessage]);

  const retryStreamNow = useCallback((threadId?: string | null) => {
    const targetThreadId = threadId ?? activeThreadId;
    if (!targetThreadId) return;

    const daemonThreadId = resolveTargetDaemonThreadId(targetThreadId);
    if (!daemonThreadId) return;

    clearThreadRetryStatus(daemonThreadId);

    if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) {
      return;
    }

    const zorai = getAgentBridge();
    if (zorai?.agentRetryStreamNow) {
      void zorai.agentRetryStreamNow(daemonThreadId);
    }
  }, [activeThreadId, agentSettings.agent_backend, resolveTargetDaemonThreadId]);

  const { sendMessageLegacy } = useLegacyAgentMessaging({
    activeThreadId,
    agentSettings,
    addMessage,
    abortRef,
    createThread,
    setView,
    stopStreaming,
    updateLastAssistantMessage,
  });

  const {
    builtinAgentSetup,
    cancelBuiltinAgentSetup,
    canStartGoalRun,
    sendDaemonMessage,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
  } = useDaemonAgentActions({
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    addNotification,
    agentSettings,
    createThread,
    daemonThreadIdRef,
    daemonLocalThreadRef,
    goalRunWorkspacesRef,
    goalRunsForTrace,
    latestDivergentSessionId,
    setActiveThread,
    setDaemonTodosByThread,
    setThreadDaemonId,
    setThreadTodos,
    setLatestDivergentSessionId,
    setView,
  });

  useEffect(() => {
    if (threads.length === 0) return;
    threads.forEach((thread) => {
      if (!thread.daemonThreadId) return;
      const items = daemonTodosByThread[thread.daemonThreadId];
      if (!items) return;
      setThreadTodos(thread.id, items);
    });
  }, [daemonTodosByThread, setThreadTodos, threads]);

  const refreshSpawnedAgentRuns = useCallback(async () => {
    if (!activeDaemonThreadId) {
      activeDaemonThreadIdRef.current = null;
      setSpawnedAgentRuns((current) => current.length === 0 ? current : []);
      return;
    }
    activeDaemonThreadIdRef.current = activeDaemonThreadId;
    const runs = await fetchAgentRuns(activeDaemonThreadId);
    if (activeDaemonThreadIdRef.current !== activeDaemonThreadId) return;
    const next = runs.filter(isSubagentRun);
    setSpawnedAgentRuns((current) => sameAgentRunSnapshot(current, next) ? current : next);
  }, [activeDaemonThreadId]);

  useEffect(() => {
    activeDaemonThreadIdRef.current = activeDaemonThreadId;
    setSpawnedAgentRuns([]);
  }, [activeDaemonThreadId]);

  useEffect(() => {
    void refreshSpawnedAgentRuns();
    const interval = window.setInterval(() => {
      void refreshSpawnedAgentRuns();
    }, 5000);
    return () => window.clearInterval(interval);
  }, [refreshSpawnedAgentRuns]);

  useEffect(() => {
    const zorai = getAgentBridge();
    if (!zorai?.onAgentEvent) {
      return;
    }

    const unsubscribe = zorai.onAgentEvent((event: any) => {
      if (event?.type === "task_update") {
        void refreshSpawnedAgentRuns();
      } else if (event?.type === "message_feedback_updated") {
        const daemonThreadId: string | undefined = event.thread_id;
        const messageId: string | undefined = event.message_id;
        const rawReaction = typeof event.reaction === "string" ? event.reaction : null;
        const reaction: "up" | "down" | null =
          rawReaction === "up" || rawReaction === "down" ? rawReaction : null;
        if (!daemonThreadId || !messageId) return;
        useAgentStore.setState((state) => {
          const next: typeof state.messages = { ...state.messages };
          let changed = false;
          for (const [threadId, list] of Object.entries(state.messages)) {
            const matchThread = state.threads.find((entry) => entry.id === threadId);
            if (matchThread?.daemonThreadId !== daemonThreadId) continue;
            const updated = list.map((message) =>
              message.id === messageId ? { ...message, feedback: reaction } : message);
            if (updated !== list) {
              next[threadId] = updated;
              changed = true;
            }
          }
          return changed ? { messages: next } : state;
        });
      }
    });

    return () => unsubscribe?.();
  }, [refreshSpawnedAgentRuns]);

  const messages = useMemo(() => storeMessages ?? EMPTY_MESSAGES, [storeMessages]);
  const todos = useMemo(() => storeTodos ?? EMPTY_TODOS, [storeTodos]);
  const scopePaneId = activeThread?.paneId ?? activePaneId;
  const pendingApprovals = useMemo(
    () => approvals.filter((approval) => approval.status === "pending"),
    [approvals],
  );
  const scopeController = getTerminalController(scopePaneId);
  const usageMessageCount = useMemo(
    () => Object.values(allMessagesByThread)
      .flat()
      .filter((message) => message.role === "assistant" && ((message.totalTokens ?? 0) > 0 || message.cost !== undefined)).length,
    [allMessagesByThread],
  );
  const scopedOperationalEvents = useMemo(() => {
    if (!scopePaneId) return operationalEvents.slice(0, 30);
    return operationalEvents.filter((event) => event.paneId === scopePaneId).slice(0, 30);
  }, [operationalEvents, scopePaneId]);
  const scopedCognitiveEvents = useMemo(() => {
    if (!scopePaneId) return cognitiveEvents.slice(0, 20);
    return cognitiveEvents.filter((event) => event.paneId === scopePaneId).slice(0, 20);
  }, [cognitiveEvents, scopePaneId]);
  const latestContextSnapshot = useMemo(() => {
    if (!scopePaneId) return contextSnapshots[0];
    return contextSnapshots.find((snapshot) => snapshot.paneId === scopePaneId) ?? contextSnapshots[0];
  }, [contextSnapshots, scopePaneId]);
  const spawnedAgentNavigation = useMemo(
    () => deriveSpawnedAgentNavigationState({
      activeThread,
      threads,
      threadHistoryStack,
      runs: spawnedAgentRuns,
    }),
    [activeThread, spawnedAgentRuns, threadHistoryStack, threads],
  );

  useEffect(() => {
    if (!shouldFollowThreadHistoryBottom()) return;
    beginProgrammaticThreadHistoryScroll();
    messagesEndRef.current?.scrollIntoView({ block: "end" });
    endProgrammaticThreadHistoryScroll();
  }, [messages.length]);

  const inputIsImageCommand = useComposerDraftStore((state) => composerDraftIsImageCommand(state.input));

  useEffect(() => {
    if (isOpen && (activeThreadId || inputIsImageCommand)) {
      setView("chat");
      setTimeout(() => inputRef.current?.focus(), 100);
    } else if (isOpen) {
      setView("threads");
    }
  }, [activeThreadId, inputIsImageCommand, isOpen]);

  useEffect(() => {
    const handleComposeImage = (event: Event) => {
      const detail = event instanceof CustomEvent ? event.detail : null;
      const prompt = detail && typeof detail.prompt === "string" ? detail.prompt.trim() : "";
      useWorkspaceStore.setState({ agentPanelOpen: true });
      setChatBackView("threads");
      setView("chat");
      writeComposerDraftInput(prompt ? `/image ${prompt}` : "/image ");
      window.setTimeout(() => inputRef.current?.focus(), 50);
    };

    window.addEventListener("zorai-agent-compose-image", handleComposeImage);
    window.addEventListener("zorai-agent-compose-image", handleComposeImage);
    return () => {
      window.removeEventListener("zorai-agent-compose-image", handleComposeImage);
      window.removeEventListener("zorai-agent-compose-image", handleComposeImage);
    };
  }, []);

  const searchedThreads = useMemo(
    () => filterThreadsForSearch(threads, searchQuery),
    [threads, searchQuery],
  );
  const filteredThreads = useMemo(
    () => filterThreadsForBrowserView(searchedThreads, view),
    [searchedThreads, view],
  );
  const isStreamingResponse = threadTurnIsActive(messages);
  const canOpenSpawnedThread = useCallback((run: AgentRun) => {
    if (!run.thread_id) {
      return false;
    }

    const existingThread = findLocalThreadByDaemonThreadId(threads, run.thread_id);
    if (existingThread) {
      return existingThread.id !== activeThreadId;
    }

    return Boolean(getAgentBridge()?.agentGetThread);
  }, [activeThreadId, threads]);
  const openSpawnedThread = useCallback(async (run: AgentRun) => {
    const zorai = getAgentBridge();
    return openSpawnedAgentThreadFromRun({
      activeThreadId,
      threads,
      workspaces,
      run,
      messageLimit: resolveReactChatHistoryMessageLimit(agentSettings.react_chat_history_page_size) ?? null,
      getRemoteThread: zorai?.agentGetThread
        ? async (threadId, options): Promise<RemoteAgentThread | null> => {
          const result = await zorai.agentGetThread?.(threadId, options);
          return (result as RemoteAgentThread | null | undefined) ?? null;
        }
        : undefined,
      fetchThreadTodos,
      createThread,
      addMessage,
      setThreadDaemonId,
      setThreadTodos,
      openSpawnedThread: openSpawnedThreadInStore,
    });
  }, [
    activeThreadId,
    addMessage,
    agentSettings.react_chat_history_page_size,
    createThread,
    openSpawnedThreadInStore,
    setThreadDaemonId,
    setThreadTodos,
    threads,
    workspaces,
  ]);

  const refreshThreadList = useCallback(async () => {
    const zorai = getAgentBridge();
    if (!zorai?.agentListThreads) {
      return;
    }

    const remoteThreads = await zorai.agentListThreads().catch(() => []);
    if (!Array.isArray(remoteThreads)) {
      return;
    }

    const agentName = useAgentStore.getState().agentSettings.agent_name;
    useAgentStore.setState((state) => {
      const existingByDaemonThreadId = new Map(
        state.threads
          .filter((thread) => typeof thread.daemonThreadId === "string" && thread.daemonThreadId)
          .map((thread) => [thread.daemonThreadId as string, thread]),
      );
      const nextThreads: AgentThread[] = [];
      const nextMessages = { ...state.messages };
      const nextTodos = { ...state.todos };
      const seenDaemonThreadIds = new Set<string>();

      for (const remoteThread of remoteThreads) {
        const hydrated = buildHydratedRemoteThread(remoteThread ?? {}, agentName);
        if (!hydrated?.thread.daemonThreadId) {
          continue;
        }

        const daemonThreadId = hydrated.thread.daemonThreadId;
        if (seenDaemonThreadIds.has(daemonThreadId)) {
          continue;
        }
        seenDaemonThreadIds.add(daemonThreadId);

        const existing = existingByDaemonThreadId.get(daemonThreadId);
        if (existing) {
          const merged = {
            ...existing,
            ...hydrated.thread,
            id: existing.id,
            workspaceId: existing.workspaceId,
            surfaceId: existing.surfaceId,
            paneId: existing.paneId,
          };
          nextThreads.push({
            ...merged,
            ...mergeRemoteThreadProfile(existing, merged),
          });
          if (!(existing.id in nextMessages)) {
            nextMessages[existing.id] = hydrated.messages.map((message) => ({
              ...message,
              threadId: existing.id,
            }));
          }
          if (!(existing.id in nextTodos)) {
            nextTodos[existing.id] = [];
          }
          continue;
        }

        nextThreads.push(hydrated.thread);
        nextMessages[hydrated.thread.id] = hydrated.messages;
        nextTodos[hydrated.thread.id] = [];
      }

      for (const localThread of state.threads) {
        if (!localThread.daemonThreadId) {
          nextThreads.push(localThread);
        }
      }
      const activeThread = state.threads.find((thread) => thread.id === state.activeThreadId);
      if (
        activeThread?.daemonThreadId
        && !seenDaemonThreadIds.has(activeThread.daemonThreadId)
        && !nextThreads.some((thread) => thread.id === activeThread.id)
      ) {
        nextThreads.push(activeThread);
      }

      nextThreads.sort((left, right) => right.updatedAt - left.updatedAt);
      const validThreadIds = new Set(nextThreads.map((thread) => thread.id));

      for (const threadId of Object.keys(nextMessages)) {
        if (!validThreadIds.has(threadId)) {
          delete nextMessages[threadId];
        }
      }
      for (const threadId of Object.keys(nextTodos)) {
        if (!validThreadIds.has(threadId)) {
          delete nextTodos[threadId];
        }
      }

      return {
        threads: nextThreads,
        messages: nextMessages,
        todos: nextTodos,
        activeThreadId:
          state.activeThreadId && validThreadIds.has(state.activeThreadId)
            ? state.activeThreadId
            : null,
        threadHistoryStack: state.threadHistoryStack.filter((threadId) =>
          validThreadIds.has(threadId),
        ),
      };
    });
  }, []);

  const exportThread = useCallback(async (messageId: string) => {
    const notify = useNotificationStore.getState().addNotification;
    const daemonThreadId = activeThread?.daemonThreadId ?? null;
    if (!daemonThreadId) {
      notify({ source: "system", title: "Export unavailable", body: "This thread is not saved yet." });
      return;
    }
    const api = getAgentDbApi();
    if (!api?.dbExportThread) {
      notify({ source: "system", title: "Export unavailable", body: "Export is not supported by this build." });
      return;
    }
    const result = await api
      .dbExportThread(daemonThreadId, messageId)
      .catch((error) => ({ ok: false, file_path: null, error: String(error) }));
    if (result?.ok && result.file_path) {
      notify({ source: "system", title: "Thread exported", body: `Saved to ${result.file_path}` });
    } else {
      notify({ source: "system", title: "Export failed", body: result?.error || "Unknown error." });
    }
  }, [activeThread]);

  const fetchThreadList = useCallback(async (options?: { agentFilter?: string | null; includeInternal?: boolean }) => {
    const zorai = getAgentBridge();
    if (!zorai?.agentListThreads) {
      return [];
    }
    return fetchHydratedRemoteThreads({
      agentListThreads: zorai.agentListThreads,
      fallbackAgentName: useAgentStore.getState().agentSettings.agent_name,
      agentFilter: options?.agentFilter ?? null,
      includeInternal: options?.includeInternal === true,
      existingThreads: useAgentStore.getState().threads,
    });
  }, []);

  const loadThreadPage = useCallback((
    threadId: string,
    direction: "latest" | "older",
  ): Promise<boolean> => {
    const replaceEpoch = direction === "latest" ? threadHistoryReplaceEpoch(threadId) : undefined;
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
    const trackedThreadIds = [threadId, thread?.daemonThreadId]
      .map((id) => id?.trim())
      .filter((id): id is string => Boolean(id));
    const uniqueTrackedThreadIds = [...new Set(trackedThreadIds)];
    const finishLoading = direction === "latest"
      && thread?.daemonThreadId
      && getAgentBridge()?.agentGetThread
      ? beginThreadLoadingFor(uniqueTrackedThreadIds)
      : () => {};
    const runThreadPageLoad = async (): Promise<boolean> => {
      try {
      const currentThread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
      const daemonThreadId = currentThread?.daemonThreadId;
      if (!daemonThreadId || !getAgentBridge()?.agentGetThread) {
        return false;
      }
      const messageLimit = resolveReactChatHistoryMessageLimit(agentSettings.react_chat_history_page_size) ?? null;
      if (direction === "latest") {
        return loadDaemonThreadPageIntoLocalState({
          daemonThreadId,
          localThreadId: threadId,
          messageLimit,
          messageOffset: 0,
          mergeMode: "replace",
          replaceEpoch,
          setThreadTodos,
          setDaemonTodosByThread,
        });
      }

      const currentMessages = useAgentStore.getState().messages[threadId] ?? [];
      const messageOffset = resolveOlderThreadPageMessageOffset({
        loadedMessageStart: currentThread?.loadedMessageStart,
        loadedMessageEnd: currentThread?.loadedMessageEnd,
        messageCount: currentThread?.messageCount,
        currentMessageCount: currentMessages.length,
      });
      if (messageOffset === null) {
        return false;
      }

      return loadDaemonThreadPageIntoLocalState({
        daemonThreadId,
        localThreadId: threadId,
        messageLimit,
        messageOffset,
        mergeMode: "prepend",
        setThreadTodos,
        setDaemonTodosByThread,
      });
      } finally {
        finishLoading();
      }
    };

    return runThreadPageLoad();
  }, [agentSettings.react_chat_history_page_size, setThreadTodos]);

  const openThread = useCallback((threadId: string) => {
    const state = useAgentStore.getState();
    const thread = findThreadByAuthoritativeIdentity(state.threads, threadId);
    const localId = thread?.id ?? threadId;
    // Mark this thread as "latest loaded" BEFORE issuing the fetch. The
    // activeThread effect below also triggers loadThreadPage for newly
    // selected daemon threads; without this guard the two concurrent
    // "latest" loads bump each other's replace epoch and the loser is
    // discarded, leaving the chat pane empty ("click a thread, nothing
    // happens" until you click again).
    const alreadyLatest = latestLoadedThreadIdRef.current === localId;
    latestLoadedThreadIdRef.current = localId;
    daemonLocalThreadRef.current = localId;
    daemonThreadIdRef.current = thread?.daemonThreadId ?? null;
    setFollowThreadHistoryBottom(true);
    setActiveThread(localId);
    setChatBackView("threads");
    setView("chat");
    const loadedCount = useAgentStore.getState().messages[localId]?.length ?? 0;
    const needsHistoryWindow = Boolean(
      thread?.daemonThreadId
      && (
        (thread.messageCount ?? 0) > loadedCount
        || ((thread.loadedMessageStart ?? 0) > 0 && loadedCount === 0)
      ),
    );
    if (!alreadyLatest || needsHistoryWindow) {
      void loadThreadPage(localId, "latest");
    }
  }, [loadThreadPage, setActiveThread, setChatBackView, setFollowThreadHistoryBottom, setView]);

  const forkThread = useCallback(async (messageId: string) => {
    const notify = useNotificationStore.getState().addNotification;
    const daemonThreadId = resolveDaemonOwnedThreadId({
      threads: useAgentStore.getState().threads,
      threadId: activeThreadId ?? "",
      activeThreadId,
      activeDaemonThreadId: daemonThreadIdRef.current,
    });
    if (!daemonThreadId) {
      notify({ source: "system", title: "Fork unavailable", body: "This thread is not saved yet." });
      return;
    }
    const api = getAgentDbApi();
    if (!api?.dbForkThread) {
      notify({ source: "system", title: "Fork unavailable", body: "Fork is not supported by this build." });
      return;
    }
    const result = await api
      .dbForkThread(daemonThreadId, messageId)
      .catch((error) => ({ ok: false, thread_id: null, title: null, error: String(error) }));
    if (result?.ok && result.thread_id) {
      await refreshThreadList();
      let local = findLocalThreadByDaemonThreadId(useAgentStore.getState().threads, result.thread_id);
      if (!local) {
        const localId = storeCreateThread({ title: result.title || "Forked thread" });
        setThreadDaemonId(localId, result.thread_id);
        local = useAgentStore.getState().threads.find((thread) => thread.id === localId);
      }
      if (local) {
        openThread(local.id);
      }
      notify({ source: "system", title: "Thread forked", body: result.title || "Forked thread created." });
    } else {
      notify({ source: "system", title: "Fork failed", body: result?.error || "Unknown error." });
    }
  }, [activeThreadId, openThread, refreshThreadList, setThreadDaemonId, storeCreateThread]);

  useEffect(() => {
    if (!activeThreadId || latestLoadedThreadIdRef.current === activeThreadId) {
      return;
    }
    const daemonThreadId = activeThread?.daemonThreadId;
    if (!daemonThreadId) {
      return;
    }
    latestLoadedThreadIdRef.current = activeThreadId;
    setFollowThreadHistoryBottom(true);
    void loadThreadPage(activeThreadId, "latest");
  }, [activeThread?.daemonThreadId, activeThreadId, loadThreadPage]);

  const loadOlderThreadMessages = useCallback(async () => {
    if (!activeThreadId) return false;
    return loadThreadPage(activeThreadId, "older");
  }, [activeThreadId, loadThreadPage]);

  const trimThreadMessagesToLatestWindow = useCallback((threadId?: string | null) => {
    const localThreadId = threadId ?? activeThreadId;
    if (!localThreadId) return false;
    const messageLimit = resolveReactChatHistoryMessageLimit(agentSettings.react_chat_history_page_size);
    return trimDaemonThreadMessagesToLatestWindow({
      localThreadId,
      messageLimit,
    });
  }, [activeThreadId, agentSettings.react_chat_history_page_size]);

  const reloadCollaborationThread = useCallback(async (daemonThreadId: string) => {
    await reloadDaemonThreadIntoLocalState({
      daemonThreadId,
      setThreadTodos,
      setDaemonTodosByThread,
    });
  }, [setThreadTodos]);

  const sendMessage = useCallback((payload: { text: string; contentBlocksJson?: string | null; localContentBlocks?: import("@/lib/agentStore/types").AgentContentBlock[] }) => {
    if (!payload.text.trim() && !payload.contentBlocksJson) return;
    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && sendDaemonMessage(payload)) {
      return;
    }
    sendMessageLegacy(payload.text);
  }, [agentSettings.agent_backend, sendDaemonMessage, sendMessageLegacy]);

  const spawnSubagent = useCallback(async (request: { title: string; description: string; cwd?: string | null }) => {
    if (isLeadOnlyPersona(request.title)) {
      return { ok: false, error: LEAD_PERSONA_SPAWN_ERROR };
    }
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === activeThreadId);
    const daemonThreadId = thread?.daemonThreadId ?? daemonThreadIdRef.current;
    const bridge = getAgentBridge();
    if (!daemonThreadId) return { ok: false, error: "Send the first message before delegating to a subagent." };
    if (!bridge?.agentSpawnSubagent) return { ok: false, error: "Subagent delegation bridge is unavailable." };
    try {
      return await bridge.agentSpawnSubagent(daemonThreadId, request);
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : "Subagent delegation failed." };
    }
  }, [activeThreadId]);

  const collaborationActions = useMemo(() => createThreadCollaborationActions({
    getActiveDaemonThread: () => {
      const state = useAgentStore.getState();
      const localThreadId = state.activeThreadId;
      const thread = localThreadId
        ? state.threads.find((entry) => entry.id === localThreadId)
        : undefined;
      const daemonThreadId = thread?.daemonThreadId ?? null;
      return localThreadId && daemonThreadId ? { localThreadId, daemonThreadId } : null;
    },
    getBridge: getAgentBridge,
    reloadThread: reloadCollaborationThread,
    stopStreaming,
  }), [reloadCollaborationThread, stopStreaming]);

  const {
    pushHandoff,
    returnHandoff,
    upsertParticipant,
    deactivateParticipant,
    getOperationStatus,
    cancelOperation,
    sendParticipantSuggestion,
    dismissParticipantSuggestion,
  } = collaborationActions;

  const deleteMessage = useCallback((threadId: string, messageId: string) => {
    const daemonThreadId = resolveDaemonOwnedThreadId({
      threads: useAgentStore.getState().threads,
      threadId,
      activeThreadId,
      activeDaemonThreadId: daemonThreadIdRef.current,
    });
    deleteMessageFromStore(threadId, messageId);
    if (!shouldUseDaemonRuntime(agentSettings.agent_backend) || !daemonThreadId) {
      return;
    }
    void getAgentDbApi()?.dbDeleteMessage?.(daemonThreadId, messageId);
  }, [activeThreadId, agentSettings.agent_backend, deleteMessageFromStore]);

  const pinMessageForCompaction = useCallback(async (threadId: string, messageId: string) => {
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
    const daemonThreadId = thread?.daemonThreadId ?? (threadId === activeThreadId ? daemonThreadIdRef.current : null);
    const zorai = getAgentBridge();

    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && daemonThreadId && zorai?.agentPinThreadMessageForCompaction) {
      const result = await zorai.agentPinThreadMessageForCompaction(daemonThreadId, messageId) as ZoraiThreadMessagePinResult;
      if (result?.ok) {
        await reloadDaemonThreadIntoLocalState({
          daemonThreadId,
          setThreadTodos,
          setDaemonTodosByThread,
        });
      }
      return result;
    }

    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        [threadId]: (state.messages[threadId] ?? []).map((message) =>
          message.id === messageId ? { ...message, pinnedForCompaction: true } : message),
      },
    }));
    return {
      ok: true,
      thread_id: threadId,
      message_id: messageId,
      current_pinned_chars: 0,
      pinned_budget_chars: 0,
    } satisfies ZoraiThreadMessagePinResult;
  }, [activeThreadId, agentSettings.agent_backend, setThreadTodos]);

  const unpinMessageForCompaction = useCallback(async (threadId: string, messageId: string) => {
    const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
    const daemonThreadId = thread?.daemonThreadId ?? (threadId === activeThreadId ? daemonThreadIdRef.current : null);
    const zorai = getAgentBridge();

    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && daemonThreadId && zorai?.agentUnpinThreadMessageForCompaction) {
      const result = await zorai.agentUnpinThreadMessageForCompaction(daemonThreadId, messageId) as ZoraiThreadMessagePinResult;
      if (result?.ok) {
        await reloadDaemonThreadIntoLocalState({
          daemonThreadId,
          setThreadTodos,
          setDaemonTodosByThread,
        });
      }
      return result;
    }

    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        [threadId]: (state.messages[threadId] ?? []).map((message) =>
          message.id === messageId ? { ...message, pinnedForCompaction: false } : message),
      },
    }));
    return {
      ok: true,
      thread_id: threadId,
      message_id: messageId,
      current_pinned_chars: 0,
      pinned_budget_chars: 0,
    } satisfies ZoraiThreadMessagePinResult;
  }, [activeThreadId, agentSettings.agent_backend, setThreadTodos]);

  const submitMessageFeedback = useCallback(async (threadId: string, messageId: string, reaction: "up" | "down" | null) => {
    const currentState = useAgentStore.getState();
    const currentMessage = (currentState.messages[threadId] ?? [])
      .find((message) => message.id === messageId);
    if (!currentMessage || currentMessage.isStreaming === true) {
      return;
    }
    const thread = currentState.threads.find((entry) => entry.id === threadId);
    const daemonThreadId = thread?.daemonThreadId ?? (threadId === activeThreadId ? daemonThreadIdRef.current : null);
    const absoluteMessageIndex = resolveAbsoluteMessageIndex(
      thread?.loadedMessageStart,
      currentState.messages[threadId] ?? [],
      messageId,
    );
    const zorai = getAgentBridge();

    // Optimistic local update so the UI shows the reaction instantly. The
    // daemon broadcasts the resolved state back, which will overwrite this
    // if it disagrees (it should not).
    useAgentStore.setState((state) => ({
      messages: {
        ...state.messages,
        [threadId]: (state.messages[threadId] ?? []).map((message) =>
          message.id === messageId ? { ...message, feedback: reaction } : message),
      },
    }));

    if (shouldUseDaemonRuntime(agentSettings.agent_backend) && daemonThreadId && zorai?.agentMessageFeedback) {
      try {
        await zorai.agentMessageFeedback(daemonThreadId, messageId, reaction, absoluteMessageIndex);
      } catch (error) {
        console.warn("agentMessageFeedback failed", error);
      }
    }
  }, [activeThreadId, agentSettings.agent_backend]);

  const handleSend = useCallback(() => {
    const text = useComposerDraftStore.getState().input.trim();
    if (!text) return;
    setFollowThreadHistoryBottom(true);
    sendMessage({ text });
    writeComposerDraftInput("");
  }, [sendMessage]);

  const handleKeyDown = useCallback((event: React.KeyboardEvent) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      handleSend();
    }
  }, [handleSend]);

  const pinnedMessages = useMemo(
    () => messages.filter((message) => message.pinnedForCompaction),
    [messages],
  );
  const pinnedUsageChars = useMemo(
    () => sumMessageContentChars(pinnedMessages),
    [pinnedMessages],
  );
  const pinnedBudgetChars = useMemo(() => {
    const activeProviderId = (agentSettings as { active_provider?: keyof typeof agentSettings }).active_provider;
    const activeProvider = activeProviderId
      ? agentSettings[activeProviderId] as { context_window_tokens?: number | null } | undefined
      : undefined;
    const contextWindowTokens = Number(activeProvider?.context_window_tokens ?? agentSettings.context_window_tokens ?? 0);
    return pinnedMessageBudgetChars(contextWindowTokens);
  }, [agentSettings]);
  const pinnedOverBudget = pinnedUsageChars > pinnedBudgetChars;

  const tabItems = buildAgentChatPanelTabItems({
    threads,
    pinnedMessageCount: pinnedMessages.length,
    scopedCognitiveEventCount: scopedCognitiveEvents.length,
    usageMessageCount,
  });

  const value = useMemo<AgentChatPanelRuntimeValue>(() => ({
    togglePanel,
    activeWorkspace,
    threads,
    activeThread,
    activeThreadId,
    createThread,
    deleteThread,
    setActiveThread,
    openThread,
    agentSettings,
    updateAgentSetting,
    searchQuery,
    setSearchQuery,
    refreshThreadList,
    fetchThreadList,
    loadOlderThreadMessages,
    trimThreadMessagesToLatestWindow,
    messages,
    todos,
    daemonTodosByThread,
    spawnedAgentTree: spawnedAgentNavigation.tree,
    canGoBackThread: spawnedAgentNavigation.canGoBackThread,
    goBackThread,
    canOpenSpawnedThread,
    openSpawnedThread,
    threadNavigationDepth: spawnedAgentNavigation.threadNavigationDepth,
    backThreadTitle: spawnedAgentNavigation.backThreadTitle,
    goalRunsForTrace,
    allMessagesByThread,
    pendingApprovals,
    scopedOperationalEvents,
    scopedCognitiveEvents,
    latestContextSnapshot,
    memory,
    updateMemory,
    historySummary,
    historyHits,
    symbolHits,
    snippets,
    transcripts,
    scopePaneId,
    scopeController,
    historyQuery,
    setHistoryQuery,
    symbolQuery,
    setSymbolQuery,
    view,
    setView,
    chatBackView,
    setChatBackView,
    usageMessageCount,
    filteredThreads,
    isStreamingResponse,
    messagesEndRef,
    inputRef,
    sendMessage,
    spawnSubagent,
    pushHandoff,
    returnHandoff,
    upsertParticipant,
    deactivateParticipant,
    getOperationStatus,
    cancelOperation,
    sendParticipantSuggestion,
    dismissParticipantSuggestion,
    deleteMessage,
    forkThread,
    exportThread,
    pinMessageForCompaction,
    unpinMessageForCompaction,
    submitMessageFeedback,
    stopStreaming,
    retryStreamNow,
    handleSend,
    handleKeyDown,
    builtinAgentSetup,
    canStartGoalRun,
    cancelBuiltinAgentSetup,
    startGoalRunFromPrompt,
    submitBuiltinAgentSetup,
    tabItems,
    pinnedMessages,
    pinnedBudgetChars,
    pinnedUsageChars,
    pinnedOverBudget,
    welesHealth,
  }), [
    activeThread,
    activeThreadId,
    activeWorkspace,
    agentSettings,
    updateAgentSetting,
    allMessagesByThread,
    chatBackView,
    builtinAgentSetup,
    canStartGoalRun,
    cancelBuiltinAgentSetup,
    startGoalRunFromPrompt,
    daemonTodosByThread,
    deleteMessage,
    forkThread,
    exportThread,
    deleteThread,
    dismissParticipantSuggestion,
    filteredThreads,
    goalRunsForTrace,
    goBackThread,
    handleSend,
    handleKeyDown,
    historyHits,
    historyQuery,
    historySummary,
    isStreamingResponse,
    latestContextSnapshot,
    memory,
    messages,
    pendingApprovals,
    spawnedAgentNavigation,
    pinMessageForCompaction,
    pinnedBudgetChars,
    pinnedMessages,
    pinnedOverBudget,
    pinnedUsageChars,
    submitMessageFeedback,
    scopeController,
    scopePaneId,
    scopedCognitiveEvents,
    scopedOperationalEvents,
    searchQuery,
    canOpenSpawnedThread,
    openSpawnedThread,
    loadOlderThreadMessages,
    trimThreadMessagesToLatestWindow,
    openThread,
    refreshThreadList,
    fetchThreadList,
    setActiveThread,
    setSearchQuery,
    snippets,
    stopStreaming,
    retryStreamNow,
    submitBuiltinAgentSetup,
    symbolHits,
    symbolQuery,
    tabItems,
    threads,
    todos,
    togglePanel,
    transcripts,
    unpinMessageForCompaction,
    updateMemory,
    usageMessageCount,
    view,
    welesHealth,
    createThread,
    sendMessage,
    spawnSubagent,
    pushHandoff,
    returnHandoff,
    upsertParticipant,
    deactivateParticipant,
    getOperationStatus,
    cancelOperation,
    sendParticipantSuggestion,
  ]);

  return { isOpen, value };
}
