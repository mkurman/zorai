import { LoadingState, ThreadListSkeleton } from "@/components/LoadingState";
import { ThreadNeedsActionIcon, useThreadIdsNeedingAction } from "./threadNeedsAction";
import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { threadDateGroupLabel } from "./threadDateGroups";
import { ThreadRowHoverCard } from "./ThreadRowHoverCard";
import { Fragment, memo, startTransition, useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type RefObject } from "react";
import { CodeContextMenu } from "@/zorai/features/code/CodeToolbarMenu";
import { threadTint, threadTintKey, tintMenuItem, tintStyle, useTintStore } from "@/zorai/features/tints/tintStore";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { ACTIVE_RUN_POLL_MS, fetchAgentRuns, IDLE_RUN_POLL_MS, runListIsActive, type AgentRun } from "@/lib/agentRuns";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { getBridge } from "@/lib/bridge";
import {
  buildThreadFilterTabs,
  daemonAgentFilterForThreadTab,
  dateFilters,
  DEFAULT_THREAD_DATE_FILTER,
  filterThreads,
  mergeLocalDraftThreads,
  overlayStoreThreadTitles,
  resolveThreadCreationAgent,
  resolveThreadListSource,
  searchThreadTabs,
  threadDeletionTarget,
  type DateFilterId,
  type ThreadFilterTab,
  visibleThreadTabs,
} from "./threadFilterModel";
import { openListedThread, openThreadTarget } from "./openThreadTarget";
import {
  searchThreads,
  THREAD_SEARCH_DEBOUNCE_MS,
  threadSearchQueryIsActive,
  type ThreadSearchResponse,
} from "./threadSearchModel";
import { fetchGatewayChats, gatewayLinkedThreadIds } from "./gatewayChatModel";
import { isThreadLoading, useThreadLoadingStore } from "./threadLoadingStore";
import { useThreadRailTabStore } from "./threadRailTabStore";
import { useAgentDisplayName } from "@/lib/useAgentDisplayName";
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

type ThreadsRailRuntime = Pick<
  AgentChatPanelRuntimeValue,
  | "activeThread"
  | "activeThreadId"
  | "activeWorkspace"
  | "createThread"
  | "fetchThreadList"
  | "filteredThreads"
  | "goalRunsForTrace"
  | "isStreamingResponse"
  | "searchQuery"
  | "setChatBackView"
  | "setSearchQuery"
  | "setView"
>;

export function ThreadsRail() {
  const runtime = useAgentChatPanelRuntime();
  const runtimeRef = useRef(runtime);
  runtimeRef.current = runtime;
  return (
    <ThreadsRailBody
      runtimeRef={runtimeRef}
      activeThread={runtime.activeThread}
      activeThreadId={runtime.activeThreadId}
      activeWorkspace={runtime.activeWorkspace}
      createThread={runtime.createThread}
      fetchThreadList={runtime.fetchThreadList}
      filteredThreads={runtime.filteredThreads}
      goalRunsForTrace={runtime.goalRunsForTrace}
      isStreamingResponse={runtime.isStreamingResponse}
      searchQuery={runtime.searchQuery}
      setChatBackView={runtime.setChatBackView}
      setSearchQuery={runtime.setSearchQuery}
      setView={runtime.setView}
    />
  );
}

const ThreadsRailBody = memo(function ThreadsRailBody({
  runtimeRef,
  ...runtime
}: ThreadsRailRuntime & { runtimeRef: RefObject<AgentChatPanelRuntimeValue> }) {
  const subAgents = useAgentStore((state) => state.subAgents);
  const storeThreads = useAgentStore((state) => state.threads);
  const threadLoadingByThreadId = useThreadLoadingStore((state) => state.byThreadId);
  const refreshSubAgents = useAgentStore((state) => state.refreshSubAgents);
  const updateThreadTitle = useAgentStore((state) => state.updateThreadTitle);
  const deleteThread = useAgentStore((state) => state.deleteThread);
  const lastReadAtByThread = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const threadIdsNeedingAction = useThreadIdsNeedingAction();
  const [runs, setRuns] = useState<AgentRun[]>([]);
  const runsRef = useRef(runs);
  runsRef.current = runs;
  const [editingThreadId, setEditingThreadId] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const tab = useThreadRailTabStore((state) => state.tab);
  const setTab = useThreadRailTabStore((state) => state.setTab);
  const favorites = useThreadRailTabStore((state) => state.favorites);
  const toggleFavorite = useThreadRailTabStore((state) => state.toggleFavorite);
  const agentDisplayName = useAgentDisplayName();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const showSearch = searchOpen || Boolean(runtime.searchQuery);
  const [pickerQuery, setPickerQuery] = useState("");
  const [dateFilter, setDateFilter] = useState<DateFilterId>(DEFAULT_THREAD_DATE_FILTER);
  const [fromDate, setFromDate] = useState("");
  const [toDate, setToDate] = useState("");
  const [daemonFilteredThreads, setDaemonFilteredThreads] = useState<AgentThread[] | null>(null);
  const [gatewayThreadIds, setGatewayThreadIds] = useState<Set<string>>(() => new Set());
  const [loadingTab, setLoadingTab] = useState<ThreadFilterTab | null>(tab);
  const pendingFetchIdRef = useRef(0);
  const loadedAgentFilterRef = useRef<string | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const [threadSearch, setThreadSearch] = useState<{ query: string; response: ThreadSearchResponse | null; error: string | null }>({ query: "", response: null, error: null });
  const searchActive = threadSearchQueryIsActive(runtime.searchQuery);
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
    gatewayThreadIds,
  }), [dateFilter, fromDate, gatewayThreadIds, goalThreadIdSet, sourceThreads, subAgents, tab, toDate]);
  const threadTabs = useMemo(() => buildThreadFilterTabs(
    runtime.filteredThreads,
    subAgents,
    goalThreadIdSet,
  ).map((item) => ({ ...item, label: agentDisplayName(item.label) })), [agentDisplayName, goalThreadIdSet, runtime.filteredThreads, subAgents]);
  const visibleTabs = useMemo(
    () => visibleThreadTabs(threadTabs, favorites, tab),
    [favorites, tab, threadTabs],
  );
  const pickerTabs = useMemo(
    () => searchThreadTabs(threadTabs, pickerQuery),
    [pickerQuery, threadTabs],
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
    const query = runtime.searchQuery.trim();
    if (!threadSearchQueryIsActive(query)) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      searchThreads(query)
        .then((response) => {
          if (!cancelled) setThreadSearch({ query, response, error: null });
        })
        .catch((error: unknown) => {
          if (!cancelled) setThreadSearch({ query, response: null, error: error instanceof Error ? error.message : String(error) });
        });
    }, THREAD_SEARCH_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [runtime.searchQuery]);

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
        if (!cancelled && !sameAgentRunSnapshot(runsRef.current, next)) setRuns(next);
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
      setSearchOpen(true);
      window.requestAnimationFrame(() => {
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      });
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

  const refreshGatewayThreadIds = useCallback(() => {
    void fetchGatewayChats()
      .then((chats) => setGatewayThreadIds(gatewayLinkedThreadIds(chats)))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    refreshGatewayThreadIds();
  }, [refreshGatewayThreadIds]);

  useEffect(() => {
    const onRefresh = () => {
      refreshSelectedTab();
      refreshGatewayThreadIds();
    };
    window.addEventListener(ZORAI_THREAD_LIST_REFRESH_EVENT, onRefresh);
    return () => window.removeEventListener(ZORAI_THREAD_LIST_REFRESH_EVENT, onRefresh);
  }, [refreshGatewayThreadIds, refreshSelectedTab]);

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-rail-actions">
        <button
          type="button"
          className="zorai-rail-new-thread"
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
          <ThreadRowIcon kind="new" />
          New thread
        </button>
      </div>
      <div className="zorai-thread-section">
        <button
          type="button"
          className="zorai-thread-section__label"
          aria-expanded={filtersOpen}
          title="Show filters"
          onClick={() => setFiltersOpen((open) => !open)}
        >
          {[threadTabs.find((item) => item.id === tab)?.label ?? "Threads", dateFilters.find((item) => item.id === dateFilter)?.label].filter(Boolean).join(" · ")}
        </button>
        <button
          type="button"
          className={["zorai-icon-button", "zorai-thread-section__icon", showSearch ? "is-active" : ""].filter(Boolean).join(" ")}
          aria-expanded={showSearch}
          aria-label="Search threads"
          title="Search threads (Ctrl+F)"
          onClick={() => {
            if (showSearch) {
              runtime.setSearchQuery("");
              setSearchOpen(false);
            } else {
              setSearchOpen(true);
              window.requestAnimationFrame(() => searchInputRef.current?.focus());
            }
          }}
        >
          <ThreadRowIcon kind="search" />
        </button>
        <button
          type="button"
          className={["zorai-icon-button", "zorai-thread-section__icon", filtersOpen ? "is-active" : ""].filter(Boolean).join(" ")}
          aria-expanded={filtersOpen}
          aria-label="Filters"
          title="Filters"
          onClick={() => setFiltersOpen((open) => !open)}
        >
          <ThreadRowIcon kind="filter" />
        </button>
        <RefreshButton className="zorai-thread-section__icon" onClick={() => { refreshSelectedTab(); refreshGatewayThreadIds(); }} disabled={loadingTab !== null} busy={loadingTab !== null} />
      </div>
      {showSearch ? (
      <div className="zorai-search-field">
        <input ref={searchInputRef} className="zorai-search-input" value={runtime.searchQuery} onChange={(event) => runtime.setSearchQuery(event.target.value)} placeholder="Search threads" onKeyDown={(event) => {
          if (event.key === "Escape") {
            runtime.setSearchQuery("");
            setSearchOpen(false);
          }
        }} />
        {runtime.searchQuery ? (
          <button
            type="button"
            className="zorai-search-field__clear"
            title="Clear search"
            aria-label="Clear search"
            onClick={() => {
              runtime.setSearchQuery("");
              searchInputRef.current?.focus();
            }}
          >
            ×
          </button>
        ) : null}
      </div>
      ) : null}
      {filtersOpen ? (
      <div className="zorai-thread-filters">
      <div className="zorai-thread-filter-tabs" aria-label="Thread source filters">
        {visibleTabs.map((item) => (
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
        <button
          type="button"
          className={["zorai-thread-filter-tab", pickerOpen ? "zorai-thread-filter-tab--active" : ""].filter(Boolean).join(" ")}
          onClick={() => setPickerOpen((open) => !open)}
          aria-expanded={pickerOpen}
          title="Find agents and filters, pin favorites"
        >
          + More
        </button>
      </div>
      {pickerOpen ? (
        <div className="zorai-thread-agent-filter">
          <input
            className="zorai-search-input"
            value={pickerQuery}
            onChange={(event) => setPickerQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") setPickerOpen(false);
              if (event.key === "Enter" && pickerTabs[0]) {
                setTab(pickerTabs[0].id);
                setPickerOpen(false);
              }
            }}
            placeholder="Search agents and filters"
            aria-label="Search agents and filters"
            autoFocus
          />
          <div className="zorai-thread-agent-picker" role="listbox" aria-label="Agents and filters">
            {pickerTabs.length === 0 ? (
              <div className="zorai-empty">No matches.</div>
            ) : pickerTabs.map((item) => {
              const pinned = favorites.includes(item.id);
              return (
                <div key={item.id} className={["zorai-thread-agent-picker__row", tab === item.id ? "zorai-thread-agent-picker__row--active" : ""].filter(Boolean).join(" ")}>
                  <button
                    type="button"
                    className="zorai-thread-agent-picker__select"
                    role="option"
                    aria-selected={tab === item.id}
                    onClick={() => {
                      setTab(item.id);
                      setPickerOpen(false);
                    }}
                  >
                    {item.label}
                  </button>
                  <button
                    type="button"
                    className={["zorai-thread-agent-picker__pin", pinned ? "zorai-thread-agent-picker__pin--on" : ""].filter(Boolean).join(" ")}
                    onClick={() => toggleFavorite(item.id)}
                    aria-pressed={pinned}
                    aria-label={pinned ? `Unpin ${item.label}` : `Pin ${item.label}`}
                    title={pinned ? "Remove from favorites" : "Add to favorites"}
                  >
                    {pinned ? "★" : "☆"}
                  </button>
                </div>
              );
            })}
          </div>
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
      ) : null}
      {searchActive ? (
        <ThreadSearchResults
          query={runtime.searchQuery.trim()}
          search={threadSearch}
          now={now}
          activeDaemonThreadId={runtime.activeThread?.daemonThreadId ?? null}
          onOpen={(threadId) => void openThreadTarget(runtimeRef.current, threadId)}
        />
      ) : (
      <div className="zorai-thread-list" aria-busy={loadingTab !== null}>
        {loadingTab && daemonFilteredThreads === null ? (
          <ThreadListSkeleton />
        ) : listedThreads.length === 0 ? (
          <div className="zorai-empty">No threads match this search.</div>
        ) : listedThreads.map((thread, index) => {
          const group = threadDateGroupLabel(thread.updatedAt, now);
          const groupStarts = index === 0 || threadDateGroupLabel(listedThreads[index - 1].updatedAt, now) !== group;
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
          const needsAction = Boolean(thread.daemonThreadId && threadIdsNeedingAction.has(thread.daemonThreadId));
          return (
            <Fragment key={thread.daemonThreadId ?? thread.id}>
            {groupStarts ? <div className="zorai-thread-group-label">{group}</div> : null}
            <ThreadSessionRow
              thread={thread}
              active={Boolean(active)}
              loading={loading}
              working={working}
              needsAction={needsAction}
              unread={unread}
              workers={workers}
              activity={sessionActivityLabel(thread.updatedAt, now)}
              editing={editingThreadId === thread.id}
              onOpen={() => void openListedThread(runtimeRef.current, thread)}
              onStartRename={() => setEditingThreadId(thread.id)}
              onRename={(title) => {
                updateThreadTitle(thread.id, title);
                if (thread.daemonThreadId && title.trim()) {
                  void getBridge()?.agentRenameThread?.(thread.daemonThreadId, title.trim());
                }
                setEditingThreadId(null);
              }}
              onCancelRename={() => setEditingThreadId(null)}
              onDelete={() => {
                if (!window.confirm(`Delete “${thread.title}”?`)) return;
                const target = threadDeletionTarget(thread, storeThreads);
                if (target.storeThreadId) deleteThread(target.storeThreadId);
                if (target.daemonThreadId) void getBridge()?.agentDeleteThread?.(target.daemonThreadId);
                setDaemonFilteredThreads((current) => current?.filter((item) => item.id !== thread.id) ?? null);
              }}
            />
            </Fragment>
          );
        })}
      </div>
      )}
    </div>
  );
});

function ThreadSearchResults({
  query,
  search,
  now,
  activeDaemonThreadId,
  onOpen,
}: {
  query: string;
  search: { query: string; response: ThreadSearchResponse | null; error: string | null };
  now: number;
  activeDaemonThreadId: string | null;
  onOpen: (threadId: string) => void;
}) {
  const settled = search.query === query;
  const response = settled ? search.response : null;
  return (
    <div className="zorai-thread-list zorai-thread-search" aria-busy={!settled}>
      {settled && search.error ? (
        <div className="zorai-empty">Search failed: {search.error}</div>
      ) : !response ? (
        <ThreadListSkeleton />
      ) : (
        <>
          <div className="zorai-thread-search__mode">
            {response.semantic ? "Semantic + keyword search" : "Keyword search"}
            {response.semantic_error ? ` · semantic unavailable: ${response.semantic_error}` : ""}
          </div>
          {response.results.length === 0 ? (
            <div className="zorai-empty">No threads match this search.</div>
          ) : response.results.map((hit) => {
            const active = hit.thread_id === activeDaemonThreadId;
            return (
              <div key={hit.thread_id} className={["zorai-thread-row", "zorai-thread-search__hit", active ? "zorai-thread-row--active" : ""].filter(Boolean).join(" ")}>
                <button
                  type="button"
                  className="zorai-thread-row__main"
                  aria-current={active ? "page" : undefined}
                  title={`${hit.title} — matched ${hit.matched.join(", ")}`}
                  onClick={() => onOpen(hit.thread_id)}
                >
                  <span className="zorai-thread-title">{hit.title}</span>
                  <span className="zorai-thread-activity">{sessionActivityLabel(hit.updated_at, now)}</span>
                  {hit.snippet ? <span className="zorai-thread-search__snippet">{hit.snippet}</span> : null}
                </button>
              </div>
            );
          })}
        </>
      )}
    </div>
  );
}

function ThreadSessionRow({
  thread,
  active,
  loading,
  working,
  needsAction,
  unread,
  workers,
  activity,
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
  needsAction: boolean;
  unread: boolean;
  workers: number;
  activity: string;
  editing: boolean;
  onOpen: () => void;
  onStartRename: () => void;
  onRename: (title: string) => void;
  onCancelRename: () => void;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState(thread.title);
  const [hoverAnchor, setHoverAnchor] = useState<DOMRect | null>(null);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const tintKey = threadTintKey(thread);
  const tint = useTintStore((state) => threadTint(state.threads, thread));
  const setThreadTint = useTintStore((state) => state.setThreadTint);
  const closeMenu = useCallback(() => setMenuAt(null), []);
  const hoverTimerRef = useRef<number | null>(null);
  const status = needsAction ? "needs-action" : working ? "working" : active ? "active" : unread ? "unread" : "idle";
  const statusLabel = [needsAction ? "needs your action" : "", working ? "working" : "", unread ? "unread" : ""].filter(Boolean).join(", ");

  useEffect(() => {
    if (editing) setDraft(thread.title);
  }, [editing, thread.title]);

  const hideHoverCard = () => {
    if (hoverTimerRef.current != null) window.clearTimeout(hoverTimerRef.current);
    hoverTimerRef.current = null;
    setHoverAnchor(null);
  };

  useEffect(() => hideHoverCard, []);

  return (
    <div
      className={["zorai-thread-row", active ? "zorai-thread-row--active" : "", loading ? "zorai-thread-row--loading" : "", tint ? "zorai-thread-row--tinted" : ""].filter(Boolean).join(" ")}
      style={tintStyle(tint) as CSSProperties | undefined}
      data-status={status}
      onContextMenu={(event) => {
        if (editing) return;
        event.preventDefault();
        hideHoverCard();
        setMenuAt({ x: event.clientX, y: event.clientY });
      }}
      onMouseEnter={(event) => {
        if (editing) return;
        const row = event.currentTarget;
        if (hoverTimerRef.current != null) window.clearTimeout(hoverTimerRef.current);
        hoverTimerRef.current = window.setTimeout(() => setHoverAnchor(row.getBoundingClientRect()), 550);
      }}
      onMouseLeave={hideHoverCard}
      onMouseDown={hideHoverCard}
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
          onClick={onOpen}
          onDoubleClick={(event) => {
            event.preventDefault();
            onStartRename();
          }}
        >
          {status === "needs-action" ? <span className="zorai-thread-needs-action-slot" title="Waiting for your approval or answer"><ThreadNeedsActionIcon /></span> : null}
          {status === "working" ? <span className="zorai-thread-dot zorai-thread-dot--working" aria-hidden="true" /> : null}
          <span className="zorai-thread-title">{thread.title}</span>
          {loading ? <LoadingState size={12} className="zorai-thread-item__spinner" /> : null}
        </button>
      )}
      {editing ? null : (
        <span className="zorai-thread-row__actions">
          <button type="button" className="zorai-thread-row__action" aria-label={`Rename ${thread.title}`} title="Rename" onClick={onStartRename}><ThreadRowIcon kind="rename" /></button>
          <button type="button" className="zorai-thread-row__action zorai-thread-row__action--danger" aria-label={`Delete ${thread.title}`} title="Delete" onClick={onDelete}><ThreadRowIcon kind="delete" /></button>
        </span>
      )}
      {menuAt ? (
        <CodeContextMenu
          x={menuAt.x}
          y={menuAt.y}
          label={`Actions for ${thread.title}`}
          onClose={closeMenu}
          items={[
            tintMenuItem(tint, (next) => setThreadTint(tintKey, next)),
            { key: "rename", label: "Rename", separatorBefore: true, onSelect: onStartRename },
            { key: "delete", label: "Delete…", danger: true, onSelect: onDelete },
          ]}
        />
      ) : null}
      {hoverAnchor && !editing ? <ThreadRowHoverCard anchor={hoverAnchor} thread={thread} activity={activity} workers={workers} working={working} /> : null}
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

function ThreadRowIcon({ kind }: { kind: "rename" | "delete" | "new" | "filter" | "search" }) {
  if (kind === "new" || kind === "filter" || kind === "search") {
    return (
      <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        {kind === "new" ? (
          <>
            <circle cx="12" cy="12" r="9" />
            <path d="M12 8v8M8 12h8" />
          </>
        ) : kind === "search" ? (
          <>
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </>
        ) : (
          <path d="M4 6h16M7 12h10M10 18h4" />
        )}
      </svg>
    );
  }
  return (
    <svg width={12} height={12} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {kind === "rename" ? (
        <>
          <path d="M12 20h9" />
          <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
        </>
      ) : (
        <>
          <polyline points="3 6 5 6 21 6" />
          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
          <path d="M10 11v6M14 11v6M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2" />
        </>
      )}
    </svg>
  );
}
