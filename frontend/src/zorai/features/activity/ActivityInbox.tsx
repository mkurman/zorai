import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { useApprovalInlineHosts } from "@/components/AgentApprovalOverlay";
import { useNotificationStore } from "@/lib/notificationStore";
import { useOperatorQuestionStore } from "@/lib/operatorQuestionStore";
import { filterInboxItems, formatWhen, inboxSectionCounts, type InboxFilter, type InboxItem, type InboxSection } from "./inboxModel";
import { useInboxStore, useInboxViewStore } from "./inboxStore";
import { InboxDetail } from "./InboxDetail";

const FILTERS: ReadonlyArray<{ id: InboxFilter; label: string }> = [
  { id: "all", label: "All" },
  { id: "needs", label: "Needs you" },
  { id: "replies", label: "Replies" },
  { id: "updates", label: "Updates" },
];

const SECTION_LABELS: Record<InboxSection, string> = {
  needs: "Needs you",
  replies: "Replies",
  updates: "Updates",
};

const KIND_MARKS: Record<InboxItem["kind"], string> = {
  approval: "▲",
  question: "?",
  goal: "◆",
  reply: "●",
  notification: "!",
  updates: "≡",
};

export function ActivityInbox({ items, onOpenThread, onOpenGoal }: { items: InboxItem[]; onOpenThread: (threadId: string) => void; onOpenGoal: (goalRunId: string) => void }) {
  const filter = useInboxViewStore((state) => state.filter);
  const setFilter = useInboxViewStore((state) => state.setFilter);
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const markNotificationRead = useNotificationStore((state) => state.markRead);
  const archiveNotification = useNotificationStore((state) => state.archiveNotification);
  const markThreadRead = useInboxStore((state) => state.markThreadRead);
  const archiveThread = useInboxStore((state) => state.archiveThread);

  const counts = useMemo(() => inboxSectionCounts(items), [items]);
  const visible = useMemo(
    () => filterInboxItems(items, filter, false, query).filter((item) => !unreadOnly || item.unread || item.id === selectedId),
    [filter, items, query, selectedId, unreadOnly],
  );
  const selected = visible.find((item) => item.id === selectedId) ?? null;

  useEffect(() => {
    if (selectedId && !visible.some((item) => item.id === selectedId)) setSelectedId(null);
  }, [selectedId, visible]);

  const registerInlineHost = useOperatorQuestionStore((state) => state.registerInlineHost);
  useEffect(() => registerInlineHost(), [registerInlineHost]);
  const registerApprovalHost = useApprovalInlineHosts((state) => state.register);
  useEffect(() => registerApprovalHost(), [registerApprovalHost]);

  const markVisibleRead = () => {
    for (const item of visible) {
      if (!item.unread) continue;
      if (item.kind === "reply") markThreadRead(item.threadKey, item.at);
      if (item.kind === "notification") markNotificationRead(item.notification.id);
      if (item.kind === "updates") item.notifications.forEach((notification) => markNotificationRead(notification.id));
    }
  };

  const archiveVisibleRead = () => {
    for (const item of visible) {
      if (item.unread) continue;
      if (item.kind === "reply") archiveThread(item.threadKey, item.at);
      if (item.kind === "notification") archiveNotification(item.notification.id);
      if (item.kind === "updates") item.notifications.forEach((notification) => archiveNotification(notification.id));
    }
  };

  const onListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "j" && event.key !== "k") return;
    if (visible.length === 0) return;
    event.preventDefault();
    const index = visible.findIndex((item) => item.id === selectedId);
    const step = event.key === "ArrowDown" || event.key === "j" ? 1 : -1;
    const next = visible[Math.min(visible.length - 1, Math.max(0, index < 0 ? 0 : index + step))];
    setSelectedId(next.id);
    document.getElementById(inboxRowDomId(next.id))?.scrollIntoView({ block: "nearest" });
  };

  let lastSection: InboxSection | null = null;

  return (
    <div className="zorai-inbox">
      <div className="zorai-inbox__toolbar">
        <div className="zorai-inbox__filters" role="tablist" aria-label="Inbox filters">
          {FILTERS.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="tab"
              aria-selected={filter === entry.id}
              className={["zorai-inbox__filter", filter === entry.id ? "zorai-inbox__filter--active" : ""].filter(Boolean).join(" ")}
              onClick={() => setFilter(entry.id)}
            >
              {entry.label}
              {counts[entry.id] > 0 ? <span className="zorai-inbox__count">{counts[entry.id]}</span> : null}
            </button>
          ))}
          <label className="zorai-inbox__unread-toggle">
            <input type="checkbox" checked={unreadOnly} onChange={(event) => setUnreadOnly(event.target.checked)} />
            Unread only
          </label>
        </div>
        <input
          className="zorai-input zorai-inbox__search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search inbox…"
          aria-label="Search inbox"
        />
        <div className="zorai-inbox__bulk">
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={markVisibleRead}>Mark read</button>
          <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={archiveVisibleRead}>Archive read</button>
        </div>
      </div>

      <div className="zorai-inbox__split">
        <div className="zorai-inbox__list" role="listbox" aria-label="Inbox" tabIndex={0} onKeyDown={onListKeyDown}>
          {visible.length === 0 ? (
            <div className="zorai-empty-state">{items.length === 0 ? "You're all caught up." : "Nothing matches these filters."}</div>
          ) : visible.map((item) => {
            const header = item.section !== lastSection ? SECTION_LABELS[item.section] : null;
            lastSection = item.section;
            return (
              <div key={item.id} className="zorai-inbox__group">
                {header ? <div className="zorai-inbox__section">{header}</div> : null}
                <button
                  id={inboxRowDomId(item.id)}
                  type="button"
                  role="option"
                  aria-selected={item.id === selectedId}
                  className={[
                    "zorai-inbox__row",
                    `zorai-inbox__row--${item.kind}`,
                    item.unread ? "zorai-inbox__row--unread" : "",
                    item.id === selectedId ? "zorai-inbox__row--selected" : "",
                  ].filter(Boolean).join(" ")}
                  onClick={() => setSelectedId(item.id)}
                >
                  <span className="zorai-inbox__mark" aria-hidden="true">{KIND_MARKS[item.kind]}</span>
                  <span className="zorai-inbox__text">
                    <span className="zorai-inbox__title">{item.title}</span>
                    <span className="zorai-inbox__summary">{item.summary}</span>
                  </span>
                  <span className="zorai-inbox__time">{formatWhen(item.at)}</span>
                </button>
              </div>
            );
          })}
        </div>
        <div className="zorai-inbox__reader">
          {selected ? (
            <InboxDetail key={selected.id} item={selected} onOpenThread={onOpenThread} onOpenGoal={onOpenGoal} />
          ) : (
            <div className="zorai-empty-state">Select an item to read it, reply, or decide.</div>
          )}
        </div>
      </div>
    </div>
  );
}

function inboxRowDomId(id: string): string {
  return `zorai-inbox-row-${id.replace(/[^a-zA-Z0-9_-]/g, "_")}`;
}
