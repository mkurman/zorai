import { scheduleJsonWrite } from "../persistence";
import {
  AGENT_ACTIVE_THREAD_FILE,
  getAgentDbApi,
  nextMessageId,
  nextThreadId,
  persistDaemonThreadMap,
  serializeMessage,
  serializeThread,
  shouldPersistHistory,
} from "./history";
import { normalizeAgentProviderId } from "./providers";
import type { AgentSettings } from "./settings";
import type { AgentState, AgentStoreGet, AgentStoreSet } from "./storeTypes";
import { applyRendererMessageBudget, boundContentBlocks, boundOptionalRendererText, boundRendererText } from "./rendererText";
import type { AgentMessage } from "./types";

type ThreadActionKeys =
  | "createThread"
  | "deleteThread"
  | "setActiveThread"
  | "openSpawnedThread"
  | "goBackThread"
  | "searchThreads"
  | "addMessage"
  | "updateLastAssistantMessage"
  | "getThreadMessages"
  | "deleteMessage"
  | "setThreadTodos"
  | "getThreadTodos"
  | "setThreadDaemonId"
  | "setThreadOwner"
  | "updateThreadTitle"
  | "toggleAgentPanel"
  | "setSearchQuery"
  | "getThreadsForPane";

function shouldPersistCurrentHistory(agentSettings: AgentSettings): boolean {
  return shouldPersistHistory(agentSettings.agent_backend);
}

function persistActiveThreadSelection(get: AgentStoreGet, activeThreadId: string | null): void {
  if (shouldPersistCurrentHistory(get().agentSettings)) {
    scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId });
  }
}

function appendThreadHistory(stack: string[], fromThreadId: string): string[] {
  if (stack[stack.length - 1] === fromThreadId) {
    return stack;
  }
  return [...stack, fromThreadId];
}

function popPreviousThread(
  currentActiveThreadId: string | null,
  threads: AgentState["threads"],
  stack: string[],
): { activeThreadId: string | null; threadHistoryStack: string[] } {
  const nextStack = [...stack];

  while (nextStack.length > 0) {
    const nextThreadId = nextStack.pop();
    if (!nextThreadId) {
      continue;
    }
    if (threads.some((thread) => thread.id === nextThreadId)) {
      return {
        activeThreadId: nextThreadId,
        threadHistoryStack: nextStack,
      };
    }
  }

  return { activeThreadId: currentActiveThreadId, threadHistoryStack: [] };
}

export function createThreadActions(
  set: AgentStoreSet,
  get: AgentStoreGet,
): Pick<AgentState, ThreadActionKeys> {
  return {
    createThread: (opts) => {
      const id = nextThreadId();
      const now = Date.now();
      const thread = {
        id,
        daemonThreadId: null,
        workspaceId: opts.workspaceId ?? null,
        surfaceId: opts.surfaceId ?? null,
        paneId: opts.paneId ?? null,
        agent_name: opts.agentName?.trim() || get().agentSettings.agent_name,
        targetAgentId: opts.agentId?.trim() || null,
        title: opts.title ?? "New Conversation",
        createdAt: now,
        updatedAt: now,
        messageCount: 0,
        totalInputTokens: 0,
        totalOutputTokens: 0,
        totalTokens: 0,
        compactionCount: 0,
        lastMessagePreview: "",
        upstreamThreadId: null,
        upstreamTransport: undefined,
        upstreamProvider: null,
        upstreamModel: null,
        upstreamAssistantId: null,
        profileProvider: opts.profileProvider?.trim() || null,
        profileModel: opts.profileModel?.trim() || null,
        profileReasoningEffort: opts.profileReasoningEffort?.trim() || null,
        profileContextWindowTokens: typeof opts.profileContextWindowTokens === "number"
          && opts.profileContextWindowTokens > 0
          ? Math.trunc(opts.profileContextWindowTokens)
          : null,
      };
      set((state) => {
        const activate = opts.activate !== false;
        const next = {
          threads: [thread, ...state.threads],
          messages: { ...state.messages, [id]: [] },
          todos: { ...state.todos, [id]: [] },
          activeThreadId: activate ? id : state.activeThreadId,
          threadHistoryStack: activate ? [] : state.threadHistoryStack,
        };
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(next.threads);
          scheduleJsonWrite(AGENT_ACTIVE_THREAD_FILE, { activeThreadId: next.activeThreadId });
          void getAgentDbApi()?.dbCreateThread?.(serializeThread(thread));
        }
        return next;
      });
      return id;
    },
    deleteThread: (id) => {
      set((state) => {
        const { [id]: _message, ...remainingMessages } = state.messages;
        const { [id]: _todo, ...remainingTodos } = state.todos;
        const next = {
          threads: state.threads.filter((thread) => thread.id !== id),
          messages: remainingMessages,
          todos: remainingTodos,
          activeThreadId: state.activeThreadId === id ? null : state.activeThreadId,
        };
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(next.threads);
          void getAgentDbApi()?.dbDeleteThread?.(id);
        }
        return next;
      });
    },
    setActiveThread: (id) => {
      set({ activeThreadId: id, threadHistoryStack: [] });
      persistActiveThreadSelection(get, id);
    },
    openSpawnedThread: (fromThreadId, toThreadId) => {
      const state = get();
      if (fromThreadId === toThreadId || !state.threads.some((thread) => thread.id === toThreadId)) {
        return;
      }

      set({
        activeThreadId: toThreadId,
        threadHistoryStack: appendThreadHistory(state.threadHistoryStack, fromThreadId),
      });
      persistActiveThreadSelection(get, toThreadId);
    },
    goBackThread: () => {
      const state = get();
      if (state.threadHistoryStack.length === 0) {
        return;
      }

      const next = popPreviousThread(
        state.activeThreadId,
        state.threads,
        state.threadHistoryStack,
      );
      set(next);
      persistActiveThreadSelection(get, next.activeThreadId);
    },
    searchThreads: (query) => {
      const lower = query.toLowerCase();
      return get().threads.filter((thread) =>
        thread.title.toLowerCase().includes(lower)
        || thread.lastMessagePreview.toLowerCase().includes(lower)
        || thread.agent_name.toLowerCase().includes(lower));
    },
    addMessage: (threadId, message) => {
      const fullMessage: AgentMessage = {
        ...message,
        content: boundRendererText(message.content),
        contentBlocks: boundContentBlocks(message.contentBlocks),
        compactionPayload: typeof message.compactionPayload === "string" ? boundRendererText(message.compactionPayload) : message.compactionPayload,
        toolArguments: typeof message.toolArguments === "string" ? boundRendererText(message.toolArguments) : message.toolArguments,
        reasoning: typeof message.reasoning === "string" ? boundRendererText(message.reasoning) : message.reasoning,
        id: nextMessageId(),
        threadId,
        createdAt: Date.now(),
      };
      set((state) => {
        const threadMessages = [...(state.messages[threadId] ?? []), fullMessage];
        const next = {
          messages: { ...state.messages, [threadId]: threadMessages },
          todos: state.todos,
          threads: state.threads.map((thread) =>
            thread.id === threadId
              ? {
                ...thread,
                messageCount: thread.messageCount + 1,
                updatedAt: Date.now(),
                totalInputTokens: thread.totalInputTokens + message.inputTokens,
                totalOutputTokens: thread.totalOutputTokens + message.outputTokens,
                totalTokens: thread.totalTokens + message.totalTokens,
                lastMessagePreview: message.content.slice(0, 100),
              }
              : thread),
          activeThreadId: state.activeThreadId,
        };
        const updatedThread = next.threads.find((thread) => thread.id === threadId);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          void (async () => {
            const api = getAgentDbApi();
            if (updatedThread) {
              await api?.dbCreateThread?.(serializeThread(updatedThread));
            }
            await api?.dbAddMessage?.(serializeMessage(fullMessage));
          })();
        }
        return {
          ...next,
          messages: applyRendererMessageBudget(next.messages, next.activeThreadId),
        };
      });
    },
    updateLastAssistantMessage: (threadId, content, streaming, meta) => {
      set((state) => {
        const messages = state.messages[threadId];
        if (!messages || messages.length === 0) {
          return state;
        }
        const lastMessage = messages[messages.length - 1];
        if (lastMessage.role !== "assistant") {
          return state;
        }
        const nextInputTokens = meta?.inputTokens ?? lastMessage.inputTokens;
        const nextOutputTokens = meta?.outputTokens ?? lastMessage.outputTokens;
        const nextTotalTokens = meta?.totalTokens ?? lastMessage.totalTokens;
        const updatedLastMessage: AgentMessage = {
          ...lastMessage,
          content: boundRendererText(content),
          isStreaming: streaming ?? false,
          inputTokens: nextInputTokens,
          outputTokens: nextOutputTokens,
          totalTokens: nextTotalTokens,
          reasoning: boundOptionalRendererText(meta?.reasoning ?? lastMessage.reasoning),
          reasoningTokens: meta?.reasoningTokens ?? lastMessage.reasoningTokens,
          audioTokens: meta?.audioTokens ?? lastMessage.audioTokens,
          videoTokens: meta?.videoTokens ?? lastMessage.videoTokens,
          cost: meta?.cost ?? lastMessage.cost,
          tps: meta?.tps ?? lastMessage.tps,
          toolCalls: meta?.toolCalls ?? lastMessage.toolCalls,
          provider: meta?.provider ?? lastMessage.provider,
          model: meta?.model ?? lastMessage.model,
          api_transport: meta?.api_transport ?? lastMessage.api_transport,
          responseId: meta?.responseId ?? lastMessage.responseId,
          providerFinalResult:
            meta?.providerFinalResult ?? lastMessage.providerFinalResult,
        };
        const updatedMessages = [...messages.slice(0, -1), updatedLastMessage];
        const tokenDeltaIn = nextInputTokens - lastMessage.inputTokens;
        const tokenDeltaOut = nextOutputTokens - lastMessage.outputTokens;
        const tokenDeltaTotal = nextTotalTokens - lastMessage.totalTokens;
        const previousCostKnown = typeof lastMessage.cost === "number" && Number.isFinite(lastMessage.cost);
        const nextCostKnown = typeof updatedLastMessage.cost === "number" && Number.isFinite(updatedLastMessage.cost);
        const previousCost = previousCostKnown ? lastMessage.cost as number : 0;
        const nextCost = nextCostKnown ? updatedLastMessage.cost as number : 0;
        const costDelta = nextCost - previousCost;
        const nextThreads = state.threads.map((thread) =>
          thread.id === threadId
            ? {
              ...thread,
              totalInputTokens: thread.totalInputTokens + tokenDeltaIn,
              totalOutputTokens: thread.totalOutputTokens + tokenDeltaOut,
              totalTokens: thread.totalTokens + tokenDeltaTotal,
              totalCostUsd: nextCostKnown && (!previousCostKnown || costDelta !== 0)
                ? (thread.totalCostUsd ?? 0) + costDelta
                : thread.totalCostUsd,
              updatedAt: Date.now(),
              lastMessagePreview: content.slice(0, 100),
            }
            : thread);
        const updatedThread = nextThreads.find((thread) => thread.id === threadId);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          void (async () => {
            const api = getAgentDbApi();
            if (updatedThread) {
              await api?.dbCreateThread?.(serializeThread(updatedThread));
            }
            await api?.dbAddMessage?.(serializeMessage(updatedLastMessage));
          })();
        }
        return { messages: { ...state.messages, [threadId]: updatedMessages }, threads: nextThreads };
      });
    },
    getThreadMessages: (threadId) => get().messages[threadId] ?? [],
    deleteMessage: (threadId, messageId) => {
      set((state) => {
        const messages = state.messages[threadId];
        if (!messages) {
          return state;
        }
        const filtered = messages.filter((message) => message.id !== messageId);
        if (filtered.length === messages.length) {
          return state;
        }
        return {
          messages: { ...state.messages, [threadId]: filtered },
          threads: state.threads.map((thread) =>
            thread.id === threadId
              ? { ...thread, messageCount: Math.max(0, thread.messageCount - 1), updatedAt: Date.now() }
              : thread),
        };
      });
    },
    setThreadTodos: (threadId, todos) => {
      set((state) => ({
        todos: { ...state.todos, [threadId]: [...todos].sort((left, right) => left.position - right.position) },
      }));
    },
    getThreadTodos: (threadId) => get().todos[threadId] ?? [],
    setThreadDaemonId: (threadId, daemonThreadId) => {
      set((state) => {
        const threads = state.threads.map((thread) =>
          thread.id === threadId ? { ...thread, daemonThreadId } : thread);
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(threads);
        }
        return { threads };
      });
    },
    setThreadOwner: (threadId, owner) => {
      const agentId = owner.agentId.trim();
      const agentName = owner.agentName.trim() || agentId;
      if (!agentId) {
        return;
      }
      set((state) => {
        let updatedThread = null as AgentState["threads"][number] | null;
        const threads = state.threads.map((thread) => {
          if (thread.id !== threadId) {
            return thread;
          }
          updatedThread = {
            ...thread,
            agent_name: agentName,
            targetAgentId: agentId,
            updatedAt: Date.now(),
            ...(thread.daemonThreadId
              ? {}
              : {
                profileProvider: null,
                profileModel: null,
                profileReasoningEffort: null,
                profileContextWindowTokens: null,
              }),
          };
          return updatedThread;
        });
        if (!updatedThread) {
          return state;
        }
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(threads);
          void getAgentDbApi()?.dbCreateThread?.(serializeThread(updatedThread));
        }
        return { threads };
      });
    },
    updateThreadTitle: (threadId, title) => {
      const nextTitle = title.trim();
      if (!nextTitle) {
        return;
      }
      set((state) => {
        let updatedThread = null as AgentState["threads"][number] | null;
        const threads = state.threads.map((thread) => {
          if (thread.id !== threadId && thread.daemonThreadId !== threadId) {
            return thread;
          }
          updatedThread = { ...thread, title: nextTitle, updatedAt: Date.now() };
          return updatedThread;
        });
        if (!updatedThread) {
          return state;
        }
        if (shouldPersistCurrentHistory(get().agentSettings)) {
          persistDaemonThreadMap(threads);
          void getAgentDbApi()?.dbCreateThread?.(serializeThread(updatedThread));
        }
        return { threads };
      });
    },
    toggleAgentPanel: () => set((state) => ({ agentPanelOpen: !state.agentPanelOpen })),
    setSearchQuery: (query) => set({ searchQuery: query }),
    getThreadsForPane: (paneId) => get().threads.filter((thread) => thread.paneId === paneId),
  };
}

export { normalizeAgentProviderId };
