import { createElement, useEffect, useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { setBrowserTabLoading, syncBrowserTab } from "./threadBrowserAgent";
import {
  browserSlot,
  browserSlotKeys,
  forgetBrowserWebview,
  markBrowserWebviewReady,
  subscribeBrowserSlots,
  type BrowserWebview,
} from "./threadBrowserRegistry";
import { BROWSER_HOME_URL, useThreadBrowserStore, type ThreadBrowserTab } from "./threadBrowserStore";

const PARKED_WIDTH = 1280;
const PARKED_HEIGHT = 800;

function placeContainer(container: HTMLElement, slot: HTMLElement | null) {
  const rect = slot?.getBoundingClientRect();
  if (rect && rect.width > 0 && rect.height > 0) {
    container.style.left = `${rect.left}px`;
    container.style.top = `${rect.top}px`;
    container.style.width = `${rect.width}px`;
    container.style.height = `${rect.height}px`;
    container.dataset.placed = "true";
  } else {
    container.style.left = `${-PARKED_WIDTH - 10_000}px`;
    container.style.top = "0px";
    container.style.width = `${PARKED_WIDTH}px`;
    container.style.height = `${PARKED_HEIGHT}px`;
    container.dataset.placed = "false";
  }
}

function BrowserTabView({ sessionKey, tab, active, shielded }: { sessionKey: string; tab: ThreadBrowserTab; active: boolean; shielded: boolean }) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const webviewRef = useRef<BrowserWebview | null>(null);
  const initialUrl = useRef(tab.url);

  useEffect(() => {
    const webview = webviewRef.current;
    if (!webview) return;
    let ready = false;
    const onDomReady = () => {
      if (!ready) {
        ready = true;
        markBrowserWebviewReady(tab.id, webview);
      }
      syncBrowserTab(sessionKey, tab.id, webview);
    };
    const sync = (loading?: boolean) => {
      if (ready) syncBrowserTab(sessionKey, tab.id, webview, loading);
      else if (loading !== undefined) setBrowserTabLoading(sessionKey, tab.id, loading);
    };
    const onNavigate = () => sync();
    const onStart = () => sync(true);
    const onStop = () => sync(false);
    webview.addEventListener("dom-ready", onDomReady);
    webview.addEventListener("did-navigate", onNavigate);
    webview.addEventListener("did-navigate-in-page", onNavigate);
    webview.addEventListener("page-title-updated", onNavigate);
    webview.addEventListener("did-start-loading", onStart);
    webview.addEventListener("did-stop-loading", onStop);
    return () => {
      webview.removeEventListener("dom-ready", onDomReady);
      webview.removeEventListener("did-navigate", onNavigate);
      webview.removeEventListener("did-navigate-in-page", onNavigate);
      webview.removeEventListener("page-title-updated", onNavigate);
      webview.removeEventListener("did-start-loading", onStart);
      webview.removeEventListener("did-stop-loading", onStop);
      forgetBrowserWebview(tab.id);
    };
  }, [sessionKey, tab.id]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    if (!active) {
      placeContainer(container, null);
      return;
    }
    let frame = 0;
    const follow = () => {
      placeContainer(container, browserSlot(sessionKey));
      frame = window.requestAnimationFrame(follow);
    };
    follow();
    return () => window.cancelAnimationFrame(frame);
  }, [active, sessionKey]);

  return (
    <div ref={containerRef} className="zorai-thread-browser-host__tab" data-tab-id={tab.id}>
      {createElement("webview", {
        ref: webviewRef,
        src: initialUrl.current,
        partition: "persist:zorai-browser",
        allowpopups: "true",
        webpreferences: "backgroundThrottling=no",
        className: "zorai-thread-browser-host__webview",
      })}
      {active && shielded ? <div className="zorai-thread-browser-host__shield" title="Zorai is using this browser. Take control in the panel above to use it yourself." /> : null}
    </div>
  );
}

export function ThreadBrowserHost() {
  const sessions = useThreadBrowserStore((state) => state.sessions);
  useSyncExternalStore(subscribeBrowserSlots, () => browserSlotKeys().join("|"), () => "");
  const tabs = Object.entries(sessions).flatMap(([sessionKey, session]) =>
    session.tabs.map((tab) => ({
      sessionKey,
      tab,
      active: session.panelOpen && session.activeTabId === tab.id && tab.url !== BROWSER_HOME_URL,
      shielded: session.agentUsed && !session.operatorControl,
    })),
  );
  if (tabs.length === 0) return null;
  return createPortal(
    <div className="zorai-thread-browser-host" aria-hidden={tabs.every((entry) => !entry.active)}>
      {tabs.map((entry) => (
        <BrowserTabView key={entry.tab.id} {...entry} />
      ))}
    </div>,
    document.body,
  );
}
