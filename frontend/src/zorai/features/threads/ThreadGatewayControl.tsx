import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { notifyThreadListRefresh } from "../../shell/zoraiNavigationEvents";
import {
  fetchGatewayChats,
  gatewayChatLabel,
  gatewayChatLinkState,
  linkThreadToGatewayChat,
  linkedGatewayChat,
  type GatewayChat,
} from "./gatewayChatModel";
import { ThreadSidePanel, useThreadPanelStore } from "./threadSidePanel";

export function ThreadGatewayControl({ daemonThreadId, icon }: { daemonThreadId: string | null | undefined; icon?: ReactNode }) {
  const open = useThreadPanelStore((state) => state.open === "gateway");
  const togglePanel = useThreadPanelStore((state) => state.toggle);
  const closePanel = useThreadPanelStore((state) => state.close);
  const [chats, setChats] = useState<GatewayChat[]>([]);
  const [loading, setLoading] = useState(false);
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const loadChats = useCallback(async () => {
    setLoading(true);
    try {
      const next = await fetchGatewayChats();
      if (mountedRef.current) setChats(next);
    } catch (reason) {
      if (mountedRef.current) setError(reason instanceof Error ? reason.message : "Failed to load gateway chats.");
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadChats();
  }, [daemonThreadId, loadChats]);

  const link = async (channelKey: string | null) => {
    if (!daemonThreadId) return;
    setPendingKey(channelKey ?? "");
    setError(null);
    try {
      const next = await linkThreadToGatewayChat(daemonThreadId, channelKey);
      if (mountedRef.current) setChats(next);
    } catch (reason) {
      if (mountedRef.current) setError(reason instanceof Error ? reason.message : "Gateway link failed.");
      await loadChats();
    } finally {
      if (mountedRef.current) setPendingKey(null);
      notifyThreadListRefresh();
    }
  };

  const linked = linkedGatewayChat(chats, daemonThreadId);
  const pending = pendingKey !== null;

  return (
    <div className="zorai-handoff-control">
      <button
        type="button"
        className={icon ? ["zorai-thread-header__icon", linked ? "is-active" : ""].filter(Boolean).join(" ") : "zorai-ghost-button"}
        aria-expanded={open}
        aria-label={icon ? (linked ? `Linked to ${gatewayChatLabel(linked)}` : "Continue on gateway") : undefined}
        title={linked ? `Linked to ${gatewayChatLabel(linked)}` : icon ? "Continue on gateway" : undefined}
        onClick={() => {
          if (!open) void loadChats();
          togglePanel("gateway");
        }}
      >
        {icon ?? (linked ? `↔ ${gatewayChatLabel(linked)}` : "Continue on gateway")}
      </button>
      {open ? (
        <ThreadSidePanel title="Continue on a messaging app" subtitle="Link this thread to a gateway chat" onClose={() => closePanel("gateway")}>
        <div className="zorai-handoff-popover zorai-handoff-popover--panel">
          {!daemonThreadId ? (
            <p className="zorai-inline-error">
              Send the first message to create the daemon thread before linking it to a gateway chat.
            </p>
          ) : null}
          {chats.length === 0 ? (
            <p>{loading ? "Loading gateway chats…" : "No gateway chats yet. A chat must message zorai once (Telegram, Discord, Slack or WhatsApp) before a thread can continue there."}</p>
          ) : (
            <ul className="zorai-handoff-stack" aria-label="Gateway chats">
              {chats.map((chat) => {
                const state = gatewayChatLinkState(chat, daemonThreadId);
                return (
                  <li key={chat.channel_key}>
                    <button
                      type="button"
                      className="zorai-ghost-button"
                      aria-pressed={state === "this"}
                      disabled={!daemonThreadId || pending || state === "this"}
                      onClick={() => void link(chat.channel_key)}
                    >
                      <strong>{gatewayChatLabel(chat)}</strong>
                      {" · "}
                      {pendingKey === chat.channel_key
                        ? "Linking…"
                        : state === "this"
                          ? "Linked to this thread"
                          : state === "other"
                            ? `On ${chat.thread_title ?? chat.thread_id}`
                            : "Not linked"}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {error ? <p className="zorai-inline-error" role="alert">{error}</p> : null}
          {linked ? (
            <div className="zorai-card-actions">
              <button
                type="button"
                className="zorai-ghost-button"
                disabled={pending}
                onClick={() => void link(null)}
              >
                {pendingKey === "" ? "Unlinking…" : "Unlink"}
              </button>
            </div>
          ) : null}
        </div>
        </ThreadSidePanel>
      ) : null}
    </div>
  );
}
