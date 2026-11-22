import { useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentTodoItem, AgentThread } from "@/lib/agentStore";
import { useAuditStore } from "@/lib/auditStore";
import { formatGoalRunStatus, isGoalRunTerminal, type GoalRun } from "@/lib/goalRuns";
import { ActivityInbox } from "./ActivityInbox";
import { inboxSectionCounts, type InboxFilter } from "./inboxModel";
import { useInboxViewStore, type ActivityTab } from "./inboxStore";
import { useInboxItems } from "./useInboxItems";
import { openThreadTarget } from "../threads/openThreadTarget";
import { navigateZorai } from "../../shell/zoraiNavigationEvents";

const RAIL_FILTERS: ReadonlyArray<{ id: InboxFilter; label: string }> = [
  { id: "all", label: "Inbox" },
  { id: "needs", label: "Needs you" },
  { id: "replies", label: "Replies" },
  { id: "updates", label: "Updates" },
];

const ACTIVITY_TABS: ReadonlyArray<{ id: ActivityTab; label: string }> = [
  { id: "inbox", label: "Inbox" },
  { id: "timeline", label: "Timeline" },
  { id: "planner", label: "Planner" },
];

const TODO_MARKS: Record<AgentTodoItem["status"], string> = {
  completed: "✓",
  in_progress: "◐",
  pending: "○",
  blocked: "⊘",
};

export function ActivityRail() {
  const items = useInboxItems();
  const counts = useMemo(() => inboxSectionCounts(items), [items]);
  const tab = useInboxViewStore((state) => state.tab);
  const filter = useInboxViewStore((state) => state.filter);
  const setTab = useInboxViewStore((state) => state.setTab);
  const setFilter = useInboxViewStore((state) => state.setFilter);
  const auditEntries = useAuditStore((state) => state.entries);
  const { scopedOperationalEvents } = useAgentChatPanelRuntime();

  return (
    <div className="zorai-rail-stack">
      {RAIL_FILTERS.map((entry) => (
        <button
          key={entry.id}
          type="button"
          className={["zorai-rail-card", "zorai-rail-card--button", tab === "inbox" && filter === entry.id ? "zorai-rail-card--active" : ""].filter(Boolean).join(" ")}
          onClick={() => {
            setTab("inbox");
            setFilter(entry.id);
          }}
        >
          <strong className="zorai-rail-card__title">{entry.label}</strong>
          <span className="zorai-rail-card__status">{counts[entry.id] > 0 ? `${counts[entry.id]} unread` : "Nothing new"}</span>
        </button>
      ))}
      <button
        type="button"
        className={["zorai-rail-card", "zorai-rail-card--button", tab === "timeline" ? "zorai-rail-card--active" : ""].filter(Boolean).join(" ")}
        onClick={() => setTab("timeline")}
      >
        <strong className="zorai-rail-card__title">Timeline</strong>
        <span className="zorai-rail-card__status">{scopedOperationalEvents.length} events · {auditEntries.length} audit entries</span>
      </button>
    </div>
  );
}

export function ActivityView() {
  const runtime = useAgentChatPanelRuntime();
  const storedTab = useInboxViewStore((state) => state.tab);
  const setTab = useInboxViewStore((state) => state.setTab);
  const tab: ActivityTab = ACTIVITY_TABS.some((entry) => entry.id === storedTab) ? storedTab : "inbox";
  const inboxItems = useInboxItems();
  const inboxUnread = useMemo(() => inboxSectionCounts(inboxItems).all, [inboxItems]);
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();

  const operationalEvents = useMemo(() => {
    return runtime.scopedOperationalEvents.filter((event) => {
      if (!normalizedQuery) return true;
      return [event.kind, event.command ?? "", event.message ?? "", event.blastRadius ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
  }, [normalizedQuery, runtime.scopedOperationalEvents]);

  const plannerThreads = useMemo(() => {
    return Object.entries(runtime.daemonTodosByThread)
      .filter(([, todos]) => todos.length > 0)
      .map(([threadId, todos]) => ({ threadId, todos, title: plannerThreadTitle(threadId, runtime.threads) }))
      .filter((entry) => !normalizedQuery || `${entry.title} ${entry.todos.map((todo) => todo.content).join(" ")}`.toLowerCase().includes(normalizedQuery))
      .sort((a, b) => Number(isActivePlan(b.todos)) - Number(isActivePlan(a.todos)));
  }, [normalizedQuery, runtime.daemonTodosByThread, runtime.threads]);

  const plannerGoals = useMemo(() => {
    return runtime.goalRunsForTrace
      .filter((goal) => !normalizedQuery || `${goal.title} ${goal.goal}`.toLowerCase().includes(normalizedQuery))
      .sort((a, b) => Number(isActiveGoal(b)) - Number(isActiveGoal(a)) || (b.created_at || 0) - (a.created_at || 0));
  }, [normalizedQuery, runtime.goalRunsForTrace]);
  const [planPage, setPlanPage] = useState(0);
  const [goalPage, setGoalPage] = useState(0);
  const shownPlans = pageOf(plannerThreads, planPage);
  const shownGoals = pageOf(plannerGoals, goalPage);

  const openThread = async (threadId: string) => {
    if (await openThreadTarget(runtime, threadId)) navigateZorai({ view: "threads" });
  };
  const openGoal = (goalRunId: string) => navigateZorai({ view: "goals", goalRunId });
  const relatedThreadId = (...candidates: Array<string | null | undefined>) => {
    for (const candidate of candidates) {
      if (!candidate) continue;
      const thread = runtime.threads.find((entry) => entry.id === candidate || entry.daemonThreadId === candidate);
      if (thread) return thread.daemonThreadId || thread.id;
    }
    return null;
  };

  return (
    <section className="zorai-feature-surface zorai-activity-surface">
      <div className="zorai-view-header zorai-activity-header">
        <div>
          <div className="zorai-kicker">Inbox</div>
          <h1>Everything waiting for you, in one place.</h1>
          <p>Approve, answer, and reply to agents without opening their threads.</p>
        </div>
      </div>

      <div className="zorai-activity-tabs" role="tablist" aria-label="Inbox sections">
        {ACTIVITY_TABS.map((entry) => (
          <button
            type="button"
            role="tab"
            key={entry.id}
            aria-selected={tab === entry.id}
            className={["zorai-inbox__filter", tab === entry.id ? "zorai-inbox__filter--active" : ""].filter(Boolean).join(" ")}
            onClick={() => setTab(entry.id)}
          >
            {entry.label}
            {entry.id === "inbox" && inboxUnread > 0 ? <span className="zorai-inbox__count">{inboxUnread}</span> : null}
          </button>
        ))}
        {tab !== "inbox" ? (
          <input
            className="zorai-input zorai-activity-tabs__search"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setPlanPage(0);
              setGoalPage(0);
            }}
            placeholder={tab === "timeline" ? "Search events…" : "Search plans…"}
          />
        ) : null}
      </div>

      {tab === "inbox" ? <ActivityInbox items={inboxItems} onOpenThread={(threadId) => void openThread(threadId)} onOpenGoal={openGoal} /> : null}

      {tab === "timeline" ? (
        <div className="zorai-timeline">
          {operationalEvents.length === 0 ? <div className="zorai-empty-state">No operational events match.</div> : (
            <ol className="zorai-timeline__list">
              {operationalEvents.slice(0, 200).map((event) => {
                const threadId = relatedThreadId(event.sessionId, event.surfaceId);
                return (
                  <li key={event.id} className="zorai-timeline__row" title={activityProvenance(event)}>
                    <time className="zorai-timeline__time">{formatClock(event.timestamp)}</time>
                    <span className={`zorai-timeline__kind zorai-timeline__kind--${timelineTone(event.kind)}`}>{event.kind}</span>
                    <code className="zorai-timeline__text">{event.command || event.message || event.blastRadius || "Runtime event"}</code>
                    {threadId ? (
                      <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void openThread(threadId)}>Thread</button>
                    ) : <span />}
                  </li>
                );
              })}
            </ol>
          )}
        </div>
      ) : null}

      {tab === "planner" ? (
        <div className="zorai-planner">
          <section className="zorai-planner__column" aria-label="Thread plans">
            <div className="zorai-section-label">Thread plans</div>
            {plannerThreads.length === 0 ? <div className="zorai-empty-state">No active planner todos.</div> : shownPlans.map((entry) => (
              <PlanCard key={entry.threadId} title={entry.title} subtitle={entry.threadId} todos={entry.todos} actionLabel="Open thread" onAction={() => void openThread(entry.threadId)} />
            ))}
            <Pager page={planPage} total={plannerThreads.length} onPage={setPlanPage} />
          </section>
          <section className="zorai-planner__column" aria-label="Goals">
            <div className="zorai-section-label">Goals</div>
            {plannerGoals.length === 0 ? <div className="zorai-empty-state">No goals are loaded.</div> : shownGoals.map((goal) => (
              <GoalCard key={goal.id} goal={goal} onOpen={() => openGoal(goal.id)} />
            ))}
            <Pager page={goalPage} total={plannerGoals.length} onPage={setGoalPage} />
          </section>
        </div>
      ) : null}
    </section>
  );
}

function PlanCard({ title, subtitle, todos, actionLabel, onAction }: { title: string; subtitle: string; todos: AgentTodoItem[]; actionLabel: string; onAction: () => void }) {
  const [expanded, setExpanded] = useState(false);
  const ordered = [...todos].sort((a, b) => a.position - b.position);
  const done = ordered.filter((todo) => todo.status === "completed").length;
  const hiddenDone = expanded ? 0 : Math.max(0, done - 2);
  let skipped = 0;
  const shown = ordered.filter((todo) => {
    if (todo.status !== "completed" || expanded) return true;
    skipped += 1;
    return skipped > hiddenDone;
  });

  return (
    <article className="zorai-plan-card">
      <header className="zorai-plan-card__header">
        <div className="zorai-plan-card__titles">
          <strong>{title}</strong>
          <span>{subtitle}</span>
        </div>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={onAction}>{actionLabel}</button>
      </header>
      <div className="zorai-plan-card__progress" role="progressbar" aria-valuemin={0} aria-valuemax={ordered.length} aria-valuenow={done} aria-label={`${done} of ${ordered.length} done`}>
        <span style={{ width: `${ordered.length ? (done / ordered.length) * 100 : 0}%` }} />
      </div>
      <div className="zorai-plan-card__meta">{done} of {ordered.length} done</div>
      {hiddenDone > 0 ? (
        <button type="button" className="zorai-plan-card__more" onClick={() => setExpanded(true)}>Show {hiddenDone} earlier completed</button>
      ) : null}
      <ul className="zorai-plan-card__todos">
        {shown.map((todo) => (
          <li key={todo.id} className={`zorai-plan-card__todo zorai-plan-card__todo--${todo.status}`}>
            <span className="zorai-plan-card__mark" aria-label={todo.status.replace("_", " ")}>{TODO_MARKS[todo.status]}</span>
            <span>{todo.content}</span>
          </li>
        ))}
      </ul>
    </article>
  );
}

function GoalCard({ goal, onOpen }: { goal: GoalRun; onOpen: () => void }) {
  const steps = goal.steps ?? [];
  const done = steps.filter((step) => step.status === "completed").length;
  const recent = (goal.events ?? []).slice(-3).reverse();
  return (
    <article className="zorai-plan-card">
      <header className="zorai-plan-card__header">
        <div className="zorai-plan-card__titles">
          <strong>{goal.title || goal.goal}</strong>
          <span><span className={`zorai-plan-card__status zorai-plan-card__status--${goal.status}`}>{formatGoalRunStatus(goal.status)}</span>{goal.current_step_title ? ` · ${goal.current_step_title}` : ""}</span>
        </div>
        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={onOpen}>Open goal</button>
      </header>
      {steps.length > 0 ? (
        <>
          <div className="zorai-plan-card__progress" role="progressbar" aria-valuemin={0} aria-valuemax={steps.length} aria-valuenow={done} aria-label={`${done} of ${steps.length} steps done`}>
            <span style={{ width: `${(done / steps.length) * 100}%` }} />
          </div>
          <div className="zorai-plan-card__meta">{done} of {steps.length} steps done</div>
        </>
      ) : null}
      {recent.length > 0 ? (
        <ul className="zorai-plan-card__events">
          {recent.map((event) => (
            <li key={event.id}>
              <time>{formatClock(event.timestamp)}</time>
              <span>{event.message}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </article>
  );
}

const PLANNER_PAGE_SIZE = 10;

function pageOf<T>(items: T[], page: number): T[] {
  const lastPage = Math.max(0, Math.ceil(items.length / PLANNER_PAGE_SIZE) - 1);
  const safePage = Math.min(page, lastPage);
  return items.slice(safePage * PLANNER_PAGE_SIZE, (safePage + 1) * PLANNER_PAGE_SIZE);
}

function Pager({ page, total, onPage }: { page: number; total: number; onPage: (page: number) => void }) {
  const pages = Math.ceil(total / PLANNER_PAGE_SIZE);
  if (pages <= 1) return null;
  const current = Math.min(page, pages - 1);
  return (
    <div className="zorai-planner__pager">
      <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" disabled={current === 0} onClick={() => onPage(current - 1)}>Previous</button>
      <span>{current + 1} / {pages} · {total} total</span>
      <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" disabled={current >= pages - 1} onClick={() => onPage(current + 1)}>Next</button>
    </div>
  );
}

function isActiveGoal(goal: GoalRun): boolean {
  return !isGoalRunTerminal(goal);
}

function isActivePlan(todos: AgentTodoItem[]): boolean {
  return todos.some((todo) => todo.status === "in_progress" || todo.status === "pending");
}

function plannerThreadTitle(threadId: string, threads: AgentThread[]): string {
  const thread = threads.find((entry) => entry.daemonThreadId === threadId || entry.id === threadId);
  if (thread?.title) return thread.title;
  if (threadId.startsWith("dm:")) {
    const [, from, to] = threadId.split(":");
    if (from && to) return `${capitalize(from)} → ${capitalize(to)}`;
  }
  return threadId;
}

function capitalize(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function timelineTone(kind: string): string {
  if (/fail|error|denied|blocked/i.test(kind)) return "danger";
  if (/approval|warning|risk/i.test(kind)) return "warning";
  if (/finish|complete|success/i.test(kind)) return "success";
  return "neutral";
}

function activityProvenance(item: { id: string; paneId?: string | null; workspaceId?: string | null; surfaceId?: string | null; sessionId?: string | null }): string {
  return [
    `event: ${item.id}`,
    item.sessionId ? `session: ${item.sessionId}` : "",
    item.paneId ? `pane: ${item.paneId}` : "",
    item.surfaceId ? `surface: ${item.surfaceId}` : "",
    item.workspaceId ? `workspace: ${item.workspaceId}` : "",
  ].filter(Boolean).join(" · ");
}

function formatClock(timestamp: number): string {
  if (!Number.isFinite(timestamp)) return "";
  const date = new Date(timestamp);
  const sameDay = date.toDateString() === new Date().toDateString();
  return sameDay ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }) : date.toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}
