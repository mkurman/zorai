import { useEffect } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import { openThreadTarget } from "./openThreadTarget";
import { useOpenSessionTabs } from "./openSessionTabs";
import { threadIsUnread } from "./sessionCooperation";
import { threadReadKey, useThreadReadStateStore } from "./threadReadStateStore";

export type SessionTabModel = {
  id: string;
  title: string;
  active: boolean;
  working: boolean;
  unread: boolean;
};

export function SessionTabStrip({
  tabs,
  onSelect,
  onClose,
}: {
  tabs: SessionTabModel[];
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
}) {
  if (tabs.length === 0) return null;
  return (
    <nav className="zorai-session-tabs" aria-label="Open sessions">
      {tabs.map((tab) => (
        <div key={tab.id} className={tab.active ? "zorai-session-tab zorai-session-tab--active" : "zorai-session-tab"}>
          <button type="button" className="zorai-session-tab__main" aria-current={tab.active ? "page" : undefined} onClick={() => onSelect(tab.id)}>
            <span className={`zorai-thread-dot zorai-thread-dot--${tab.working ? "working" : tab.unread ? "unread" : tab.active ? "active" : "idle"}`} aria-hidden="true" />
            <span>{tab.title}</span>
          </button>
          <button type="button" className="zorai-session-tab__close" aria-label={`Close ${tab.title}`} onClick={() => onClose(tab.id)}>
            ×
          </button>
        </div>
      ))}
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

export function ThreadSessionTabs() {
  const runtime = useAgentChatPanelRuntime();
  const threads = useAgentStore((state) => state.threads);
  const openSessionIds = useOpenSessionTabs((state) => state.ids);
  const rememberOpenSession = useOpenSessionTabs((state) => state.remember);
  const closeOpenSession = useOpenSessionTabs((state) => state.close);
  const lastReadAtByThread = useThreadReadStateStore((state) => state.lastReadAtByThread);
  const activeId = runtime.activeThread?.id ?? null;

  useEffect(() => {
    if (activeId) rememberOpenSession(activeId);
  }, [activeId, rememberOpenSession]);

  const tabs = openSessionIds.map((id) => {
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
      onClose={(id) => {
        const next = closeOpenSession(id);
        const active = runtime.activeThread?.id === id || runtime.activeThread?.daemonThreadId === id;
        if (active && next) void openThreadTarget(runtime, next);
      }}
    />
  );
}
