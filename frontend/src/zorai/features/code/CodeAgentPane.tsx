import { useCallback, useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore, type AgentMessage, type AgentThread } from "@/lib/agentStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";
import { fetchAgentTasks, type AgentQueueTask } from "@/lib/agentTaskQueue";
import { useWorkspaceContextStore } from "@/lib/workspaceContextStore";
import { ThreadsView } from "../threads/ThreadsView";
import { classifyThreadActivityMessage } from "../threads/threadActivityModel";
import { openThreadTarget } from "../threads/openThreadTarget";
import { threadReadKey, useThreadReadStateStore } from "../threads/threadReadStateStore";
import { displayRootName } from "./codeEmptyStateModel";
import {
  actualThreadResponder,
  authoritativeThreadIdentity,
  projectThreadsForRoot,
  resolveCodeProjectThreadStatus,
  type CodeProjectThreadEntry,
} from "./codeProjectThreads";
import { useCodeWorkspaceBindingStore } from "./codeWorkspaceBindingStore";
import { CodeThreadHistoryMenu, type CodeThreadHistoryEntry } from "./CodeThreadHistoryMenu";
import { threadTurnIsActive } from "@/components/agent-chat-panel/runtime/threadTurnState";
const ACTIVE_GOAL_STATUSES = new Set(["queued", "planning", "running", "paused"]);

/**
 * Module-level empty fallback so the projectThreadIds selector never returns
 * a fresh array reference per snapshot (that re-renders forever — React
 * error #185 — because useSyncExternalStore compares snapshots by identity).
 */
const NO_PROJECT_THREADS: string[] = [];

export function CodeAgentPane() {
  const runtime = useAgentChatPanelRuntime();
  const activeThreadId = useAgentStore((state) => state.activeThreadId);
  const localThreads = useAgentStore((state) => state.threads);
  const activeThread = localThreads.find((thread) => thread.id === activeThreadId) ?? null;
  const contextsByThreadId = useWorkspaceContextStore((state) => state.byThreadId);
  void useWorkspaceContextStore((state) => state.hydrated);
  const bindRoot = useWorkspaceContextStore((state) => state.bindRoot);
  const root = useCodeWorkspaceBindingStore((state) => state.lastRoot);
  const threadsByRoot = useCodeWorkspaceBindingStore((state) => state.threadsByRoot);
  const projectThreadIds = useMemo(
    () => (root ? threadsByRoot[root] ?? NO_PROJECT_THREADS : NO_PROJECT_THREADS),
    [root, threadsByRoot],
  );
  const readState = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const operatorQuestion = useOperatorQuestionStore((state) => state.question);
  const fetchThreadList = runtime.fetchThreadList;
  const [daemonThreads, setDaemonThreads] = useState<AgentThread[]>([]);
  const [tasks, setTasks] = useState<AgentQueueTask[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await useWorkspaceContextStore.getState().hydrate();
      const [nextThreads, nextTasks] = await Promise.all([
        fetchThreadList({ includeInternal: true }),
        fetchAgentTasks(),
      ]);
      setDaemonThreads(nextThreads);
      setTasks(nextTasks);
      // History membership was previously keyed by ephemeral local ids
      // (thread_5) that get reminted on hydrate. Heal the persisted set:
      // for any daemon-backed local thread currently in scope, ensure its
      // durable daemon id is present so reload doesn't drop history to 1.
      if (root) {
        const daemonByLocal = new Map<string, string>();
        for (const thread of useAgentStore.getState().threads) {
          const daemonId = thread.daemonThreadId?.trim();
          if (daemonId) daemonByLocal.set(thread.id, daemonId);
        }
        const needsHeal = projectThreadIds.some((id) => /^(?:local_thread_|thread_)\d+$/.test(id) && !id.includes(":"));
        if (needsHeal) {
          for (const id of [...projectThreadIds]) {
            const daemonId = daemonByLocal.get(id);
            if (daemonId && !projectThreadIds.includes(daemonId)) {
              useCodeWorkspaceBindingStore.getState().recordProjectThread(root, daemonId);
            }
          }
        }
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not load project threads.");
    } finally {
      setLoading(false);
    }
  }, [fetchThreadList, projectThreadIds, root]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!root || !activeThread?.daemonThreadId) return;
    const context = contextsByThreadId[activeThread.id];
    if (context?.root !== root) return;
    useCodeWorkspaceBindingStore.getState().recordProjectThread(root, activeThread.daemonThreadId);
  }, [activeThread, contextsByThreadId, root]);

  const allMessagesByThread = runtime.allMessagesByThread;
  const goalRunsForTrace = runtime.goalRunsForTrace;
  const entries = useMemo(() => {
    // Don't block history on context hydrate: without a provider-backed
    // context map the membership gate still recovers via daemon synthesis.
    // Once `hydrated` flips the root gate self-corrects.
    return projectThreadsForRoot({
      root,
      localThreads,
      daemonThreads,
      contextsByLocalThreadId: contextsByThreadId,
      projectThreadIds,
    }).map((entry): CodeThreadHistoryEntry => {
      const evidence = evidenceForEntry(
        entry,
        {
          allMessagesByThread,
          goalRunsForTrace,
        },
        operatorQuestion?.threadId ?? null,
        readState,
        tasks,
      );
      return {
        ...entry,
        latestCompletionAt: evidence.latestCompletionAt,
        status: resolveCodeProjectThreadStatus(evidence),
      };
    });
  }, [
    allMessagesByThread,
    contextsByThreadId,
    daemonThreads,
    goalRunsForTrace,
    localThreads,
    operatorQuestion?.threadId,
    projectThreadIds,
    readState,
    root,
    tasks,
  ]);

  useEffect(() => {
    if (!activeThread) return;
    const entry = entries.find((candidate) => candidate.localThread.id === activeThread.id);
    if (!entry?.latestCompletionAt) return;
    const key = threadReadKey(entry.thread);
    if (!key) return;
    const lastReadAt = useThreadReadStateStore.getState().lastReadAt(key) ?? 0;
    if (entry.latestCompletionAt <= lastReadAt) return;
    useThreadReadStateStore.getState().markRead(key, entry.latestCompletionAt);
  }, [activeThread, entries]);

  const createProjectThread = () => {
    if (!root) return;
    const responder = activeThread
      ? actualThreadResponder(activeThread)
      : { id: "swarog", name: "Svarog" };
    const localId = runtime.createThread({
      workspaceId: runtime.activeWorkspace?.id ?? null,
      title: `Code · ${displayRootName(root)}`,
      agentId: responder.id,
      agentName: responder.name,
    });
    bindRoot(localId, root);
    useCodeWorkspaceBindingStore.getState().recordProjectThread(root, localId);
    runtime.openThread(localId);
    setError(null);
  };

  const selectProjectThread = async (entry: CodeThreadHistoryEntry) => {
    setError(null);
    if (root) bindRoot(entry.localThread.id, root);
    const target = entry.thread.daemonThreadId ?? entry.localThread.daemonThreadId;
    const opened = target
      ? await openThreadTarget(runtime, target)
      : (runtime.openThread(entry.localThread.id), true);
    if (!opened) {
      setError("This project thread is no longer available.");
      return;
    }
    const openedId = useAgentStore.getState().activeThreadId;
    if (root && openedId) bindRoot(openedId, root);
    if (root && target) {
      useCodeWorkspaceBindingStore.getState().recordProjectThread(root, target);
    }
  };

  // const workspace = activeThreadId ? contextsByThreadId[activeThreadId] ?? null : null;
  // const activeFile = workspace?.activeFile?.split(/[\\/]/).slice(-1)[0] ?? null;
  // const selection = workspace?.selection;
  const currentIdentity = activeThread ? authoritativeThreadIdentity(activeThread) : null;

  return (
    <div className="zorai-code-agent-pane">
      <ThreadsView
        variant="compact"
        compactHeaderActions={
          <CodeThreadHistoryMenu
            entries={entries}
            currentIdentity={currentIdentity}
            canCreate={Boolean(root)}
            loading={loading}
            error={error}
            onCreate={createProjectThread}
            onOpen={() => void refresh()}
            onSelect={(entry) => void selectProjectThread(entry)}
            onRetry={() => void refresh()}
          />
        }
      />
    </div>
  );
}

const EMPTY_MESSAGES: AgentMessage[] = [];
const threadMessageScans = new WeakMap<AgentMessage[], { working: boolean; latestCompletionAt: number | null }>();

function scanThreadMessages(messages: AgentMessage[]): { working: boolean; latestCompletionAt: number | null } {
  const cached = threadMessageScans.get(messages);
  if (cached) return cached;

  let working = threadTurnIsActive(messages);
  let latestCompletionAt: number | null = null;
  for (const message of messages) {
    if (message.role === "assistant" && !message.isStreaming) {
      latestCompletionAt = Math.max(latestCompletionAt ?? 0, message.createdAt);
    }
    const activity = classifyThreadActivityMessage(message);
    if (activity?.kind !== "operation") continue;
    if (activity.operations.some((operation) => operation.state === "accepted" || operation.state === "started")) {
      working = true;
    }
    if (activity.operations.some((operation) => operation.state === "completed" || operation.state === "failed")) {
      latestCompletionAt = Math.max(latestCompletionAt ?? 0, message.createdAt);
    }
  }

  const scanned = { working, latestCompletionAt };
  threadMessageScans.set(messages, scanned);
  return scanned;
}

function evidenceForEntry(
  entry: CodeProjectThreadEntry,
  runtime: {
    allMessagesByThread: ReturnType<typeof useAgentChatPanelRuntime>["allMessagesByThread"];
    goalRunsForTrace: ReturnType<typeof useAgentChatPanelRuntime>["goalRunsForTrace"];
  },
  questionThreadId: string | null,
  readState: Record<string, number>,
  tasks: AgentQueueTask[],
) {
  const identities = new Set([entry.localThread.id, entry.thread.daemonThreadId, entry.localThread.daemonThreadId].filter(Boolean));
  // Status for the history list is per-entry, not "is the currently
  // focused thread streaming". A background thread keeps Working/Needs
  // attention even while you look at another thread.
  const messages = runtime.allMessagesByThread[entry.localThread.id]
    ?? runtime.allMessagesByThread[entry.thread.daemonThreadId ?? ""] ?? EMPTY_MESSAGES;
  const scanned = scanThreadMessages(messages);
  let working = scanned.working;
  let latestCompletionAt = scanned.latestCompletionAt;

  let needsOperatorAction = Boolean(questionThreadId && identities.has(questionThreadId));
  for (const task of tasks) {
    const associated = [task.thread_id, task.parent_thread_id].some((threadId) => threadId && identities.has(threadId));
    if (!associated) continue;
    if (task.status === "awaiting_approval") needsOperatorAction = true;
    if (["queued", "in_progress", "failed_analyzing"].includes(task.status)) working = true;
    if (["completed", "failed", "cancelled", "budget_exceeded"].includes(task.status) && task.completed_at) {
      latestCompletionAt = Math.max(latestCompletionAt ?? 0, task.completed_at);
    }
  }
  for (const goal of runtime.goalRunsForTrace) {
    const goalThreads = [goal.thread_id, goal.root_thread_id, goal.active_thread_id, ...(goal.execution_thread_ids ?? [])];
    if (!goalThreads.some((threadId) => threadId && identities.has(threadId))) continue;
    if (goal.status === "awaiting_approval") needsOperatorAction = true;
    if (ACTIVE_GOAL_STATUSES.has(goal.status)) working = true;
    if (goal.completed_at) latestCompletionAt = Math.max(latestCompletionAt ?? 0, goal.completed_at);
  }

  const key = threadReadKey(entry.thread);
  return {
    needsOperatorAction,
    working,
    latestCompletionAt,
    lastReadAt: key ? readState[key] ?? null : null,
  };
}
