import { LoadingState, ThreadListSkeleton } from "@/components/LoadingState";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { ACTIVE_RUN_POLL_MS, fetchAgentRuns, IDLE_RUN_POLL_MS, runListIsActive, type AgentRun } from "@/lib/agentRuns";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import {
  buildThreadFilterTabs,
  daemonAgentFilterForThreadTab,
  dateFilters,
  DEFAULT_THREAD_DATE_FILTER,
  filterThreads,
  fixedThreadTabs,
  mergeLocalDraftThreads,
  overlayStoreThreadTitles,
  resolveThreadCreationAgent,
  resolveThreadListSource,
  type DateFilterId,
  type ThreadFilterTab,
} from "./threadFilterModel";
import { openListedThread } from "./openThreadTarget";
import { isThreadLoading, useThreadLoadingStore } from "./threadLoadingStore";
import { threadReadKey, useThreadReadStateStore } from "./threadReadStateStore";
import {
  isWorkerThread,
  sameAgentRunSnapshot,
  sessionActivityLabel,
  threadIsUnread,
  threadIsWorking,
  workerCountForThread,
  workerThreadIds,
} from "./sessionCooperation";
import { ZORAI_FOCUS_SEARCH_EVENT, ZORAI_THREAD_LIST_REFRESH_EVENT, consumePendingFocusSearch } from "../../shell/zoraiNavigationEvents";

const THREAD_FILTER_FETCH_DEBOUNCE_MS = 1000;

export function ThreadsRail() {
  const runtime = useAgentChatPanelRuntime();
  const subAgents = useAgentStore((state) => state.subAgents);
  const storeThreads = useAgentStore((state) => state.threads);
  const threadLoadingByThreadId = useThreadLoadingStore((state) => state.byThreadId);
  const refreshSubAgents = useAgentStore((state) => state.refreshSubAgents);
  const updateThreadTitle = useAgentStore((state) => state.updateThreadTitle);
  const deleteThread = useAgentStore((state) => state.deleteThread);
  const lastReadAtByThread = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const runsRef = useRef(runs);
  runsRef.current = runs;
  const [editingThreadId, setEditingThreadId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [tab, setTab] = useState<ThreadFilterTab>("svarog");
  const [dateFilter, setDateFilter] = useState<DateFilterId>(DEFAULT_THREAD_DATE_FILTER);
  const [fromDate, setFromDate] = useState("");
  const [toDate, setToDate] = useState("");
  const [daemonFilteredThreads, setDaemonFilteredThreads] = useState<AgentThread[] | null>(null);
  const [loadingTab, setLoadingTab] = useState<ThreadFilterTab | null>(tab);
  const pendingFetchIdRef = useRef(0);
  const loadedAgentFilterRef = useRef<string | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const goalThreadIdSet = useMemo(() => goalThreadIds(runtime.goalRunsForTrace), [runtime.goalRunsForTrace]);
  const daemonAgentFilter = useMemo(() => daemonAgentFilterForThreadTab(tab, subAgents), [subAgents, tab]);
  const fetchKey = daemonAgentFilter ?? "__all__";
  const fetchThreadList = runtime.fetchThreadList;
  const sourceThreads = useMemo(() => {
    const baseThreads = overlayStoreThreadTitles(
      mergeLocalDraftThreads(
        resolveThreadListSource(daemonFilteredThreads, runtime.filteredThreads),
        storeThreads,
      ),
      storeThreads,
    );
    return filterThreadsForSearchQuery(baseThreads, runtime.searchQuery);
  }, [daemonFilteredThreads, runtime.filteredThreads, runtime.searchQuery, storeThreads]);
  const displayedThreads = useMemo(() => filterThreads(sourceThreads, {
    tab,
    dateFilter,
    fromDate,
    toDate,
    goalThreadIds: goalThreadIdSet,
    subAgents,
  }), [dateFilter, fromDate, goalThreadIdSet, sourceThreads, subAgents, tab, toDate]);
  const threadTabs = useMemo(() => buildThreadFilterTabs(
    runtime.filteredThreads,
    subAgents,
    goalThreadIdSet,
  ), [goalThreadIdSet, runtime.filteredThreads, subAgents]);
  const agentFilterOptions = useMemo(
    () => threadTabs.filter((item) => item.id.startsWith("agent:")),
    [threadTabs],
  );
  const threadCreationAgent = useMemo(
    () => resolveThreadCreationAgent(tab, subAgents),
    [subAgents, tab],
  );
  const spawnedWorkerIds = useMemo(() => workerThreadIds(runs), [runs]);
  const listedThreads = useMemo(
    () => tab === "internal"
      ? displayedThreads
      : displayedThreads.filter((thread) => !isWorkerThread(thread, spawnedWorkerIds)),
    [displayedThreads, spawnedWorkerIds, tab],
  );

  useEffect(() => {
    void refreshSubAgents();
  }, [refreshSubAgents]);

  useEffect(() => {
    let cancelled = false;
    let inFlight = false;
    let lastFetch = 0;
    const load = () => {
      if (cancelled || inFlight || document.hidden) return;
      const now = Date.now();
      const wait = runListIsActive(runsRef.current) ? ACTIVE_RUN_POLL_MS : IDLE_RUN_POLL_MS;
      if (lastFetch !== 0 && now - lastFetch < wait) return;
      lastFetch = now;
      inFlight = true;
      void fetchAgentRuns().then((next) => {
        if (!cancelled) setRuns((current) => sameAgentRunSnapshot(current, next) ? current : next);
      }).finally(() => {
        inFlight = false;
      });
    };
    load();
    const timer = window.setInterval(load, ACTIVE_RUN_POLL_MS);
    const clock = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      window.clearInterval(clock);
    };
  }, []);

  useEffect(() => {
    const focusSearch = () => {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    };
    if (consumePendingFocusSearch()) focusSearch();
    const onFocusSearch = () => {
      if (consumePendingFocusSearch()) focusSearch();
    };
    window.addEventListener(ZORAI_FOCUS_SEARCH_EVENT, onFocusSearch);
    return () => window.removeEventListener(ZORAI_FOCUS_SEARCH_EVENT, onFocusSearch);
  }, []);

  useEffect(() => {
    pendingFetchIdRef.current += 1;
    const fetchId = pendingFetchIdRef.current;
    if (loadedAgentFilterRef.current !== fetchKey) setDaemonFilteredThreads(null);
    setLoadingTab(tab);

    const timeoutId = window.setTimeout(() => {
      void fetchThreadList({ agentFilter: daemonAgentFilter, includeInternal: true })
        .then((threads) => {
          if (pendingFetchIdRef.current !== fetchId) return;
          startTransition(() => {
            loadedAgentFilterRef.current = fetchKey;
            setDaemonFilteredThreads(threads);
            setLoadingTab(null);
          });
        })
        .catch(() => {
          if (pendingFetchIdRef.current !== fetchId) return;
          // Keep the last successful list for the same filter instead of
          // blanking the rail into "No threads match this search" while the
          // daemon is slow; only fall back when we never loaded this filter.
          setDaemonFilteredThreads((prev) => (loadedAgentFilterRef.current === fetchKey ? prev : null));
          setLoadingTab(null);
        });
    }, loadedAgentFilterRef.current == null ? 0 : THREAD_FILTER_FETCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timeoutId);
  }, [daemonAgentFilter, fetchKey, fetchThreadList, tab]);

  const refreshSelectedTab = useCallback(() => {
    pendingFetchIdRef.current += 1;
    const fetchId = pendingFetchIdRef.current;
    setLoadingTab(tab);
    void fetchThreadList({ agentFilter: daemonAgentFilter, includeInternal: true })
      .then((threads) => {
        if (pendingFetchIdRef.current !== fetchId) return;
        startTransition(() => {
          loadedAgentFilterRef.current = fetchKey;
          setDaemonFilteredThreads(threads);
          setLoadingTab(null);
        });
      })
      .catch(() => {
        if (pendingFetchIdRef.current === fetchId) setLoadingTab(null);
      });
  }, [daemonAgentFilter, fetchKey, fetchThreadList, tab]);

  useEffect(() => {
    const onRefresh = () => refreshSelectedTab();
    window.addEventListener(ZORAI_THREAD_LIST_REFRESH_EVENT, onRefresh);
    return () => window.removeEventListener(ZORAI_THREAD_LIST_REFRESH_EVENT, onRefresh);
  }, [refreshSelectedTab]);

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-rail-actions">
        <button
          type="button"
          className="zorai-primary-button"
          onClick={() => {
            runtime.createThread({
              workspaceId: runtime.activeWorkspace?.id ?? null,
              agentId: threadCreationAgent?.id ?? null,
              agentName: threadCreationAgent?.name ?? null,
            });
            runtime.setChatBackView("threads");
            runtime.setView("chat");
          }}
        >
          + New Thread
        </button>
        <RefreshButton onClick={refreshSelectedTab} disabled={loadingTab !== null} busy={loadingTab !== null} />
      </div>
      <input ref={searchInputRef} className="zorai-search-input" value={runtime.searchQuery} onChange={(event) => runtime.setSearchQuery(event.target.value)} placeholder="Search threads" />
      <div className="zorai-thread-filters">
        <summary>Filters</summary>
      <div className="zorai-thread-filter-tabs" aria-label="Thread source filters">
        {fixedThreadTabs.map((item) => (
          <button
            type="button"
            key={item.id}
            className={["zorai-thread-filter-tab", tab === item.id ? "zorai-thread-filter-tab--active" : "", loadingTab === item.id ? "zorai-thread-filter-tab--loading" : ""].filter(Boolean).join(" ")}
            onClick={() => setTab(item.id)}
            aria-busy={loadingTab === item.id}
          >
            {item.label}
            {loadingTab === item.id ? <LoadingState size={12} className="zorai-thread-filter-tab__spinner" /> : null}
          </button>
        ))}
      </div>
      {agentFilterOptions.length > 0 ? (
        <div className="zorai-thread-agent-filter">
          <select
            aria-label="Agents and subagents"
            className={tab.startsWith("agent:") ? "zorai-thread-agent-filter--active" : ""}
            value={tab.startsWith("agent:") ? tab : ""}
            onChange={(event) => setTab((event.target.value || "svarog") as ThreadFilterTab)}
          >
            <option value="">Agents & subagents</option>
            {agentFilterOptions.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
          {loadingTab?.startsWith("agent:") ? <LoadingState size={12} className="zorai-thread-filter-tab__spinner" /> : null}
        </div>
      ) : null}
      <div className="zorai-thread-date-filters" aria-label="Thread date filters">
        <select value={dateFilter} onChange={(event) => setDateFilter(event.target.value as DateFilterId)}>
          {dateFilters.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
        </select>
        {dateFilter === "custom" ? (
          <>
            <input type="date" value={fromDate} onChange={(event) => setFromDate(event.target.value)} aria-label="From date" />
            <input type="date" value={toDate} onChange={(event) => setToDate(event.target.value)} aria-label="To date" />
          </>
        ) : null}
      </div>
      </div>
      <div className="zorai-thread-list" aria-busy={loadingTab !== null}>
        {loadingTab && daemonFilteredThreads === null ? (
          <ThreadListSkeleton />
        ) : listedThreads.length === 0 ? (
          <div className="zorai-empty">No threads match this search.</div>
        ) : listedThreads.map((thread) => {
          const loading = isThreadLoading(threadLoadingByThreadId, thread.id, thread.daemonThreadId);
          const active = thread.id === runtime.activeThreadId || thread.daemonThreadId === runtime.activeThread?.daemonThreadId;
          const working = threadIsWorking(
            thread,
            runs,
            runtime.isStreamingResponse ? runtime.activeThreadId : null,
          );
          const readKey = threadReadKey(thread);
          const unread = threadIsUnread(thread, readKey ? lastReadAtByThread[readKey] ?? null : null, Boolean(active));
          const workers = workerCountForThread(thread, runs);
          return (
            <ThreadSessionRow
              key={thread.daemonThreadId ?? thread.id}
              thread={thread}
              active={Boolean(active)}
              loading={loading}
              working={working}
              unread={unread}
              workers={workers}
              activity={sessionActivityLabel(thread.updatedAt, now)}
              history={threadHistoryLabel(thread)}
              editing={editingThreadId === thread.id}
              onOpen={() => void openListedThread(runtime, thread)}
              onStartRename={() => setEditingThreadId(thread.id)}
              onRename={(title) => {
                updateThreadTitle(thread.id, title);
                setEditingThreadId(null);
              }}
              onCancelRename={() => setEditingThreadId(null)}
              onDelete={() => {
                if (window.confirm(`Delete “${thread.title}”?`)) deleteThread(thread.id);
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

function ThreadSessionRow({
  thread,
  active,
  loading,
  working,
  unread,
  workers,
  activity,
  history,
  editing,
  onOpen,
  onStartRename,
  onRename,
  onCancelRename,
  onDelete,
}: {
  thread: AgentThread;
  active: boolean;
  loading: boolean;
  working: boolean;
  unread: boolean;
  workers: number;
  activity: string;
  history: string;
  editing: boolean;
  onOpen: () => void;
  onStartRename: () => void;
  onRename: (title: string) => void;
  onCancelRename: () => void;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState(thread.title);
  const status = working ? "working" : active ? "active" : unread ? "unread" : "idle";
  const statusLabel = [working ? "working" : "", unread ? "unread" : ""].filter(Boolean).join(", ");

  useEffect(() => {
    if (editing) setDraft(thread.title);
  }, [editing, thread.title]);

  return (
    <div
      className={["zorai-thread-row", active ? "zorai-thread-row--active" : "", loading ? "zorai-thread-row--loading" : ""].filter(Boolean).join(" ")}
      data-status={status}
    >
      {editing ? (
        <input
          className="zorai-thread-row__input"
          aria-label="Rename session"
          value={draft}
          autoFocus
          spellCheck={false}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              const title = draft.trim();
              if (title && title !== thread.title) onRename(title);
              else onCancelRename();
            } else if (event.key === "Escape") {
              event.preventDefault();
              onCancelRename();
            }
          }}
          onBlur={() => {
            const title = draft.trim();
            if (title && title !== thread.title) onRename(title);
            else onCancelRename();
          }}
        />
      ) : (
        <button
          type="button"
          className="zorai-thread-row__main"
          aria-current={active ? "page" : undefined}
          aria-busy={loading}
          aria-label={[thread.title, statusLabel].filter(Boolean).join(", ")}
          title={`${thread.title} — ${history}`}
          onClick={onOpen}
          onDoubleClick={(event) => {
            event.preventDefault();
            onStartRename();
          }}
        >
          <span className={`zorai-thread-dot zorai-thread-dot--${status}`} aria-hidden="true" />
          <span className="zorai-thread-title">{thread.title}</span>
          {workers > 0 ? <span className="zorai-thread-workers" title={`${workers} spawned ${workers === 1 ? "agent" : "agents"}`}>{workers}</span> : null}
          {loading ? <LoadingState size={12} className="zorai-thread-item__spinner" /> : <span className="zorai-thread-activity">{activity}</span>}
        </button>
      )}
      {editing ? null : (
        <span className="zorai-thread-row__actions">
          <button type="button" className="zorai-thread-row__action" aria-label={`Rename ${thread.title}`} onClick={onStartRename}>Rename</button>
          <button type="button" className="zorai-thread-row__action zorai-thread-row__action--danger" aria-label={`Delete ${thread.title}`} onClick={onDelete}>Delete</button>
        </span>
      )}
    </div>
  );
}

function filterThreadsForSearchQuery(threads: AgentThread[], searchQuery: string): AgentThread[] {
  const lower = searchQuery.trim().toLowerCase();
  return lower
    ? threads.filter((thread) => thread.title.toLowerCase().includes(lower) || thread.lastMessagePreview.toLowerCase().includes(lower))
    : threads;
}

function goalThreadIds(goalRuns: ReturnType<typeof useAgentChatPanelRuntime>["goalRunsForTrace"]): Set<string> {
  const ids = new Set<string>();
  for (const goal of goalRuns) {
    for (const id of [goal.thread_id, goal.root_thread_id, goal.active_thread_id, ...(goal.execution_thread_ids ?? [])]) {
      if (id) ids.add(id);
    }
  }
  return ids;
}

function threadHistoryLabel(thread: AgentThread): string {
  if (thread.messageCount > 0) return `${thread.messageCount} msgs`;
  if ((thread.totalInputTokens ?? 0) > 0 || (thread.totalOutputTokens ?? 0) > 0 || (thread.totalTokens ?? 0) > 0) return "history";
  return "0 msgs";
}
