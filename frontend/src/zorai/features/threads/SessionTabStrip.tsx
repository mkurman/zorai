import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from "react";
import { CodeContextMenu, type CodeToolbarMenuItem } from "@/zorai/features/code/CodeToolbarMenu";
import { threadTint, tintColor, useTintStore } from "@/zorai/features/tints/tintStore";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { openThreadTarget, splitCandidateFor } from "./openThreadTarget";
import { tabStripMenuItems } from "./threadContextMenus";
import { neighborSessionId, sessionCloseFollowUp, useOpenSessionTabs } from "./openSessionTabs";
import { threadIsUnread } from "./sessionCooperation";
import { threadReadKey, useThreadReadStateStore } from "./threadReadStateStore";
import { ThreadNeedsActionIcon, useThreadIdsNeedingAction } from "./threadNeedsAction";
import { useThreadRailTabStore } from "./threadRailTabStore";
import { resolveThreadCreationAgent } from "./threadFilterModel";
import { MAX_SPLIT_PANES, splitLayoutsFor, splitPaneIds, useThreadSplitStore, type ThreadSplitDropZone, type ThreadSplitLayout } from "./threadSplitModel";
import { consumePendingThreadCommand, ZORAI_THREAD_COMMAND_EVENT, type ZoraiThreadCommand } from "@/zorai/shell/zoraiAppCommands";

export type SessionTabModel = {
  id: string;
  title: string;
  active: boolean;
  inSplit?: boolean;
  working: boolean;
  unread: boolean;
  needsAction?: boolean;
  pinned?: boolean;
  tint?: string | null;
};

export const SESSION_TAB_DRAG_TYPE = "application/x-zorai-session-tab";

type SessionTabDrop = { id: string; after: boolean };

export function SessionTabStrip({
  tabs,
  onSelect,
  onClose,
  onMove,
  onTogglePin,
  onNew,
  selectedIds,
  onToggleSelect,
  onTabContextMenu,
  onStripContextMenu,
  label = "Open sessions",
}: {
  tabs: SessionTabModel[];
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onMove?: (id: string, targetId: string, placeAfter: boolean) => void;
  onTogglePin?: (id: string) => void;
  onNew?: () => void;
  selectedIds?: ReadonlySet<string>;
  onToggleSelect?: (id: string) => void;
  onTabContextMenu?: (id: string, x: number, y: number) => void;
  onStripContextMenu?: (x: number, y: number) => void;
  label?: string;
}) {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [drop, setDrop] = useState<SessionTabDrop | null>(null);
  if (tabs.length === 0 && !onNew) return null;

  const endDrag = () => {
    setDraggingId(null);
    setDrop(null);
  };

  const dropTarget = (event: DragEvent<HTMLDivElement>, id: string): SessionTabDrop => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { id, after: event.clientX > rect.left + rect.width / 2 };
  };

  return (
    <nav
      className="zorai-session-tabs"
      aria-label={label}
      onContextMenu={onStripContextMenu ? (event) => {
        if ((event.target as Element).closest(".zorai-session-tab")) return;
        event.preventDefault();
        event.stopPropagation();
        onStripContextMenu(event.clientX, event.clientY);
      } : undefined}
    >
      {tabs.map((tab) => {
        const classes = ["zorai-session-tab"];
        if (tab.active) classes.push("zorai-session-tab--active");
        if (tab.inSplit && !tab.active) classes.push("zorai-session-tab--split");
        if (tab.pinned) classes.push("is-pinned");
        if (selectedIds?.has(tab.id)) classes.push("is-selected");
        if (tab.tint) classes.push("zorai-session-tab--tinted");
        if (tab.id === draggingId) classes.push("is-dragging");
        if (drop?.id === tab.id && draggingId && draggingId !== tab.id) classes.push(drop.after ? "is-drop-after" : "is-drop-before");
        return (
          <div
            key={tab.id}
            className={classes.join(" ")}
            style={tab.tint ? { "--zorai-tint": tab.tint } as CSSProperties : undefined}
            title={tab.title}
            draggable={Boolean(onMove)}
            onDragStart={(event) => {
              event.dataTransfer.setData(SESSION_TAB_DRAG_TYPE, tab.id);
              event.dataTransfer.effectAllowed = "copyMove";
              setDraggingId(tab.id);
            }}
            onDragOver={(event) => {
              if (!draggingId || !event.dataTransfer.types.includes(SESSION_TAB_DRAG_TYPE)) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              const next = dropTarget(event, tab.id);
              if (next.id !== drop?.id || next.after !== drop?.after) setDrop(next);
            }}
            onDrop={(event) => {
              const id = event.dataTransfer.getData(SESSION_TAB_DRAG_TYPE);
              if (!id) return;
              event.preventDefault();
              const target = dropTarget(event, tab.id);
              onMove?.(id, target.id, target.after);
              endDrag();
            }}
            onDragEnd={endDrag}
            onContextMenu={onTabContextMenu ? (event) => {
              event.preventDefault();
              onTabContextMenu(tab.id, event.clientX, event.clientY);
            } : undefined}
            onAuxClick={(event) => {
              if (event.button === 1 && !tab.pinned) onClose(tab.id);
            }}
          >
            <button type="button" className="zorai-session-tab__main" aria-current={tab.active ? "page" : undefined} onClick={(event) => {
              if ((event.ctrlKey || event.metaKey) && onToggleSelect) onToggleSelect(tab.id);
              else onSelect(tab.id);
            }}>
              {tab.needsAction ? <span className="zorai-thread-needs-action-slot" title="Waiting for your approval or answer"><ThreadNeedsActionIcon /></span> : tab.working ? <span className="zorai-thread-dot zorai-thread-dot--working" aria-hidden="true" /> : null}
              <span>{tab.title}</span>
            </button>
            {onTogglePin ? (
              <button
                type="button"
                className="zorai-session-tab__pin"
                aria-label={tab.pinned ? `Unpin ${tab.title}` : `Pin ${tab.title}`}
                aria-pressed={Boolean(tab.pinned)}
                title={tab.pinned ? "Unpin" : "Pin"}
                onClick={() => onTogglePin(tab.id)}
              >
                <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                  <path fill="currentColor" d="M9.7 1.4H6.3l-.7 1.2.9.6v3.2L4.3 8.7v1.1h3.1V15h1.2V9.8h3.1V8.7L9.5 6.4V3.2l.9-.6-.7-1.2z" />
                </svg>
              </button>
            ) : null}
            {tab.pinned ? null : (
              <button type="button" className="zorai-session-tab__close" aria-label={`Close ${tab.title}`} onClick={() => onClose(tab.id)}>
                ×
              </button>
            )}
          </div>
        );
      })}
      {onNew ? (
        <button type="button" className="zorai-session-tabs__new" title="New thread" aria-label="New thread" onClick={onNew}>
          <span aria-hidden="true">+</span> New
        </button>
      ) : null}
    </nav>
  );
}

export function sessionTabModel(
  id: string,
  threads: readonly AgentThread[],
  activeId: string | null,
  working: boolean,
  unread: boolean,
): SessionTabModel {
  const thread = threads.find((item) => item.id === id || item.daemonThreadId === id);
  return {
    id,
    title: thread?.title?.trim() || "Session",
    active: id === activeId || thread?.id === activeId || thread?.daemonThreadId === activeId,
    working: working && (id === activeId || thread?.id === activeId),
    unread,
  };
}

export function ThreadSessionTabs({ includeSession, onLastSessionClosed, onNewSession }: {
  includeSession?: (id: string, thread: AgentThread | undefined) => boolean;
  onLastSessionClosed?: () => void;
  onNewSession?: () => void;
} = {}) {
  const railTab = useThreadRailTabStore((state) => state.tab);
  const subAgents = useAgentStore((state) => state.subAgents);
  const runtime = useAgentChatPanelRuntime();
  const threads = useAgentStore((state) => state.threads);
  const openSessionIds = useOpenSessionTabs((state) => state.ids);
  const rememberOpenSession = useOpenSessionTabs((state) => state.remember);
  const closeOpenSession = useOpenSessionTabs((state) => state.close);
  const moveOpenSession = useOpenSessionTabs((state) => state.move);
  const lastReadAtByThread = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const threadIdsNeedingAction = useThreadIdsNeedingAction();
  const threadTints = useTintStore((state) => state.threads);
  const activeId = runtime.activeThread?.id ?? null;
  const activeDaemonId = runtime.activeThread?.daemonThreadId ?? null;
  const activeAlreadyOpen = openSessionIds.some((id) => id === activeId || (activeDaemonId !== null && id === activeDaemonId));
  const splitRoot = useThreadSplitStore((state) => state.root);
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<string>>(() => new Set());
  const [menu, setMenu] = useState<{ id: string | null; x: number; y: number } | null>(null);

  useEffect(() => {
    if (activeId && !activeAlreadyOpen) rememberOpenSession(activeId);
  }, [activeAlreadyOpen, activeId, rememberOpenSession]);

  const visibleIds = includeSession
    ? openSessionIds.filter((id) => includeSession(id, threads.find((item) => item.id === id || item.daemonThreadId === id)))
    : openSessionIds;
  const splitPaneIdList = splitPaneIds(splitRoot);
  const splitPaneIdSet = new Set(splitPaneIdList);
  const hasSplit = splitPaneIdList.length >= 2;
  const tabs = visibleIds.map((id) => {
    const thread = threads.find((item) => item.id === id || item.daemonThreadId === id);
    const active = thread?.id === activeId || id === activeId;
    const inSplit = hasSplit && (splitPaneIdSet.has(id) || Boolean(thread && splitPaneIdSet.has(thread.id)));
    const readKey = thread ? threadReadKey(thread) : null;
    return {
      ...sessionTabModel(
        id,
        threads,
        activeId,
        Boolean(active && runtime.isStreamingResponse),
        thread ? threadIsUnread(thread, readKey ? lastReadAtByThread[readKey] ?? null : null, Boolean(active)) : false,
      ),
      inSplit,
      needsAction: threadIdsNeedingAction.has(thread?.daemonThreadId ?? id),
      tint: tintColor(thread ? threadTint(threadTints, thread) : threadTints[id]),
    };
  });

  const localThreadId = (id: string) => threads.find((item) => item.id === id || item.daemonThreadId === id)?.id ?? null;
  const showSplit = (tabIds: string[], focusTabId: string, layout: ThreadSplitLayout) => {
    const localIds = tabIds.map(localThreadId).filter((id): id is string => Boolean(id));
    const focusId = localThreadId(focusTabId) ?? localIds[0];
    useThreadSplitStore.getState().setLayout(localIds, layout);
    localIds.filter((id) => id !== focusId).forEach((id) => runtime.openThread(id, { reuseLoaded: true }));
    if (focusId) runtime.openThread(focusId, { reuseLoaded: true });
    setSelectedIds(new Set());
  };
  const menuItems = (clickedId: string): CodeToolbarMenuItem[] => {
    const picked = new Set(selectedIds).add(clickedId);
    const activeTab = tabs.find((tab) => tab.active);
    if (picked.size < 2 && activeTab) picked.add(activeTab.id);
    const pickedIds = visibleIds.filter((id) => picked.has(id));
    const items: CodeToolbarMenuItem[] = pickedIds.length > MAX_SPLIT_PANES
      ? [{ key: "too-many", label: `Pick up to ${MAX_SPLIT_PANES} threads (${pickedIds.length} picked)`, disabled: true }]
      : pickedIds.length >= 2
        ? splitLayoutsFor(pickedIds.length).map((option) => ({ key: option.layout, label: option.label, onSelect: () => showSplit(pickedIds, clickedId, option.layout) }))
        : [{ key: "hint", label: "Ctrl+click tabs to pick threads to show together", disabled: true }];
    if (selectedIds.size > 0) items.push({ key: "clear-selection", label: "Clear selection", separatorBefore: true, onSelect: () => setSelectedIds(new Set()) });
    if (splitRoot) items.push({ key: "close-split", label: "Close split view", separatorBefore: selectedIds.size === 0, onSelect: () => useThreadSplitStore.getState().clear() });
    return items;
  };

  const closeTab = (id: string) => {
    const closedLocalId = localThreadId(id);
    const paneIds = splitPaneIds(useThreadSplitStore.getState().root);
    if (closedLocalId && paneIds.includes(closedLocalId)) {
      useThreadSplitStore.getState().removePane(closedLocalId);
      const remaining = paneIds.find((paneId) => paneId !== closedLocalId);
      if (remaining && closedLocalId === runtime.activeThread?.id) {
        closeOpenSession(id);
        runtime.openThread(remaining, { reuseLoaded: true });
        return;
      }
    }
    const visibleNext = neighborSessionId(visibleIds, id);
    const next = closeOpenSession(id);
    const action = sessionCloseFollowUp({
      closedId: id,
      activeThreadId: runtime.activeThread?.id ?? null,
      activeDaemonThreadId: runtime.activeThread?.daemonThreadId ?? null,
      nextOpenId: includeSession ? visibleNext : next,
      openCount: includeSession ? visibleIds.length - 1 : useOpenSessionTabs.getState().ids.length,
    });
    if (action.kind === "follow") void openThreadTarget(runtime, action.id);
    if (action.kind === "clear") {
      if (onLastSessionClosed) onLastSessionClosed();
      else runtime.setActiveThread(null);
    }
  };
  const splitActive = (zone: ThreadSplitDropZone) => {
    const candidate = splitCandidateFor(runtime);
    if (!activeId || !candidate) return;
    useThreadSplitStore.getState().insert(activeId, candidate, zone);
    runtime.openThread(candidate, { reuseLoaded: true });
  };
  const stepTab = (delta: number) => {
    const index = tabs.findIndex((tab) => tab.active);
    if (tabs.length < 2) return;
    const next = tabs[(Math.max(index, 0) + delta + tabs.length) % tabs.length];
    if (next) void openThreadTarget(runtime, next.id, { reuseLoaded: true });
  };
  const runThreadCommand = (command: ZoraiThreadCommand) => {
    const activeTabId = tabs.find((tab) => tab.active)?.id ?? null;
    if (command === "close-thread-tab" && activeTabId) closeTab(activeTabId);
    else if (command === "reopen-closed-thread") {
      const id = useOpenSessionTabs.getState().takeLastClosed();
      if (id) void openThreadTarget(runtime, id);
    } else if (command === "previous-thread") stepTab(-1);
    else if (command === "next-thread") stepTab(1);
    else if (command === "thread-back" && runtime.canGoBackThread) runtime.goBackThread();
    else if (command === "split-right") splitActive("right");
    else if (command === "split-down") splitActive("bottom");
    else if (command === "split-close") useThreadSplitStore.getState().clear();
    else if (command === "split-close-pane" && activeId) {
      const remaining = splitPaneIds(useThreadSplitStore.getState().root).find((id) => id !== activeId);
      useThreadSplitStore.getState().removePane(activeId);
      if (remaining) runtime.openThread(remaining, { reuseLoaded: true });
    }
  };
  const createDefaultThread = () => {
    const agent = resolveThreadCreationAgent(railTab, subAgents);
    const id = runtime.createThread({
      workspaceId: runtime.activeWorkspace?.id ?? null,
      agentId: agent?.id ?? null,
      agentName: agent?.name ?? null,
    });
    runtime.setChatBackView("threads");
    runtime.setView("chat");
    return id;
  };
  const newThread = onNewSession ?? (includeSession ? undefined : () => void createDefaultThread());
  const newThreadInSplit = (zone: ThreadSplitDropZone) => {
    const anchorId = runtime.activeThread?.id;
    if (!anchorId) return;
    const id = createDefaultThread();
    useThreadSplitStore.getState().insert(anchorId, id, zone);
    useThreadSplitStore.getState().setFocused(id);
  };
  const stripMenuItems = () => tabStripMenuItems({
    canReopen: useOpenSessionTabs.getState().closedIds.length > 0,
    canSplit: Boolean(runtime.activeThread) && splitPaneIds(splitRoot).length < MAX_SPLIT_PANES,
    inSplit: Boolean(splitRoot),
    onNew: () => newThread?.(),
    onReopen: () => runThreadCommand("reopen-closed-thread"),
    onSplit: (direction) => newThreadInSplit(direction === "right" ? "right" : "bottom"),
    onCloseSplit: () => useThreadSplitStore.getState().clear(),
  });
  const runThreadCommandRef = useRef(runThreadCommand);
  runThreadCommandRef.current = runThreadCommand;
  useEffect(() => {
    if (includeSession) return;
    const onCommand = () => {
      const command = consumePendingThreadCommand();
      if (command) runThreadCommandRef.current(command);
    };
    onCommand();
    window.addEventListener(ZORAI_THREAD_COMMAND_EVENT, onCommand);
    return () => window.removeEventListener(ZORAI_THREAD_COMMAND_EVENT, onCommand);
  }, [includeSession]);

  return (
    <>
    <SessionTabStrip
      tabs={tabs}
      selectedIds={selectedIds}
      onToggleSelect={(id) => setSelectedIds((current) => {
        const next = new Set(current);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      })}
      onTabContextMenu={(id, x, y) => setMenu({ id, x, y })}
      onSelect={(id) => {
        setSelectedIds(new Set());
        void openThreadTarget(runtime, id, { reuseLoaded: true });
      }}
      onStripContextMenu={newThread ? (x, y) => setMenu({ id: null, x, y }) : undefined}
      onNew={newThread}
      onMove={moveOpenSession}
      onClose={closeTab}
    />
    {menu ? (
      <CodeContextMenu
        x={menu.x}
        y={menu.y}
        label={menu.id ? "Show threads together" : "Thread tabs"}
        onClose={() => setMenu(null)}
        items={menu.id ? menuItems(menu.id) : stripMenuItems()}
      />
    ) : null}
    </>
  );
}
