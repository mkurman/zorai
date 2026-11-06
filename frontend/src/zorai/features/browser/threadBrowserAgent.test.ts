import { beforeEach, describe, expect, it, vi } from "vitest";

const bridge = vi.hoisted(() => ({ agentBrowserResult: vi.fn(async () => ({ ok: true })) }));
vi.mock("@/lib/bridge", () => ({ getBridge: () => bridge }));

import { handleBrowserRequestEvent, OPERATOR_CONTROL_MESSAGE, runBrowserOp } from "./threadBrowserAgent";
import { markBrowserWebviewReady, type BrowserWebview } from "./threadBrowserRegistry";
import { getBrowserSession, setBrowserOperatorControl, useThreadBrowserStore, withTabRemoved, emptyBrowserSession, setBrowserPanelOpen, normalizeBrowserAddress } from "./threadBrowserStore";

(globalThis as unknown as { window: typeof globalThis }).window ??= globalThis;

function fakeWebview(overrides: Partial<BrowserWebview> = {}) {
  let url = "about:blank";
  const webview = {
    loadURL: vi.fn(async (next: string) => {
      url = next;
    }),
    getURL: () => url,
    getTitle: () => (url === "about:blank" ? "" : `Title of ${url}`),
    executeJavaScript: vi.fn(async () => "page-value"),
    capturePage: vi.fn(async () => ({ toDataURL: () => "data:image/png;base64,AAAA" })),
    canGoBack: () => false,
    canGoForward: () => false,
    goBack: vi.fn(),
    goForward: vi.fn(),
    reload: vi.fn(),
    isLoading: () => false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    ...overrides,
  };
  return webview as unknown as BrowserWebview & typeof webview;
}

function attachWebviewsAsTabsOpen() {
  const created: Array<BrowserWebview & ReturnType<typeof fakeWebview>> = [];
  return useThreadBrowserStore.subscribe((state, previous) => {
    for (const session of Object.values(state.sessions)) {
      for (const tab of session.tabs) {
        const existed = Object.values(previous.sessions).some((old) => old.tabs.some((oldTab) => oldTab.id === tab.id));
        if (!existed) {
          const webview = fakeWebview();
          created.push(webview);
          queueMicrotask(() => markBrowserWebviewReady(tab.id, webview));
        }
      }
    }
  });
}

describe("agent browser requests in the desktop app", () => {
  beforeEach(() => {
    useThreadBrowserStore.setState({ sessions: {} });
    bridge.agentBrowserResult.mockClear();
  });

  it("refuses agent actions while the operator has taken control, without touching the page", async () => {
    setBrowserPanelOpen("thread_1", true);
    setBrowserOperatorControl("thread_1", true);

    await expect(runBrowserOp("thread_1", { kind: "navigate", url: "https://example.com" })).rejects.toThrow(OPERATOR_CONTROL_MESSAGE);
    expect(getBrowserSession("thread_1").tabs[0]?.url).toBe("about:blank");
  });

  it("opens a tab on first use and reports where the navigation landed", async () => {
    const stop = attachWebviewsAsTabsOpen();
    const info = await runBrowserOp("thread_1", { kind: "navigate", url: "http://127.0.0.1:5173/" });
    stop();

    expect(info).toEqual({ url: "http://127.0.0.1:5173/", title: "Title of http://127.0.0.1:5173/" });
    expect(getBrowserSession("thread_1").tabs).toHaveLength(1);
  });

  it("treats a navigation the page redirected (ERR_ABORTED) as success instead of an error", async () => {
    setBrowserPanelOpen("thread_1", true);
    const tabId = getBrowserSession("thread_1").activeTabId!;
    const webview = fakeWebview({ loadURL: vi.fn(async () => { throw new Error("ERR_ABORTED (-3) loading 'https://example.com/'"); }) });
    markBrowserWebviewReady(tabId, webview);

    await expect(runBrowserOp("thread_1", { kind: "navigate", url: "https://example.com/" })).resolves.toMatchObject({ url: "about:blank" });
  });

  it("runs page scripts as a user gesture and returns their value", async () => {
    setBrowserPanelOpen("thread_1", true);
    const webview = fakeWebview();
    markBrowserWebviewReady(getBrowserSession("thread_1").activeTabId!, webview);

    await expect(runBrowserOp("thread_1", { kind: "evaluate", script: "document.title" })).resolves.toEqual({ value: "page-value" });
    expect(webview.executeJavaScript).toHaveBeenCalledWith("document.title", true);
  });

  it("lets the agent open, switch and close tabs, and rejects tabs that are not in this thread", async () => {
    const stop = attachWebviewsAsTabsOpen();
    setBrowserPanelOpen("thread_1", true);
    const first = getBrowserSession("thread_1").activeTabId!;
    const opened = await runBrowserOp("thread_1", { kind: "tabs", action: "new", url: "https://example.com/" }) as { tabs: Array<{ id: string; active: boolean }> };
    stop();

    expect(opened.tabs).toHaveLength(2);
    expect(opened.tabs.find((tab) => tab.active)?.id).not.toBe(first);
    const switched = await runBrowserOp("thread_1", { kind: "tabs", action: "switch", tab_id: first }) as { tabs: Array<{ id: string; active: boolean }> };
    expect(switched.tabs.find((tab) => tab.active)?.id).toBe(first);
    await expect(runBrowserOp("thread_1", { kind: "tabs", action: "close", tab_id: "tab-from-another-thread" })).rejects.toThrow("browser_tabs");
    const closed = await runBrowserOp("thread_1", { kind: "tabs", action: "close", tab_id: first }) as { tabs: unknown[] };
    expect(closed.tabs).toHaveLength(1);
  });

  it("answers the daemon over the bridge and marks the browser as agent-driven", async () => {
    setBrowserPanelOpen("thread_1", true);
    setBrowserOperatorControl("thread_1", true);

    await handleBrowserRequestEvent({ thread_id: "thread_1", request_id: "br_1", op: { kind: "screenshot" } });

    expect(bridge.agentBrowserResult).toHaveBeenCalledWith("br_1", false, JSON.stringify(OPERATOR_CONTROL_MESSAGE));
    expect(getBrowserSession("thread_1")).toMatchObject({ agentUsed: true, agentBusy: 0, lastAgentAction: "Taking a screenshot" });

    setBrowserOperatorControl("thread_1", false);
    markBrowserWebviewReady(getBrowserSession("thread_1").activeTabId!, fakeWebview());
    await handleBrowserRequestEvent({ thread_id: "thread_1", request_id: "br_2", op: { kind: "screenshot" } });
    expect(bridge.agentBrowserResult).toHaveBeenLastCalledWith("br_2", true, JSON.stringify({ png_base64: "data:image/png;base64,AAAA" }));
  });
});

describe("browser tabs and address bar", () => {
  it("activates the neighbouring tab when the active one is closed", () => {
    const session = { ...emptyBrowserSession(), activeTabId: "b", tabs: ["a", "b", "c"].map((id) => ({ id, url: "", title: "", loading: false })) };

    expect(withTabRemoved(session, "b").activeTabId).toBe("c");
    expect(withTabRemoved({ ...session, activeTabId: "c" }, "c").activeTabId).toBe("b");
    expect(withTabRemoved(session, "a").activeTabId).toBe("b");
  });

  it("turns typed addresses into URLs and everything else into a search", () => {
    expect(normalizeBrowserAddress("127.0.0.1:5173")).toBe("http://127.0.0.1:5173");
    expect(normalizeBrowserAddress("localhost:3000/app")).toBe("http://localhost:3000/app");
    expect(normalizeBrowserAddress("example.com/docs")).toBe("https://example.com/docs");
    expect(normalizeBrowserAddress("https://a.b/c")).toBe("https://a.b/c");
    expect(normalizeBrowserAddress("how to center a div")).toBe("https://duckduckgo.com/?q=how%20to%20center%20a%20div");
  });
});
