import { useEffect, useMemo } from "react";
import { useAgentStore } from "@/lib/agentStore";
import { inboxAttentionCount, threadKey } from "./inboxModel";
import { useInboxStore } from "./inboxStore";
import { useInboxItems } from "./useInboxItems";

export function InboxRailBadge() {
  const items = useInboxItems();
  const count = useMemo(() => inboxAttentionCount(items), [items]);
  if (count === 0) return null;
  return <span className="zorai-global-item__badge" aria-label={`${count} waiting in inbox`}>{count > 99 ? "99+" : count}</span>;
}

export function InboxThreadReadTracker({ viewingThreads }: { viewingThreads: boolean }) {
  const activeThread = useAgentStore((state) => state.threads.find((thread) => thread.id === state.activeThreadId) ?? null);
  const markThreadRead = useInboxStore((state) => state.markThreadRead);
  const key = activeThread ? threadKey(activeThread) : null;
  const updatedAt = activeThread?.updatedAt ?? 0;

  useEffect(() => {
    if (viewingThreads && key) markThreadRead(key, Math.max(updatedAt, Date.now()));
  }, [key, markThreadRead, updatedAt, viewingThreads]);

  return null;
}
