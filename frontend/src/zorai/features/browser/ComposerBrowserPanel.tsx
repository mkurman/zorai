import { useEffect, useRef, useState, type FormEvent } from "react";
import { browserWebview, registerBrowserSlot } from "./threadBrowserRegistry";
import { useComposerPanelHeight } from "../threads/useComposerPanelHeight";
import {
  BROWSER_HOME_URL,
  closeBrowserTab,
  emptyBrowserSession,
  normalizeBrowserAddress,
  openBrowserTab,
  setBrowserOperatorControl,
  setBrowserPanelOpen,
  updateBrowserSession,
  useThreadBrowserStore,
} from "./threadBrowserStore";

function tabLabel(title: string, url: string): string {
  if (title) return title;
  if (!url || url === BROWSER_HOME_URL) return "New tab";
  return url.replace(/^https?:\/\//, "");
}

export function ComposerBrowserPanel({ sessionKey }: { sessionKey: string }) {
  const session = useThreadBrowserStore((state) => state.sessions[sessionKey]) ?? emptyBrowserSession();
  const slotRef = useRef<HTMLDivElement | null>(null);
  const resize = useComposerPanelHeight(460, 220);
  const activeTab = session.tabs.find((tab) => tab.id === session.activeTabId) ?? null;
  const [address, setAddress] = useState(activeTab?.url ?? "");
  const [editing, setEditing] = useState(false);
  const agentDriving = session.agentUsed && !session.operatorControl;

  useEffect(() => {
    const slot = slotRef.current;
    return slot ? registerBrowserSlot(sessionKey, slot) : undefined;
  }, [sessionKey]);

  useEffect(() => {
    if (!editing) setAddress(activeTab?.url === BROWSER_HOME_URL ? "" : activeTab?.url ?? "");
  }, [activeTab?.url, editing]);

  const webview = () => (activeTab ? browserWebview(activeTab.id) : null);
  const submitAddress = (event: FormEvent) => {
    event.preventDefault();
    setEditing(false);
    void webview()?.loadURL(normalizeBrowserAddress(address)).catch(() => undefined);
  };

  return (
    <div ref={resize.panelRef} className="zorai-browser-panel" style={{ height: resize.height }}>
      <div className="zorai-composer-panel-resize" {...resize.handleProps} />
      <div className="zorai-browser-panel__tabs" role="tablist" aria-label="Browser tabs">
        {session.tabs.map((tab) => (
          <div
            key={tab.id}
            role="tab"
            aria-selected={tab.id === session.activeTabId}
            className={["zorai-browser-panel__tab", tab.id === session.activeTabId ? "is-active" : ""].filter(Boolean).join(" ")}
            title={tab.url}
            onClick={() => updateBrowserSession(sessionKey, (current) => ({ ...current, activeTabId: tab.id }))}
          >
            {tab.loading ? <span className="zorai-browser-panel__spinner" aria-hidden="true" /> : null}
            <span className="zorai-browser-panel__tab-label">{tabLabel(tab.title, tab.url)}</span>
            <button
              type="button"
              className="zorai-browser-panel__tab-close"
              aria-label={`Close ${tabLabel(tab.title, tab.url)}`}
              disabled={agentDriving}
              onClick={(event) => {
                event.stopPropagation();
                closeBrowserTab(sessionKey, tab.id);
              }}
            >
              ×
            </button>
          </div>
        ))}
        <button type="button" className="zorai-browser-panel__icon" title="New tab" aria-label="New tab" disabled={agentDriving} onClick={() => openBrowserTab(sessionKey)}>+</button>
        <button type="button" className="zorai-browser-panel__icon zorai-browser-panel__hide" title="Hide browser (the agent can keep using it)" aria-label="Hide browser" onClick={() => setBrowserPanelOpen(sessionKey, false)}>×</button>
      </div>
      <form className="zorai-browser-panel__bar" onSubmit={submitAddress}>
        <button type="button" className="zorai-browser-panel__icon" title="Back" aria-label="Back" disabled={agentDriving} onClick={() => webview()?.goBack()}>‹</button>
        <button type="button" className="zorai-browser-panel__icon" title="Forward" aria-label="Forward" disabled={agentDriving} onClick={() => webview()?.goForward()}>›</button>
        <button type="button" className="zorai-browser-panel__icon" title="Reload" aria-label="Reload" disabled={agentDriving} onClick={() => webview()?.reload()}>↻</button>
        <input
          className="zorai-browser-panel__address"
          value={address}
          placeholder="Search or enter address"
          spellCheck={false}
          disabled={agentDriving || !activeTab}
          aria-label="Address"
          onFocus={(event) => {
            setEditing(true);
            event.currentTarget.select();
          }}
          onBlur={() => setEditing(false)}
          onChange={(event) => setAddress(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setEditing(false);
              event.currentTarget.blur();
            }
          }}
        />
      </form>
      {session.agentUsed ? (
        <div className={["zorai-browser-panel__control", agentDriving ? "is-agent" : "is-operator"].join(" ")} role="status">
          <span>
            {agentDriving
              ? `Zorai is using this browser${session.agentBusy > 0 && session.lastAgentAction ? ` · ${session.lastAgentAction}…` : ""}`
              : "You have control. The agent's browser actions are paused."}
          </span>
          <button
            type="button"
            className="zorai-ghost-button zorai-ghost-button--compact"
            onClick={() => setBrowserOperatorControl(sessionKey, agentDriving)}
          >
            {agentDriving ? "Take control" : "Hand back to agent"}
          </button>
        </div>
      ) : null}
      <div ref={slotRef} className="zorai-browser-panel__viewport" />
    </div>
  );
}
