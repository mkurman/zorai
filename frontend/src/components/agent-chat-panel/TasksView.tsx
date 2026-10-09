import { useCallback, useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import {
  controlGoalRun,
  fetchGoalRuns,
  goalRunChildTaskCount,
  goalRunSupportAvailable,
  isGoalRunActive,
  startGoalRun,
  type GoalRun,
} from "../../lib/goalRuns";
import {
  fetchAgentTasks,
  isTaskActive,
  type AgentQueueTask,
} from "../../lib/agentTaskQueue";
import { fetchAgentRuns, type AgentRun } from "../../lib/agentRuns";
import { provisionAgentWorkspaceTerminals } from "../../lib/agentWorkspace";
import { fetchThreadTodos } from "../../lib/agentTodos";
import {
  buildHydratedRemoteMessage,
  useAgentStore,
} from "../../lib/agentStore";
import { resolveReactChatHistoryMessageLimit } from "../../lib/chatHistoryPageSize";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import { GoalRunPanel } from "./tasks-view/GoalRunPanel";
import { HeartbeatSection } from "./tasks-view/HeartbeatSection";
import { TaskQueuePanel } from "./tasks-view/TaskQueuePanel";
import { findTaskWorkspaceLocation } from "./tasks-view/helpers";
import type {
  HeartbeatItem,
  RemoteAgentThread,
  TasksViewProps,
  ThreadTarget,
} from "./tasks-view/types";

export function collectSelectedTaskSubagents(
  selectedTask: AgentQueueTask | null,
  runs: AgentRun[],
): AgentRun[] {
  if (!selectedTask) {
    return [];
  }

  const subagentRuns = runs
    .filter((run) => run.kind === "subagent")
    .slice()
    .sort((a, b) => b.created_at - a.created_at);
  const selectedTaskIdentities = new Set<string>([selectedTask.id]);
  const selectedRunIds = new Set<string>([selectedTask.id]);
  let changed = true;

  while (changed) {
    changed = false;
    for (const run of subagentRuns) {
      if (selectedRunIds.has(run.id)) {
        continue;
      }

      const parentMatches =
        Boolean(run.parent_run_id && selectedRunIds.has(run.parent_run_id)) ||
        Boolean(run.parent_task_id && selectedTaskIdentities.has(run.parent_task_id));

      if (!parentMatches) {
        continue;
      }

      selectedRunIds.add(run.id);
      selectedTaskIdentities.add(run.task_id ?? run.id);
      changed = true;
    }
  }

  return subagentRuns.filter((run) => selectedRunIds.has(run.id));
}

export function TasksView({ onOpenThreadView }: TasksViewProps) {
  const [tasks, setTasks] = useState<AgentQueueTask[]>([]);
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const [goalRuns, setGoalRuns] = useState<GoalRun[]>([]);
  const [heartbeatItems, setHeartbeatItems] = useState<HeartbeatItem[]>([]);
  const [newTaskTitle, setNewTaskTitle] = useState("");
  const [newTaskDescription, setNewTaskDescription] = useState("");
  const [newTaskCommand, setNewTaskCommand] = useState("");
  const [newTaskSessionId, setNewTaskSessionId] = useState("");
  const [newTaskDependencies, setNewTaskDependencies] = useState("");
  const [newGoalTitle, setNewGoalTitle] = useState("");
  const [newGoalPrompt, setNewGoalPrompt] = useState("");
  const [newGoalSessionId, setNewGoalSessionId] = useState("");
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedGoalRunId, setSelectedGoalRunId] = useState<string | null>(
    null,
  );
  const [goalActionId, setGoalActionId] = useState<string | null>(null);
  const [goalStartError, setGoalStartError] = useState<string | null>(null);
  const [historyFailureQuery, setHistoryFailureQuery] = useState("");
  const [historyMinReplans, setHistoryMinReplans] = useState(0);
  const [historyMinChildTasks, setHistoryMinChildTasks] = useState(0);
  const [historyMinApprovals, setHistoryMinApprovals] = useState(0);
  const [historyMinDurationMinutes, setHistoryMinDurationMinutes] = useState(0);

  const zorai = getBridge();
  const goalRunsSupported = goalRunSupportAvailable();
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const createThread = useAgentStore((state) => state.createThread);
  const addMessage = useAgentStore((state) => state.addMessage);
  const setActiveThread = useAgentStore((state) => state.setActiveThread);
  const setThreadDaemonId = useAgentStore((state) => state.setThreadDaemonId);
  const setThreadTodos = useAgentStore((state) => state.setThreadTodos);
  const threads = useAgentStore((state) => state.threads);
  const reactChatHistoryPageSize = useAgentStore(
    (state) => state.agentSettings.react_chat_history_page_size,
  );

  const refreshTasks = useCallback(async () => {
    const result = await fetchAgentTasks();
    setTasks(result);
    setSelectedTaskId((current) => current ?? result[0]?.id ?? null);
  }, []);

  const refreshRuns = useCallback(async () => {
    const result = await fetchAgentRuns();
    setRuns(result);
  }, []);

  const refreshGoalRuns = useCallback(async () => {
    if (!goalRunsSupported) {
      setGoalRuns([]);
      return;
    }

    const result = await fetchGoalRuns();
    setGoalRuns(result);
    setSelectedGoalRunId((current) => current ?? result[0]?.id ?? null);
  }, [goalRunsSupported]);

  const refreshHeartbeat = useCallback(async () => {
    if (!zorai?.agentHeartbeatGetItems) {
      return;
    }

    try {
      const result = await zorai.agentHeartbeatGetItems();
      setHeartbeatItems(Array.isArray(result) ? (result as HeartbeatItem[]) : []);
    } catch {
      /* silent */
    }
  }, [zorai]);

  useEffect(() => {
    void refreshTasks();
    void refreshRuns();
    void refreshGoalRuns();
    void refreshHeartbeat();

    const interval = setInterval(() => {
      if (document.hidden) return;
      void refreshTasks();
      void refreshRuns();
      void refreshGoalRuns();
      void refreshHeartbeat();
    }, 5000);

    return () => clearInterval(interval);
  }, [refreshGoalRuns, refreshHeartbeat, refreshRuns, refreshTasks]);

  useEffect(() => {
    if (!zorai?.onAgentEvent) {
      return;
    }

    const unsubscribe = zorai.onAgentEvent((event: any) => {
      if (!event?.type) {
        return;
      }
      if (
        event.type === "goal_run_update" ||
        event.type === "goal_run_created" ||
        event.type === "todo_update"
      ) {
        void refreshGoalRuns();
      }
      if (event.type === "task_update") {
        void refreshTasks();
        void refreshRuns();
      }
    });

    return () => unsubscribe?.();
  }, [zorai, refreshGoalRuns, refreshRuns, refreshTasks]);

  const addTask = async () => {
    if (!newTaskTitle.trim() || !zorai?.agentAddTask) {
      return;
    }

    await zorai.agentAddTask({
      title: newTaskTitle.trim(),
      description: (newTaskDescription || newTaskTitle).trim(),
      priority: "normal",
      command: newTaskCommand.trim() || null,
      sessionId: newTaskSessionId.trim() || null,
      dependencies: newTaskDependencies
        .split(",")
        .map((value) => value.trim())
        .filter(Boolean),
    });

    setNewTaskTitle("");
    setNewTaskDescription("");
    setNewTaskCommand("");
    setNewTaskSessionId("");
    setNewTaskDependencies("");
    void refreshTasks();
    void refreshRuns();
  };

  const addGoalRun = async () => {
    if (!goalRunsSupported || !newGoalPrompt.trim()) {
      return;
    }

    setGoalStartError(null);
    const provision = newGoalSessionId.trim()
      ? null
      : await provisionAgentWorkspaceTerminals({
          title: newGoalTitle.trim() || newGoalPrompt.trim(),
          cwd: activeWorkspace?.cwd ?? null,
        });

    const goalRun = await startGoalRun({
      goal: newGoalPrompt.trim(),
      title: newGoalTitle.trim() || null,
      sessionId:
        newGoalSessionId.trim() || provision?.coordinatorSessionId || null,
      priority: "normal",
    });

    if (!goalRun) {
      setGoalStartError("Goal runner backend is not available yet.");
      return;
    }

    setNewGoalTitle("");
    setNewGoalPrompt("");
    setNewGoalSessionId("");
    setSelectedGoalRunId(goalRun.id);
    void refreshGoalRuns();
  };

  const changeGoalRunState = async (
    goalRunId: string,
    action: "pause" | "resume" | "cancel" | "accept" | "soft_reject" | "hard_reject",
    stepIndex?: number,
    explanation?: string,
  ) => {
    setGoalActionId(goalRunId);
    try {
      await controlGoalRun(goalRunId, action, stepIndex ?? null, explanation ?? null);
      await refreshGoalRuns();
    } finally {
      setGoalActionId(null);
    }
  };

  const cancelTask = async (taskId: string) => {
    if (!zorai?.agentCancelTask) {
      return;
    }
    await zorai.agentCancelTask(taskId);
    void refreshTasks();
    void refreshRuns();
  };

  const openTaskThread = useCallback(
    async (task: ThreadTarget) => {
      if (!task.thread_id || !zorai?.agentGetThread) {
        return;
      }

      const existingThread = threads.find(
        (entry) => entry.daemonThreadId === task.thread_id,
      );
      if (existingThread) {
        setActiveThread(existingThread.id);
        onOpenThreadView?.();
        return;
      }

      const remoteThread = (await zorai.agentGetThread(
        task.thread_id,
        {
          messageLimit:
            resolveReactChatHistoryMessageLimit(reactChatHistoryPageSize) ?? null,
        },
      )) as RemoteAgentThread | null;
      if (!remoteThread) {
        return;
      }

      const location = findTaskWorkspaceLocation(
        useWorkspaceStore.getState().workspaces,
        task.session_id,
      );
      const localThreadId = createThread({
        workspaceId: location?.workspaceId ?? null,
        surfaceId: location?.surfaceId ?? null,
        paneId: location?.paneId ?? null,
        title: remoteThread.title || task.title,
      });
      setThreadDaemonId(localThreadId, remoteThread.id);

      for (const message of remoteThread.messages ?? []) {
        addMessage(localThreadId, buildHydratedRemoteMessage(localThreadId, message));
      }

      const todos = await fetchThreadTodos(remoteThread.id).catch(() => []);
      setThreadTodos(localThreadId, todos);
      setActiveThread(localThreadId);
      onOpenThreadView?.();
    },
    [
      addMessage,
      zorai,
      createThread,
      onOpenThreadView,
      reactChatHistoryPageSize,
      setActiveThread,
      setThreadDaemonId,
      setThreadTodos,
      threads,
    ],
  );

  const topLevelTasks = useMemo(
    () => tasks.filter((task) => !task.parent_task_id),
    [tasks],
  );

  const activeTasks = topLevelTasks.filter(isTaskActive);
  const completedTasks = topLevelTasks.filter((task) => !isTaskActive(task));
  const selectedTask =
    tasks.find((task) => task.id === selectedTaskId) ??
    topLevelTasks[0] ??
    tasks[0] ??
    null;

  const selectedTaskSubagents = useMemo(
    () => collectSelectedTaskSubagents(selectedTask, runs),
    [runs, selectedTask],
  );

  const activeGoalRuns = useMemo(
    () => goalRuns.filter(isGoalRunActive),
    [goalRuns],
  );
  const historicalGoalRuns = useMemo(
    () => goalRuns.filter((goalRun) => !isGoalRunActive(goalRun)),
    [goalRuns],
  );
  const completedGoalRuns = useMemo(() => {
    const failureQuery = historyFailureQuery.trim().toLowerCase();
    return historicalGoalRuns.filter((goalRun) => {
      const durationMinutes =
        typeof goalRun.duration_ms === "number" ? goalRun.duration_ms / 60000 : 0;
      const failureText =
        `${goalRun.failure_cause ?? ""} ${goalRun.last_error ?? ""} ${
          goalRun.error ?? ""
        }`.toLowerCase();

      if (goalRun.replan_count < historyMinReplans) {
        return false;
      }
      if (goalRunChildTaskCount(goalRun) < historyMinChildTasks) {
        return false;
      }
      if ((goalRun.approval_count ?? 0) < historyMinApprovals) {
        return false;
      }
      if (durationMinutes < historyMinDurationMinutes) {
        return false;
      }
      if (failureQuery && !failureText.includes(failureQuery)) {
        return false;
      }
      return true;
    });
  }, [
    historicalGoalRuns,
    historyFailureQuery,
    historyMinApprovals,
    historyMinChildTasks,
    historyMinDurationMinutes,
    historyMinReplans,
  ]);
  const selectedGoalRun =
    goalRuns.find((goalRun) => goalRun.id === selectedGoalRunId) ??
    goalRuns[0] ??
    null;
  const selectedGoalRunAgentRuns = useMemo(
    () => selectedGoalRun
      ? runs.filter((run) => run.goal_run_id === selectedGoalRun.id)
      : [],
    [runs, selectedGoalRun],
  );

  return (
    <div style={{ padding: "var(--space-4)", overflow: "auto", height: "100%" }}>
      <GoalRunPanel
        goalRunsSupported={goalRunsSupported}
        newGoalPrompt={newGoalPrompt}
        setNewGoalPrompt={setNewGoalPrompt}
        newGoalTitle={newGoalTitle}
        setNewGoalTitle={setNewGoalTitle}
        newGoalSessionId={newGoalSessionId}
        setNewGoalSessionId={setNewGoalSessionId}
        goalStartError={goalStartError}
        onAddGoalRun={() => void addGoalRun()}
        onRefreshGoalRuns={() => void refreshGoalRuns()}
        activeGoalRuns={activeGoalRuns}
        historicalGoalRuns={historicalGoalRuns}
        completedGoalRuns={completedGoalRuns}
        selectedGoalRun={selectedGoalRun}
        selectedGoalRunAgentRuns={selectedGoalRunAgentRuns}
        selectedGoalRunId={selectedGoalRunId}
        goalActionId={goalActionId}
        onSelectGoalRun={setSelectedGoalRunId}
        onChangeGoalRunState={(goalRunId, action, stepIndex) =>
          void changeGoalRunState(goalRunId, action, stepIndex)
        }
        historyFailureQuery={historyFailureQuery}
        setHistoryFailureQuery={setHistoryFailureQuery}
        historyMinReplans={historyMinReplans}
        setHistoryMinReplans={setHistoryMinReplans}
        historyMinChildTasks={historyMinChildTasks}
        setHistoryMinChildTasks={setHistoryMinChildTasks}
        historyMinApprovals={historyMinApprovals}
        setHistoryMinApprovals={setHistoryMinApprovals}
        historyMinDurationMinutes={historyMinDurationMinutes}
        setHistoryMinDurationMinutes={setHistoryMinDurationMinutes}
        totalGoalRunCount={goalRuns.length}
      />

      <TaskQueuePanel
        newTaskTitle={newTaskTitle}
        setNewTaskTitle={setNewTaskTitle}
        newTaskDescription={newTaskDescription}
        setNewTaskDescription={setNewTaskDescription}
        newTaskCommand={newTaskCommand}
        setNewTaskCommand={setNewTaskCommand}
        newTaskSessionId={newTaskSessionId}
        setNewTaskSessionId={setNewTaskSessionId}
        newTaskDependencies={newTaskDependencies}
        setNewTaskDependencies={setNewTaskDependencies}
        onAddTask={() => void addTask()}
        activeTasks={activeTasks}
        completedTasks={completedTasks}
        selectedTask={selectedTask}
        selectedTaskSubagents={selectedTaskSubagents}
        onSelectTask={setSelectedTaskId}
        onCancelTask={(taskId) => void cancelTask(taskId)}
        onOpenTaskThread={(task) => void openTaskThread(task)}
        hasTasks={tasks.length > 0}
      />

      <HeartbeatSection heartbeatItems={heartbeatItems} />
    </div>
  );
}
