import { useCallback, useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import {
  controlGoalRun,
  fetchGoalRuns,
  formatGoalRunDuration,
  formatGoalRunStatus,
  goalRunsNeedAutoRefresh,
  goalRunSupportAvailable,
  isGoalRunActive,
  latestGoalRunTodoSnapshot,
  startGoalRun,
  summarizeGoalRunStep,
  type GoalRun,
} from "@/lib/goalRuns";
import { useAgentStore } from "@/lib/agentStore";
import { ThreadFilePreviewOverlay } from "../threads/ThreadFilePreviewOverlay";
import { GoalWorkspacePanel } from "./GoalWorkspacePanel";
import { GoalLaunchPanel } from "./GoalLaunchPanel";
import { openThreadTarget } from "../threads/openThreadTarget";
import { navigateZorai, type ZoraiReturnTarget } from "../../shell/zoraiNavigationEvents";
import { CollapsibleSummary } from "../collapsibleSummary";

const activeStatuses = new Set(["queued", "planning", "running", "awaiting_approval", "awaiting_review", "paused"]);
const GOAL_LAUNCH_EVENT = "zorai-goal-launch";

export function GoalsRail() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const activeGoals = goalRunsForTrace.filter((goal) => activeStatuses.has(goal.status));

  return (
    <div className="zorai-rail-stack">
      <button
        type="button"
        className="zorai-primary-button"
        onClick={() => window.dispatchEvent(new Event(GOAL_LAUNCH_EVENT))}
      >
        + New goal
      </button>
      <div className="zorai-section-label">Active Goals</div>
      {activeGoals.length === 0 ? (
        <div className="zorai-empty">No goal runs are active.</div>
      ) : (
        activeGoals.slice(0, 6).map((goal) => (
          <div key={goal.id} className="zorai-rail-card">
            <strong>{goal.title || goal.goal}</strong>
            <span>{formatGoalRunStatus(goal.status)}</span>
          </div>
        ))
      )}
    </div>
  );
}

export function GoalsContext() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const waiting = goalRunsForTrace.filter((goal) => goal.status === "awaiting_approval" || goal.status === "awaiting_review").length;
  const active = goalRunsForTrace.filter(isGoalRunActive).length;
  const failed = goalRunsForTrace.filter((goal) => goal.status === "failed").length;

  return (
    <div className="zorai-context-summary">
      <div className="zorai-section-label">Goal Context</div>
      <div className="zorai-context-stat-grid">
        <Metric label="Active" value={active} />
        <Metric label="Awaiting" value={waiting} />
        <Metric label="Failed" value={failed} />
      </div>
      <div className="zorai-context-block">
        <strong>Workspace modes</strong>
        <span>Worker thread plus Accept / Soft reject / Hard reject</span>
      </div>
    </div>
  );
}

export function GoalsView({
  openGoalRunRequest,
  returnTarget = null,
  onReturnTarget,
}: {
  openGoalRunRequest?: { id: string; nonce: number } | null;
  returnTarget?: ZoraiReturnTarget | null;
  onReturnTarget?: () => void;
}) {
  const runtime = useAgentChatPanelRuntime();
  const { goalRunsForTrace } = runtime;
  const [goalRuns, setGoalRuns] = useState<GoalRun[]>([]);
  const [starting, setStarting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(openGoalRunRequest?.id ?? null);
  const [workspaceOpen, setWorkspaceOpen] = useState(() => Boolean(openGoalRunRequest?.id));
  const [launchOpen, setLaunchOpen] = useState(false);
  const autoRefreshIntervalSecs = useAgentStore((state) => state.agentSettings.auto_refresh_interval_secs);
  const supported = goalRunSupportAvailable();

  const visibleGoalRuns = useMemo(() => {
    const byId = new Map<string, GoalRun>();
    for (const item of goalRunsForTrace) byId.set(item.id, item);
    for (const item of goalRuns) byId.set(item.id, item);
    return [...byId.values()].sort((a, b) => b.created_at - a.created_at);
  }, [goalRuns, goalRunsForTrace]);

  const metrics = useMemo(() => {
    const active = visibleGoalRuns.filter(isGoalRunActive).length;
    const waiting = visibleGoalRuns.filter((run) => run.status === "awaiting_approval" || run.status === "awaiting_review").length;
    const completed = visibleGoalRuns.filter((run) => run.status === "completed").length;
    return { active, waiting, completed, total: visibleGoalRuns.length };
  }, [visibleGoalRuns]);

  const selectedRun = useMemo(() => {
    return visibleGoalRuns.find((run) => run.id === selectedRunId) ?? visibleGoalRuns[0] ?? null;
  }, [selectedRunId, visibleGoalRuns]);

  useEffect(() => {
    if (!selectedRunId && visibleGoalRuns[0]) {
      setSelectedRunId(visibleGoalRuns[0].id);
    }
  }, [selectedRunId, visibleGoalRuns]);

  useEffect(() => {
    if (!openGoalRunRequest?.id) return;
    setSelectedRunId(openGoalRunRequest.id);
    setWorkspaceOpen(true);
  }, [openGoalRunRequest?.id, openGoalRunRequest?.nonce]);

  useEffect(() => {
    const onLaunch = () => setLaunchOpen(true);
    window.addEventListener(GOAL_LAUNCH_EVENT, onLaunch);
    return () => window.removeEventListener(GOAL_LAUNCH_EVENT, onLaunch);
  }, []);

  const refresh = useCallback(async () => {
    setGoalRuns(await fetchGoalRuns());
  }, []);

  const autoRefreshGoalRuns = useMemo(() => {
    if (workspaceOpen && selectedRun) {
      return goalRunsNeedAutoRefresh([selectedRun]);
    }
    return goalRunsNeedAutoRefresh(visibleGoalRuns);
  }, [selectedRun, visibleGoalRuns, workspaceOpen]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    const intervalSecs = Math.max(0, Math.trunc(Number(autoRefreshIntervalSecs) || 0));
    if (intervalSecs <= 0 || !autoRefreshGoalRuns) return;
    const timer = window.setInterval(() => void refresh(), intervalSecs * 1000);
    return () => window.clearInterval(timer);
  }, [autoRefreshGoalRuns, autoRefreshIntervalSecs, refresh]);

  const handleStartGoal = async (payload: Parameters<typeof startGoalRun>[0]) => {
    if (!payload.goal.trim() || !supported) return;
    setStarting(true);
    setMessage(null);
    const result = await startGoalRun(payload);
    setStarting(false);
    if (!result) {
      setMessage("Goal runner is not available from this runtime.");
      return;
    }
    setMessage("Goal queued.");
    await refresh();
    setLaunchOpen(false);
  };

  const openGoalView = (run: GoalRun) => {
    setSelectedRunId(run.id);
    setWorkspaceOpen(true);
  };

  const openGoalThread = async (threadId: string) => {
    const opened = await openThreadTarget(runtime, threadId);
    if (!opened) {
      setMessage(`Thread ${threadId} is not loaded yet.`);
      return;
    }
    navigateZorai({
      view: "threads",
      returnTarget: {
        view: "goals",
        label: "Return to goal",
        goalRunId: selectedRunId ?? selectedRun?.id,
      },
    });
  };

  const handleControl = async (run: GoalRun, action: "pause" | "resume" | "cancel") => {
    setMessage(null);
    const ok = await controlGoalRun(run.id, action);
    setMessage(ok ? `${formatGoalRunStatus(run.status)} goal updated.` : "Goal action failed.");
    await refresh();
  };

  const launchOverlay = launchOpen ? (
    <div className="zorai-goal-launch-overlay" role="dialog" aria-modal="true" aria-label="Start goal">
      <GoalLaunchPanel
        runtime={runtime}
        supported={supported}
        starting={starting}
        message={message}
        onLaunch={handleStartGoal}
        onClose={() => setLaunchOpen(false)}
      />
    </div>
  ) : null;

  if (workspaceOpen) {
    return (
      <section className="zorai-feature-surface zorai-goals-surface zorai-goal-view-surface">
        <div className="zorai-view-header zorai-goal-view-header">
          <div>
            <div className="zorai-kicker">Goal View</div>
            <h1>{selectedRun ? selectedRun.title || selectedRun.goal : "Goal workspace"}</h1>
            <p>{selectedRun ? `${formatGoalRunStatus(selectedRun.status)} · ${summarizeGoalRunStep(selectedRun)}` : "Select a goal run."}</p>
          </div>
          <div className="zorai-card-actions">
            {returnTarget && onReturnTarget ? (
              <button type="button" className="zorai-ghost-button" onClick={onReturnTarget}>
                {returnTarget.label}
              </button>
            ) : null}
            <button type="button" className="zorai-ghost-button" onClick={() => setWorkspaceOpen(false)}>
              Back to goals
            </button>
          </div>
        </div>
        <GoalWorkspacePanel run={selectedRun} onRefresh={refresh} onMessage={setMessage} onOpenThread={openGoalThread} />
        <ThreadFilePreviewOverlay />
        {message ? <div className="zorai-inline-note">{message}</div> : null}
        {launchOverlay}
      </section>
    );
  }

  return (
    <section className="zorai-feature-surface zorai-goals-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Goals</div>
          <h1>Run and supervise durable agent goals.</h1>
          <p>One worker pursues the outcome. Completeness is decided only by Accept, Soft reject, or Hard reject.</p>
        </div>
        <div className="zorai-card-actions">
          {returnTarget && onReturnTarget ? (
            <button type="button" className="zorai-ghost-button" onClick={onReturnTarget}>
              {returnTarget.label}
            </button>
          ) : null}
        </div>
      </div>

      <div className="zorai-metric-grid">
        <Metric label="Active" value={metrics.active} />
        <Metric label="Awaiting Review" value={metrics.waiting} />
        <Metric label="Completed" value={metrics.completed} />
        <Metric label="Total Runs" value={metrics.total} />
      </div>

      <div className="zorai-goals-layout zorai-goals-layout--runs-only">
        <div className="zorai-panel zorai-goal-list">
          <div>
            <div className="zorai-section-label">Goal Runs</div>
            <h2>Live supervision</h2>
          </div>
          {visibleGoalRuns.length === 0 ? (
            <div className="zorai-empty-state">No goal runs are loaded yet.</div>
          ) : (
            visibleGoalRuns.map((run) => (
              <GoalRunCard
                key={run.id}
                run={run}
                selected={run.id === selectedRun?.id}
                onSelect={() => setSelectedRunId(run.id)}
                onOpen={() => openGoalView(run)}
                onControl={handleControl}
              />
            ))
          )}
        </div>
      </div>
      {launchOverlay}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="zorai-metric-card">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function GoalRunCard({
  run,
  selected,
  onSelect,
  onOpen,
  onControl,
}: {
  run: GoalRun;
  selected: boolean;
  onSelect: () => void;
  onOpen: () => void;
  onControl: (run: GoalRun, action: "pause" | "resume" | "cancel") => void;
}) {
  const todos = latestGoalRunTodoSnapshot(run).slice(0, 4);
  return (
    <article className={["zorai-run-card", selected ? "zorai-run-card--active" : ""].filter(Boolean).join(" ")}>
      <div className="zorai-run-card__header">
        <div>
          <strong>{run.title || run.goal}</strong>
          <span>{summarizeGoalRunStep(run)}</span>
        </div>
        <span className="zorai-status-pill">{formatGoalRunStatus(run.status)}</span>
      </div>
      <CollapsibleSummary text={run.result || run.error || run.plan_summary || run.goal} />
      <div className="zorai-run-card__meta">
        <span>{run.thread_id ? "Worker attached" : "No worker thread"}</span>
        <span>{formatGoalRunDuration(run.duration_ms)}</span>
      </div>
      {todos.length > 0 ? (
        <div className="zorai-todo-strip">
          {todos.map((todo) => <span key={todo.id}>{todo.content}</span>)}
        </div>
      ) : null}
      {isGoalRunActive(run) ? (
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" onClick={onOpen}>Open goal view</button>
          <button type="button" className="zorai-ghost-button" onClick={onSelect}>Select</button>
          {run.status === "paused" ? (
            <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "resume")}>Resume</button>
          ) : (
            <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "pause")}>Pause</button>
          )}
          <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "cancel")}>Cancel</button>
        </div>
      ) : (
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" onClick={onOpen}>Open goal view</button>
          <button type="button" className="zorai-ghost-button" onClick={onSelect}>Select</button>
        </div>
      )}
    </article>
  );
}
