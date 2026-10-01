import { useEffect } from "react";
import { useAgentStore } from "@/lib/agentStore";
import { shouldUseDaemonRuntime, getAgentBridge } from "@/lib/agentDaemonConfig";
import { fetchAllThreadTodos, fetchThreadTodos } from "@/lib/agentTodos";
import { fetchGoalRuns, type GoalRun } from "@/lib/goalRuns";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import type { RiskLevel } from "@/lib/agent-mission-store/types";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { AgentBackend } from "@/lib/agentStore/types";
import type { AgentMessage, AgentThread, AgentTodoItem } from "@/lib/agentStore";
import type { WelesHealthState } from "@/lib/agentStore/types";
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import {
  appendDaemonSystemMessage,
  recordDaemonWorkflowNotice,
  refreshDaemonThreadMessagesIntoLocalState,
  reloadDaemonThreadIntoLocalState,
  syncWelesHealth,
  trimFollowedThreadToHistoryWindow,
} from "./daemonHelpers";
import {
  handleDivergentStartEvent,
  handleGatewayIncomingEvent,
  handleGoalRunEvent,
  handleOperatorProfileWarning,
  handlePayloadMessageEvent,
  handleTaskUpdateEvent,
  handleThreadCreatedEvent,
  handleThreadTitleUpdatedEvent,
  handleTodoUpdateEvent,
  handleWorkspaceCommand,
} from "./daemonEventHandlers";
import { finalizeStreamingAssistantMessages } from "./threadTurnState";
import { completeThreadStopBarrier, hasThreadStopBarrier } from "./threadStopBarrier";
import {
  consumeDeferredThreadReload,
  deferThreadReload,
  shouldDeferThreadReload,
} from "./deferredThreadReload";
import {
  applyDaemonRetryStatusEvent,
  clearThreadRetryStatus,
  releaseSuppressedThreadRetryStatus,
} from "@/zorai/features/threads/threadRetryStatus";

export function resolveDaemonEventLocalThreadId(
  event: any,
  fallbackLocalThreadId: string | null,
  fallbackDaemonThreadId: string | null,
  options: { allowThreadlessFallback?: boolean } = {},
): string | null {
  const daemonThreadId = typeof event?.thread_id === "string" ? event.thread_id : null;
  if (!daemonThreadId) {
    if (options.allowThreadlessFallback === false) {
      return null;
    }
    return fallbackLocalThreadId;
  }

  const matchingThread = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === daemonThreadId);
  if (matchingThread) {
    return matchingThread.id;
  }

  return daemonThreadId === fallbackDaemonThreadId ? fallbackLocalThreadId : null;
}

const THREADLESS_STREAM_EVENT_TYPES = new Set([
  "delta",
  "reasoning",
  "done",
  "turn_interrupted",
  "tool_call",
  "tool_result",
  "error",
]);

function normalizeApprovalRiskLevel(value: unknown): RiskLevel {
  return value === "high" || value === "critical" ? value : "medium";
}

export function isThreadlessDaemonStreamEvent(event: any): boolean {
  return THREADLESS_STREAM_EVENT_TYPES.has(String(event?.type ?? ""))
    && typeof event?.thread_id !== "string";
}

export function hasOpenLocalAssistantStream(localThreadId: string | null): boolean {
  if (!localThreadId) {
    return false;
  }
  const messages = useAgentStore.getState().getThreadMessages(localThreadId);
  const last = messages[messages.length - 1];
  return last?.role === "assistant" && last.isStreaming === true;
}

export function isAuxiliaryDaemonError(event: unknown): boolean {
  if (!event || typeof event !== "object") return false;
  const candidate = event as { type?: unknown; message?: unknown };
  if (candidate.type !== "error" || typeof candidate.message !== "string") return false;
  return candidate.message.startsWith("message_feedback:");
}

export function clearRetryStatusForDaemonProgressEvent(event: unknown): void {
  if (!event || typeof event !== "object") return;
  const candidate = event as { type?: unknown; thread_id?: unknown };
  if (!["delta", "reasoning", "tool_call", "tool_result", "done"].includes(String(candidate.type ?? ""))) {
    return;
  }
  clearThreadRetryStatus(typeof candidate.thread_id === "string" ? candidate.thread_id : null);
}

export function useDaemonAgentEvents({
  agentBackend,
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
}: {
  agentBackend: AgentBackend;
  activePaneId: string | null;
  activeThread: AgentThread | undefined;
  activeThreadId: string | null;
  activeWorkspace: ReturnType<ReturnType<typeof useWorkspaceStore.getState>["activeWorkspace"]>;
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  setActiveThread: ReturnType<typeof useAgentStore.getState>["setActiveThread"];
  setThreadDaemonId: ReturnType<typeof useAgentStore.getState>["setThreadDaemonId"];
  setThreadTodos: ReturnType<typeof useAgentStore.getState>["setThreadTodos"];
  updateLastAssistantMessage: ReturnType<typeof useAgentStore.getState>["updateLastAssistantMessage"];
  addNotification: ReturnType<typeof import("@/lib/notificationStore").useNotificationStore.getState>["addNotification"];
  daemonThreadIdRef: MutableRefObject<string | null>;
  daemonLocalThreadRef: MutableRefObject<string | null>;
  pendingGatewayMessagesRef: MutableRefObject<Array<{ role: "user"; content: string; inputTokens: number; outputTokens: number; totalTokens: number; isCompactionSummary: boolean }>>;
  goalRunWorkspacesRef: MutableRefObject<Record<string, string>>;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
  setGoalRunsForTrace: Dispatch<SetStateAction<GoalRun[]>>;
  setChatBackView: Dispatch<SetStateAction<import("./types").AgentChatPanelView>>;
  setLatestDivergentSessionId: Dispatch<SetStateAction<string | null>>;
  setView: Dispatch<SetStateAction<import("./types").AgentChatPanelView>>;
  setWelesHealth: Dispatch<SetStateAction<WelesHealthState | null>>;
}) {
  useEffect(() => {
    daemonThreadIdRef.current = null;
    daemonLocalThreadRef.current = null;
    setDaemonTodosByThread({});
    setGoalRunsForTrace([]);
    setChatBackView("threads");
  }, [agentBackend, daemonLocalThreadRef, daemonThreadIdRef, setChatBackView, setDaemonTodosByThread, setGoalRunsForTrace]);

  useEffect(() => {
    if (!shouldUseDaemonRuntime(agentBackend)) return;
    void fetchAllThreadTodos().then(setDaemonTodosByThread);
    void fetchGoalRuns().then(setGoalRunsForTrace);
  }, [agentBackend, setDaemonTodosByThread, setGoalRunsForTrace]);

  useEffect(() => {
    if (!activeThread) {
      daemonThreadIdRef.current = null;
      daemonLocalThreadRef.current = null;
      return;
    }
    daemonLocalThreadRef.current = activeThread.id;
    daemonThreadIdRef.current = activeThread.daemonThreadId ?? null;
  }, [activeThread, daemonLocalThreadRef, daemonThreadIdRef]);

  useEffect(() => {
    const daemonThreadId = daemonThreadIdRef.current;
    const localThreadId = daemonLocalThreadRef.current;
    if (!daemonThreadId || !localThreadId || localThreadId !== activeThreadId) return;
    void fetchThreadTodos(daemonThreadId).then((items) => {
      setThreadTodos(localThreadId, items);
      setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: items }));
    });
  }, [activeThreadId, daemonLocalThreadRef, daemonThreadIdRef, setDaemonTodosByThread, setThreadTodos]);

  useEffect(() => {
    if (!shouldUseDaemonRuntime(agentBackend)) return;

    const zorai = getAgentBridge();
    if (!zorai?.onAgentEvent) return;

    const ensureStreamingAssistantMessage = (threadId: string) => {
      const messages = useAgentStore.getState().getThreadMessages(threadId);
      const last = messages[messages.length - 1];
      if (last?.role === "assistant" && last.isStreaming) {
        return last;
      }

      const agentSettings = useAgentStore.getState().agentSettings;
      addMessage(threadId, {
        role: "assistant",
        content: "",
        provider: agentSettings.active_provider,
        model: ((agentSettings[agentSettings.active_provider] as any)?.model || ""),
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
        isStreaming: true,
      });

      const refreshedMessages = useAgentStore.getState().getThreadMessages(threadId);
      return refreshedMessages[refreshedMessages.length - 1];
    };

    // Providers can emit far more token events than the browser can paint.
    // Coalesce content/reasoning deltas to at most one Zustand update per
    // animation frame; terminal/tool events flush synchronously first.
    const pendingStreamDeltas = new Map<string, { content: string; reasoning: string }>();
    let streamFrame: number | null = null;

    const flushStreamDeltas = () => {
      if (streamFrame !== null) {
        window.cancelAnimationFrame(streamFrame);
        streamFrame = null;
      }
      const pending = [...pendingStreamDeltas.entries()];
      pendingStreamDeltas.clear();
      pending.forEach(([threadId, delta]) => {
        const last = ensureStreamingAssistantMessage(threadId);
        if (last?.role !== "assistant" || !last.isStreaming) return;
        updateLastAssistantMessage(threadId, (last.content || "") + delta.content, true, {
          reasoning: (last.reasoning || "") + delta.reasoning || undefined,
        });
      });
    };

    const queueStreamDelta = (threadId: string, content: string, reasoning: string) => {
      const current = pendingStreamDeltas.get(threadId) ?? { content: "", reasoning: "" };
      pendingStreamDeltas.set(threadId, {
        content: current.content + content,
        reasoning: current.reasoning + reasoning,
      });
      if (streamFrame === null) {
        streamFrame = window.requestAnimationFrame(flushStreamDeltas);
      }
    };

    // Walks the thread's messages from newest to oldest and rewrites the id of
    // the first one that matches `predicate` to `daemonId`. Used to reconcile
    // frontend-local placeholder ids (msg_42) with the daemon's persisted
    // agent_messages.id when the daemon surfaces it on Done/ToolCall/ToolResult.
    const replaceMessageIdAtTail = (
      threadId: string,
      daemonId: string,
      predicate: (m: AgentMessage) => boolean,
    ) => {
      useAgentStore.setState((state) => {
        const list = state.messages[threadId];
        if (!list || list.length === 0) return state;
        for (let i = list.length - 1; i >= 0; i -= 1) {
          const candidate = list[i];
          if (!predicate(candidate)) continue;
          if (candidate.id === daemonId) return state;
          const updated = [...list];
          updated[i] = { ...candidate, id: daemonId };
          return { messages: { ...state.messages, [threadId]: updated } };
        }
        return state;
      });
    };

    const cleanupGoalRunWorkspace = (goalRunId: string) => {
      const workspaceId = goalRunWorkspacesRef.current[goalRunId];
      if (!workspaceId) return;
      setTimeout(() => {
        const currentStore = useWorkspaceStore.getState();
        if (currentStore.workspaces.some((workspace) => workspace.id === workspaceId)) {
          currentStore.closeWorkspace(workspaceId);
        }
        delete goalRunWorkspacesRef.current[goalRunId];
      }, 3000);
    };

    const flushDeferredThreadReload = (localThreadId: string) => {
      if (!consumeDeferredThreadReload(localThreadId)) return;
      const thread = useAgentStore.getState().threads.find((entry) => entry.id === localThreadId);
      if (!thread?.daemonThreadId) return;
      void refreshDaemonThreadMessagesIntoLocalState({
        daemonThreadId: thread.daemonThreadId,
        setThreadTodos,
        setDaemonTodosByThread,
      });
    };

    const unsubscribe = zorai.onAgentEvent((event: any) => {
      if (!event?.type) return;

      const terminalStopEvent = event.type === "turn_interrupted"
        || event.type === "done"
        || event.type === "error";
      const allowThreadlessFallback = !isThreadlessDaemonStreamEvent(event)
        || hasOpenLocalAssistantStream(daemonLocalThreadRef.current)
        || (terminalStopEvent && Boolean(
          daemonLocalThreadRef.current && hasThreadStopBarrier(daemonLocalThreadRef.current),
        ));
      const tid = resolveDaemonEventLocalThreadId(
        event,
        daemonLocalThreadRef.current,
        daemonThreadIdRef.current,
        { allowThreadlessFallback },
      );

      clearRetryStatusForDaemonProgressEvent(event);

      switch (event.type) {
        case "delta": {
          if (!tid) break;
          queueStreamDelta(tid, event.content || "", "");
          break;
        }
        case "reasoning": {
          if (!tid) break;
          queueStreamDelta(tid, "", event.content || "");
          break;
        }
        case "retry_status": {
          applyDaemonRetryStatusEvent(event);
          break;
        }
        case "done": {
          releaseSuppressedThreadRetryStatus(typeof event.thread_id === "string" ? event.thread_id : null);
          clearThreadRetryStatus(typeof event.thread_id === "string" ? event.thread_id : null);
          if (!tid) break;
          flushStreamDeltas();
          useAgentMissionStore.getState().setSharedCursorMode("idle");
          const last = ensureStreamingAssistantMessage(tid);
          if (last?.role === "assistant") {
            updateLastAssistantMessage(tid, last.content || "(empty)", false, {
              inputTokens: event.input_tokens ?? 0,
              outputTokens: event.output_tokens ?? 0,
              totalTokens: (event.input_tokens ?? 0) + (event.output_tokens ?? 0),
              provider: event.provider || undefined,
              model: event.model || undefined,
              cost: typeof event.cost === "number" ? event.cost : undefined,
              tps: typeof event.tps === "number" ? event.tps : undefined,
              reasoning: event.reasoning || last.reasoning || undefined,
            });
          }
          // The daemon now surfaces the persisted assistant `agent_messages.id`
          // alongside the Done event. We rewrite the local id of the just-finalized
          // streaming assistant message so per-message actions (👍/👎, pin, delete)
          // can address the actual DB row instead of the frontend-local counter.
          if (typeof event.message_id === "string" && event.message_id.length > 0) {
            replaceMessageIdAtTail(tid, event.message_id, (m) => m.role === "assistant");
          }
          finalizeStreamingAssistantMessages(tid);
          completeThreadStopBarrier(tid);
          flushDeferredThreadReload(tid);
          trimFollowedThreadToHistoryWindow(tid);
          break;
        }
        case "turn_interrupted": {
          releaseSuppressedThreadRetryStatus(typeof event.thread_id === "string" ? event.thread_id : null);
          clearThreadRetryStatus(typeof event.thread_id === "string" ? event.thread_id : null);
          if (!tid) break;
          flushStreamDeltas();
          useAgentMissionStore.getState().setSharedCursorMode("idle");
          finalizeStreamingAssistantMessages(tid);
          completeThreadStopBarrier(tid);
          flushDeferredThreadReload(tid);
          trimFollowedThreadToHistoryWindow(tid);
          break;
        }
        case "tool_call": {
          if (!tid) break;
          flushStreamDeltas();
          useAgentMissionStore.getState().setSharedCursorMode("agent");
          const messages = useAgentStore.getState().getThreadMessages(tid);
          const last = messages[messages.length - 1];
          if (last?.role === "assistant" && last.isStreaming) {
            updateLastAssistantMessage(tid, last.content, false);
          }
          addMessage(tid, {
            role: "tool",
            content: "",
            toolName: event.name,
            toolCallId: event.call_id,
            toolArguments: event.arguments,
            toolStatus: "requested",
            welesReview: event.weles_review || undefined,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          });
          if (typeof event.message_id === "string" && event.message_id.length > 0) {
            replaceMessageIdAtTail(
              tid,
              event.message_id,
              (m) => m.role === "tool" && m.toolCallId === event.call_id,
            );
          }
          break;
        }
        case "tool_result": {
          if (!tid) break;
          addMessage(tid, {
            role: "tool",
            content: event.content,
            toolName: event.name,
            toolCallId: event.call_id,
            toolStatus: event.is_error ? "error" : "done",
            welesReview: event.weles_review || undefined,
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
          });
          if (typeof event.message_id === "string" && event.message_id.length > 0) {
            replaceMessageIdAtTail(
              tid,
              event.message_id,
              (m) => m.role === "tool" && m.toolCallId === event.call_id,
            );
          }
          const agentSettings = useAgentStore.getState().agentSettings;
          addMessage(tid, {
            role: "assistant",
            content: "",
            provider: agentSettings.active_provider,
            model: ((agentSettings[agentSettings.active_provider] as any)?.model || ""),
            inputTokens: 0,
            outputTokens: 0,
            totalTokens: 0,
            isCompactionSummary: false,
            isStreaming: true,
          });
          break;
        }
        case "approval_required": {
          const approvalId = typeof event.approval_id === "string" ? event.approval_id : "";
          if (!approvalId) break;
          if (useAgentStore.getState().agentSettings.managed_security_level === "yolo") {
            void getAgentBridge()?.agentResolveTaskApproval?.(approvalId, "approve-once");
            break;
          }
          const reasons = Array.isArray(event.reasons)
            ? event.reasons.map((reason: unknown) => String(reason))
            : [];
          useAgentMissionStore.getState().upsertDaemonApproval({
            id: approvalId,
            paneId: activePaneId ?? activeThreadId ?? "",
            workspaceId: activeWorkspace?.id ?? null,
            surfaceId: null,
            sessionId: typeof event.thread_id === "string" ? event.thread_id : null,
            source: "agent-task",
            command: String(event.command ?? approvalId),
            reasons: reasons.length > 0 ? reasons : ["Managed command requires approval"],
            riskLevel: normalizeApprovalRiskLevel(event.risk_level),
            blastRadius: String(event.blast_radius ?? "tool execution"),
          });
          addNotification({
            title: "Approval required",
            body: String(event.command ?? approvalId),
            subtitle: String(event.rationale ?? "Tool execution paused for operator approval"),
            icon: "shield",
            source: "system",
            workspaceId: activeWorkspace?.id ?? null,
            paneId: activePaneId ?? null,
            panelId: activePaneId ?? null,
          });
          break;
        }
        case "error": {
          if (isAuxiliaryDaemonError(event)) {
            addNotification({
              title: "Message feedback failed",
              body: String(event.message).replace(/^message_feedback:/, ""),
              subtitle: "The active response was not interrupted.",
              icon: "alert-triangle",
              source: "system",
              workspaceId: activeWorkspace?.id ?? null,
              paneId: activePaneId ?? null,
              panelId: activePaneId ?? null,
            });
            break;
          }
          clearThreadRetryStatus(typeof event.thread_id === "string" ? event.thread_id : null);
          if (!tid) break;
          flushStreamDeltas();
          useAgentMissionStore.getState().setSharedCursorMode("idle");
          ensureStreamingAssistantMessage(tid);
          updateLastAssistantMessage(tid, `Error: ${event.message}`, false);
          finalizeStreamingAssistantMessages(tid);
          completeThreadStopBarrier(tid);
          flushDeferredThreadReload(tid);
          trimFollowedThreadToHistoryWindow(tid);
          break;
        }
        case "thread_created": {
          handleThreadCreatedEvent({
            event,
            activePaneId,
            addMessage,
            createThread,
            daemonLocalThreadRef,
            daemonThreadIdRef,
            pendingGatewayMessagesRef,
            setActiveThread,
            setDaemonTodosByThread,
            setThreadDaemonId,
            setThreadTodos,
            setView,
          });
          break;
        }
        case "thread_title_updated": {
          handleThreadTitleUpdatedEvent({ event });
          break;
        }
        case "thread_reload_required": {
          const reloadThreadId = typeof event.thread_id === "string" ? event.thread_id : null;
          if (reloadThreadId) {
            const localThreadId = useAgentStore.getState().threads.find(
              (thread) => thread.daemonThreadId === reloadThreadId,
            )?.id ?? null;
            const turnActive = localThreadId
              ? shouldDeferThreadReload(useAgentStore.getState().getThreadMessages(localThreadId))
              : false;
            if (localThreadId && turnActive) {
              deferThreadReload(localThreadId);
              break;
            }
            void refreshDaemonThreadMessagesIntoLocalState({
              daemonThreadId: reloadThreadId,
              setThreadTodos,
              setDaemonTodosByThread,
            });
          }
          break;
        }
        case "participant_suggestion": {
          const reloadThreadId = typeof event.thread_id === "string" ? event.thread_id : null;
          if (reloadThreadId) {
            void reloadDaemonThreadIntoLocalState({
              daemonThreadId: reloadThreadId,
              setThreadTodos,
              setDaemonTodosByThread,
            });
          }
          break;
        }
        case "task_update":
          handleTaskUpdateEvent({ event, activePaneId, activeWorkspace, addNotification });
          break;
        case "goal_run_update":
        case "goal_run_created":
          handleGoalRunEvent({
            event,
            activePaneId,
            activeWorkspace,
            addNotification,
            cleanupGoalRunWorkspace,
            setGoalRunsForTrace,
          });
          break;
        case "todo_update":
          handleTodoUpdateEvent({
            event,
            daemonLocalThreadRef,
            daemonThreadIdRef,
            setDaemonTodosByThread,
            setGoalRunsForTrace,
            setThreadTodos,
          });
          break;
        case "workspace_task_update": {
          const workspaceId = typeof event.task?.workspace_id === "string" ? event.task.workspace_id : null;
          window.dispatchEvent(new CustomEvent("zorai-workspace-refresh", { detail: { workspaceId } }));
          break;
        }
        case "workflow_notice":
          recordDaemonWorkflowNotice({ event, activePaneId, activeWorkspace });
          if (event.kind === "operator-profile-warning") {
            handleOperatorProfileWarning({ event, activePaneId, activeWorkspace, addNotification });
          }
          break;
        case "weles_health_update":
          syncWelesHealth(event, setWelesHealth, (content) => appendDaemonSystemMessage(content, daemonLocalThreadRef.current ?? activeThreadId));
          break;
        case "agent-divergent-session-started":
          handleDivergentStartEvent({ event, activeThreadId, daemonLocalThreadRef, setLatestDivergentSessionId });
          break;
        case "agent-divergent-session":
          handlePayloadMessageEvent(event, "Failed to fetch divergent session: ", "Divergent session payload", daemonLocalThreadRef.current ?? activeThreadId);
          break;
        case "agent-explanation":
          handlePayloadMessageEvent(event, "Failed to explain action: ", "Explainability", daemonLocalThreadRef.current ?? activeThreadId);
          break;
        case "workspace_command":
          handleWorkspaceCommand(event);
          break;
        case "concierge_welcome":
          useAgentStore.setState({
            conciergeWelcome: {
              content: event.content ?? "",
              actions: event.actions ?? [],
            },
          });
          break;
        case "gateway_incoming":
          handleGatewayIncomingEvent({ event, addMessage, pendingGatewayMessagesRef });
          break;
      }
    });

    return () => {
      if (streamFrame !== null) {
        window.cancelAnimationFrame(streamFrame);
      }
      pendingStreamDeltas.clear();
      if (typeof unsubscribe === "function") {
        unsubscribe();
      }
    };
  }, [
    activePaneId,
    activeThreadId,
    activeWorkspace,
    addMessage,
    addNotification,
    agentBackend,
    createThread,
    daemonLocalThreadRef,
    daemonThreadIdRef,
    goalRunWorkspacesRef,
    pendingGatewayMessagesRef,
    setActiveThread,
    setDaemonTodosByThread,
    setGoalRunsForTrace,
    setLatestDivergentSessionId,
    setThreadDaemonId,
    setThreadTodos,
    setView,
    setWelesHealth,
    updateLastAssistantMessage,
  ]);
}
