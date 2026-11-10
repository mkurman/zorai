import { useEffect, useState, type DragEvent } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { openThreadTarget } from "./openThreadTarget";
import { neighborSessionId, sessionCloseFollowUp, useOpenSessionTabs } from "./openSessionTabs";
import { threadIsUnread } from "./sessionCooperation";
import { threadReadKey, useThreadReadStateStore } from "./threadReadStateStore";

export type SessionTabModel = {
  id: string;
  title: string;
  active: boolean;
  working: boolean;
  unread: boolean;
  pinned?: boolean;
};

const SESSION_TAB_DRAG_TYPE = "application/x-zorai-session-tab";

type SessionTabDrop = { id: string; after: boolean };

export function SessionTabStrip({
  tabs,
  onSelect,
  onClose,
  onMove,
  onTogglePin,
  label = "Open sessions",
}: {
  tabs: SessionTabModel[];
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onMove?: (id: string, targetId: string, placeAfter: boolean) => void;
  onTogglePin?: (id: string) => void;
  label?: string;
}) {
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [drop, setDrop] = useState<SessionTabDrop | null>(null);
  if (tabs.length === 0) return null;

  const endDrag = () => {
    setDraggingId(null);
    setDrop(null);
  };

  const dropTarget = (event: DragEvent<HTMLDivElement>, id: string): SessionTabDrop => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { id, after: event.clientX > rect.left + rect.width / 2 };
  };

  return (
    <nav className="zorai-session-tabs" aria-label={label}>
      {tabs.map((tab) => {
        const classes = ["zorai-session-tab"];
        if (tab.active) classes.push("zorai-session-tab--active");
        if (tab.pinned) classes.push("is-pinned");
        if (tab.id === draggingId) classes.push("is-dragging");
        if (drop?.id === tab.id && draggingId && draggingId !== tab.id) classes.push(drop.after ? "is-drop-after" : "is-drop-before");
        return (
          <div
            key={tab.id}
            className={classes.join(" ")}
            title={tab.title}
            draggable={Boolean(onMove)}
            onDragStart={(event) => {
              event.dataTransfer.setData(SESSION_TAB_DRAG_TYPE, tab.id);
              event.dataTransfer.effectAllowed = "move";
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
            onAuxClick={(event) => {
              if (event.button === 1 && !tab.pinned) onClose(tab.id);
            }}
          >
            <button type="button" className="zorai-session-tab__main" aria-current={tab.active ? "page" : undefined} onClick={() => onSelect(tab.id)}>
              {tab.working ? <span className="zorai-thread-dot zorai-thread-dot--working" aria-hidden="true" /> : null}
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

export function ThreadSessionTabs({ includeSession }: { includeSession?: (id: string, thread: AgentThread | undefined) => boolean } = {}) {
  const runtime = useAgentChatPanelRuntime();
  const threads = useAgentStore((state) => state.threads);
  const openSessionIds = useOpenSessionTabs((state) => state.ids);
  const rememberOpenSession = useOpenSessionTabs((state) => state.remember);
  const closeOpenSession = useOpenSessionTabs((state) => state.close);
  const moveOpenSession = useOpenSessionTabs((state) => state.move);
  const lastReadAtByThread = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const activeId = runtime.activeThread?.id ?? null;

  useEffect(() => {
    if (activeId) rememberOpenSession(activeId);
  }, [activeId, rememberOpenSession]);

  const visibleIds = includeSession
    ? openSessionIds.filter((id) => includeSession(id, threads.find((item) => item.id === id || item.daemonThreadId === id)))
    : openSessionIds;
  const tabs = visibleIds.map((id) => {
    const thread = threads.find((item) => item.id === id || item.daemonThreadId === id);
    const active = thread?.id === activeId || id === activeId;
    const readKey = thread ? threadReadKey(thread) : null;
    return sessionTabModel(
      id,
      threads,
      activeId,
      Boolean(active && runtime.isStreamingResponse),
      thread ? threadIsUnread(thread, readKey ? lastReadAtByThread[readKey] ?? null : null, Boolean(active)) : false,
    );
  });

  return (
    <SessionTabStrip
      tabs={tabs}
      onSelect={(id) => void openThreadTarget(runtime, id)}
      onMove={moveOpenSession}
      onClose={(id) => {
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
        if (action.kind === "clear") runtime.setActiveThread(null);
      }}
    />
  );
}
