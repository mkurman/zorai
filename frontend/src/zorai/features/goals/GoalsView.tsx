import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { Presence } from "@/components/Presence";
import {
  dispatchGoalSelect,
  firstLinePreview,
  GOAL_SELECT_EVENT,
  goalStatusTone,
  groupGoalRunsForSupervision,
  normalizeGoalText,
  searchAndFilterGoalRuns,
  useGoalSelectionStore,
  type GoalFilterId,
  type GoalSortId,
} from "./goalPresentation";

const activeStatuses = new Set(["queued", "planning", "running", "awaiting_approval", "awaiting_review", "paused"]);
const GOAL_LAUNCH_EVENT = "zorai-goal-launch";

export function GoalsRail() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const selectedRunId = useGoalSelectionStore((state) => state.selectedRunId);
  const activeGoals = goalRunsForTrace.filter((goal) => activeStatuses.has(goal.status));

  return (
    <div className="zorai-rail-stack">
      <button
        type="button"
        className="zorai-primary-button zorai-rail-cta"
        onClick={() => window.dispatchEvent(new Event(GOAL_LAUNCH_EVENT))}
      >
        + New goal
      </button>
      <div className="zorai-section-label">Active Goals ({activeGoals.length})</div>
      {activeGoals.length === 0 ? (
        <div className="zorai-empty">No goal runs are active.</div>
      ) : (
        activeGoals.slice(0, 8).map((goal) => {
          const tone = goalStatusTone(goal.status);
          const isSelected = selectedRunId === goal.id;
          return (
            <button
              type="button"
              key={goal.id}
              className={["zorai-rail-card", "zorai-rail-card--button", isSelected ? "zorai-rail-card--active" : ""].filter(Boolean).join(" ")}
              onClick={() => dispatchGoalSelect(goal.id, false)}
              onDoubleClick={() => dispatchGoalSelect(goal.id, true)}
            >
              <strong className="zorai-rail-card__title">{goal.title || goal.goal}</strong>
              <span className="zorai-rail-card__status">
                <i className={`zorai-status-dot zorai-status-dot--${tone}`} aria-hidden="true" />
                {formatGoalRunStatus(goal.status)}
              </span>
            </button>
          );
        })
      )}
    </div>
  );
}

export function GoalsContext() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const selectedRun = useGoalSelectionStore((state) => state.selectedRun);
  const waiting = goalRunsForTrace.filter((goal) => goal.status === "awaiting_approval" || goal.status === "awaiting_review").length;
  const active = goalRunsForTrace.filter(isGoalRunActive).length;
  const failed = goalRunsForTrace.filter((goal) => goal.status === "failed").length;
  const detailText = selectedRun
    ? normalizeGoalText(selectedRun.last_error || selectedRun.error || selectedRun.result || selectedRun.plan_summary || selectedRun.goal)
    : "";

  return (
    <div className="zorai-context-summary">
      <div className="zorai-section-label">Overview</div>
      <div className="zorai-context-stat-grid">
        <div className="zorai-context-stat"><strong>{active}</strong><span>Active</span></div>
        <div className="zorai-context-stat zorai-context-stat--warning"><strong>{waiting}</strong><span>Review</span></div>
        <div className="zorai-context-stat zorai-context-stat--danger"><strong>{failed}</strong><span>Failed</span></div>
      </div>
      {selectedRun ? (
        <>
          <div className="zorai-section-label">Selected Run</div>
          <div className="zorai-context-block">
            <strong>{selectedRun.title || selectedRun.goal}</strong>
            <div className="zorai-context-pills">
              <span className={`zorai-status-pill zorai-status-pill--${goalStatusTone(selectedRun.status)}`}>
                {formatGoalRunStatus(selectedRun.status)}
              </span>
              <span className="zorai-status-pill">{formatGoalRunDuration(selectedRun.duration_ms)}</span>
            </div>
          </div>
          <div className="zorai-context-actions">
            <button
              type="button"
              className="zorai-ghost-button zorai-ghost-button--primary-soft"
              onClick={() => dispatchGoalSelect(selectedRun.id, true)}
            >
              Open goal view
            </button>
          </div>
          {detailText ? (
            <div className="zorai-context-block">
              <div className="zorai-section-label">
                {selectedRun.status === "failed" ? "Rejection / Error" : "Summary"}
              </div>
              <p className="zorai-context-prose">{detailText}</p>
            </div>
          ) : null}
        </>
      ) : (
        <div className="zorai-context-block">
          <strong>Workspace modes</strong>
          <span>Worker thread plus Accept / Soft reject / Hard reject</span>
        </div>
      )}
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
  const [filter, setFilter] = useState<GoalFilterId>("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<GoalSortId>("newest");
  const searchInputRef = useRef<HTMLInputElement | null>(null);
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
    const failed = visibleGoalRuns.filter((run) => run.status === "failed").length;
    return { active, waiting, completed, failed, total: visibleGoalRuns.length };
  }, [visibleGoalRuns]);

  const filteredRuns = useMemo(
    () => searchAndFilterGoalRuns(visibleGoalRuns, { filter, query, sort }),
    [filter, query, sort, visibleGoalRuns],
  );
  const groupedRuns = useMemo(() => groupGoalRunsForSupervision(filteredRuns), [filteredRuns]);
  const hasActiveFilters = filter !== "all" || query.trim().length > 0 || sort !== "newest";

  const selectedRun = useMemo(() => {
    return visibleGoalRuns.find((run) => run.id === selectedRunId) ?? visibleGoalRuns[0] ?? null;
  }, [selectedRunId, visibleGoalRuns]);

  useEffect(() => {
    if (!selectedRunId && visibleGoalRuns[0]) {
      setSelectedRunId(visibleGoalRuns[0].id);
    }
  }, [selectedRunId, visibleGoalRuns]);

  useEffect(() => {
    useGoalSelectionStore.getState().setSelectedRun(selectedRun);
  }, [selectedRun]);

  useEffect(() => {
    if (!openGoalRunRequest?.id) return;
    setSelectedRunId(openGoalRunRequest.id);
    setWorkspaceOpen(true);
  }, [openGoalRunRequest?.id, openGoalRunRequest?.nonce]);

  useEffect(() => {
    const onLaunch = () => setLaunchOpen(true);
    const onSelect = (event: Event) => {
      const detail = (event as CustomEvent<{ goalRunId?: string; openWorkspace?: boolean }>).detail;
      if (!detail?.goalRunId) return;
      setSelectedRunId(detail.goalRunId);
      if (detail.openWorkspace) setWorkspaceOpen(true);
    };
    window.addEventListener(GOAL_LAUNCH_EVENT, onLaunch);
    window.addEventListener(GOAL_SELECT_EVENT, onSelect);
    return () => {
      window.removeEventListener(GOAL_LAUNCH_EVENT, onLaunch);
      window.removeEventListener(GOAL_SELECT_EVENT, onSelect);
    };
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

  const launchOverlay = (
    <Presence value={launchOpen}>{() => (
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
    )}</Presence>
  );

  if (workspaceOpen) {
    const viewTone = selectedRun ? goalStatusTone(selectedRun.status) : "muted";
    const viewStep = selectedRun ? firstLinePreview(summarizeGoalRunStep(selectedRun), 140) : "Select a goal run.";
    return (
      <section className="zorai-feature-surface zorai-goals-surface zorai-goal-view-surface">
        <div className="zorai-view-header zorai-view-header--compact zorai-goal-view-header">
          <div className="zorai-goal-view-header__title">
            <div className="zorai-goal-view-header__row">
              <h1>{selectedRun ? selectedRun.title || selectedRun.goal : "Goal workspace"}</h1>
              {selectedRun ? (
                <span className={`zorai-status-pill zorai-status-pill--${viewTone}`}>
                  <i className={`zorai-status-dot zorai-status-dot--${viewTone}`} aria-hidden="true" />
                  {formatGoalRunStatus(selectedRun.status)}
                </span>
              ) : null}
              {selectedRun?.duration_ms ? (
                <span className="zorai-goal-view-header__duration">{formatGoalRunDuration(selectedRun.duration_ms)}</span>
              ) : null}
            </div>
            <p title={selectedRun ? normalizeGoalText(summarizeGoalRunStep(selectedRun)) : undefined}>{viewStep}</p>
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
      <div className="zorai-view-header zorai-view-header--compact">
        <div>
          <h1>Goal Supervision</h1>
          <p>Supervise autonomous runs, review checkpoints, and manage outcomes.</p>
        </div>
        <div className="zorai-card-actions">
          {returnTarget && onReturnTarget ? (
            <button type="button" className="zorai-ghost-button" onClick={onReturnTarget}>
              {returnTarget.label}
            </button>
          ) : null}
        </div>
      </div>

      <div className="zorai-goals-controls">
        <div className="zorai-metric-strip" role="tablist" aria-label="Filter goal runs">
          <MetricFilter label="All Runs" value={metrics.total} active={filter === "all"} onClick={() => setFilter("all")} />
          <MetricFilter label="Awaiting Review" value={metrics.waiting} tone={metrics.waiting > 0 ? "warning" : undefined} active={filter === "awaiting_review"} onClick={() => setFilter("awaiting_review")} />
          <MetricFilter label="Active" value={metrics.active} tone={metrics.active > 0 ? "info" : undefined} active={filter === "active"} onClick={() => setFilter("active")} />
          <MetricFilter label="Completed" value={metrics.completed} tone="success" active={filter === "completed"} onClick={() => setFilter("completed")} />
          <MetricFilter label="Failed" value={metrics.failed} tone={metrics.failed > 0 ? "danger" : undefined} active={filter === "failed"} onClick={() => setFilter("failed")} />
        </div>
        <div className="zorai-surface-filterbar">
          <div className="zorai-search-field zorai-surface-search">
            <input
              ref={searchInputRef}
              className="zorai-search-input"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape" && query) setQuery("");
              }}
              placeholder="Search goals, prompts, or verdicts…"
              aria-label="Search goals"
            />
            {query ? (
              <button
                type="button"
                className="zorai-search-field__clear"
                title="Clear search"
                aria-label="Clear goal search"
                onClick={() => {
                  setQuery("");
                  searchInputRef.current?.focus();
                }}
              >
                ×
              </button>
            ) : null}
          </div>
          <select
            className="zorai-surface-select"
            aria-label="Sort goal runs"
            value={sort}
            onChange={(event) => setSort(event.target.value as GoalSortId)}
          >
            <option value="newest">Sort: Newest</option>
            <option value="attention">Sort: Needs review first</option>
            <option value="duration">Sort: Longest duration</option>
          </select>
          {hasActiveFilters ? (
            <button
              type="button"
              className="zorai-surface-clear"
              onClick={() => {
                setFilter("all");
                setQuery("");
                setSort("newest");
              }}
            >
              Reset ({filteredRuns.length}/{visibleGoalRuns.length})
            </button>
          ) : null}
        </div>
      </div>

      <div className="zorai-goals-layout zorai-goals-layout--runs-only">
        {filteredRuns.length === 0 ? (
          <div className="zorai-panel zorai-goal-list">
            <div className="zorai-empty-state">
              {visibleGoalRuns.length === 0 ? "No goal runs are loaded yet." : "No goal runs match your search or filter."}
            </div>
          </div>
        ) : filter === "all" && sort === "newest" ? (
          <div className="zorai-goal-groups">
            {groupedRuns.awaitingReview.length > 0 ? (
              <GoalRunSection
                title="Needs Review"
                count={groupedRuns.awaitingReview.length}
                tone="warning"
                runs={groupedRuns.awaitingReview}
                selectedId={selectedRun?.id ?? null}
                onSelect={setSelectedRunId}
                onOpen={openGoalView}
                onControl={handleControl}
              />
            ) : null}
            {groupedRuns.live.length > 0 ? (
              <GoalRunSection
                title="Live Supervision"
                count={groupedRuns.live.length}
                tone="info"
                runs={groupedRuns.live}
                selectedId={selectedRun?.id ?? null}
                onSelect={setSelectedRunId}
                onOpen={openGoalView}
                onControl={handleControl}
              />
            ) : null}
            {groupedRuns.history.length > 0 ? (
              <GoalRunSection
                title="Recent History"
                count={groupedRuns.history.length}
                runs={groupedRuns.history}
                selectedId={selectedRun?.id ?? null}
                onSelect={setSelectedRunId}
                onOpen={openGoalView}
                onControl={handleControl}
              />
            ) : null}
          </div>
        ) : (
          <div className="zorai-panel zorai-goal-list">
            {filteredRuns.map((run) => (
              <GoalRunCard
                key={run.id}
                run={run}
                selected={run.id === selectedRun?.id}
                onSelect={() => setSelectedRunId(run.id)}
                onOpen={() => openGoalView(run)}
                onControl={handleControl}
              />
            ))}
          </div>
        )}
      </div>
      {launchOverlay}
    </section>
  );
}

function MetricFilter({
  label,
  value,
  tone,
  active,
  onClick,
}: {
  label: string;
  value: number;
  tone?: "info" | "warning" | "success" | "danger";
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={[
        "zorai-metric-chip",
        tone ? `zorai-metric-chip--${tone}` : "",
        active ? "is-active" : "",
      ].filter(Boolean).join(" ")}
      onClick={onClick}
    >
      <strong>{value}</strong>
      <span>{label}</span>
    </button>
  );
}

function GoalRunSection({
  title,
  count,
  tone,
  runs,
  selectedId,
  onSelect,
  onOpen,
  onControl,
}: {
  title: string;
  count: number;
  tone?: "info" | "warning";
  runs: GoalRun[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onOpen: (run: GoalRun) => void;
  onControl: (run: GoalRun, action: "pause" | "resume" | "cancel") => void;
}) {
  return (
    <div className="zorai-panel zorai-goal-list">
      <div className="zorai-goal-section-head">
        <h2>{title}</h2>
        <span className={["zorai-goal-section-count", tone ? `zorai-goal-section-count--${tone}` : ""].filter(Boolean).join(" ")}>
          {count}
        </span>
      </div>
      {runs.map((run) => (
        <GoalRunCard
          key={run.id}
          run={run}
          selected={run.id === selectedId}
          onSelect={() => onSelect(run.id)}
          onOpen={() => onOpen(run)}
          onControl={onControl}
        />
      ))}
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
  const tone = goalStatusTone(run.status);
  const rawStep = summarizeGoalRunStep(run);
  const stepPreview = firstLinePreview(rawStep, 140);
  const rawBody = run.result || run.error || run.plan_summary || run.goal;
  const normalizedBody = normalizeGoalText(rawBody);
  const showBody = normalizedBody.trim() && normalizedBody.trim() !== normalizeGoalText(rawStep).trim();
  const todos = latestGoalRunTodoSnapshot(run)
    .filter((todo) => todo.content.trim() !== rawStep.trim())
    .slice(0, 3);

  return (
    <article
      className={["zorai-run-card", `zorai-run-card--tone-${tone}`, selected ? "zorai-run-card--active" : ""].filter(Boolean).join(" ")}
      onClick={onSelect}
      onDoubleClick={onOpen}
    >
      <div className="zorai-run-card__header">
        <div>
          <strong>{run.title || run.goal}</strong>
          <span className="zorai-run-card__step" title={normalizeGoalText(rawStep)}>{stepPreview}</span>
        </div>
        <span className={`zorai-status-pill zorai-status-pill--${tone}`}>
          <i className={`zorai-status-dot zorai-status-dot--${tone}`} aria-hidden="true" />
          {formatGoalRunStatus(run.status)}
        </span>
      </div>
      {showBody ? <CollapsibleSummary text={normalizedBody} /> : null}
      <div className="zorai-run-card__meta">
        <span>{run.thread_id ? "Worker attached" : "No worker thread"}</span>
        <span>{formatGoalRunDuration(run.duration_ms)}</span>
      </div>
      {todos.length > 0 ? (
        <ul className="zorai-goal-checklist" aria-label="Goal checkpoints">
          {todos.map((todo) => (
            <li key={todo.id} className={`zorai-goal-checklist__item is-${todo.status}`}>
              <i aria-hidden="true" />
              <span>{normalizeGoalText(todo.content)}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="zorai-card-actions" onClick={(event) => event.stopPropagation()}>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--primary-soft" onClick={onOpen}>
          Open goal view
        </button>
        {isGoalRunActive(run) ? (
          <>
            {run.status === "paused" ? (
              <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "resume")}>Resume</button>
            ) : (
              <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "pause")}>Pause</button>
            )}
            <button type="button" className="zorai-ghost-button zorai-ghost-button--danger" onClick={() => onControl(run, "cancel")}>
              Cancel
            </button>
          </>
        ) : null}
      </div>
    </article>
  );
}
