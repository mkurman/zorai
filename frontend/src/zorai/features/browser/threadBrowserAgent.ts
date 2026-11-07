import { getBridge } from "@/lib/bridge";
import {
  browserTabsResult,
  closeBrowserTab,
  getBrowserSession,
  openBrowserTab,
  updateBrowserSession,
  withTabUpdated,
} from "./threadBrowserStore";
import { whenBrowserWebviewReady, type BrowserWebview } from "./threadBrowserRegistry";

export type BrowserOp =
  | { kind: "navigate"; url: string }
  | { kind: "evaluate"; script: string }
  | { kind: "screenshot" }
  | { kind: "history"; direction: "back" | "forward" | "reload" }
  | { kind: "tabs"; action: "list" | "new" | "switch" | "close"; tab_id?: string; url?: string };

export const OPERATOR_CONTROL_MESSAGE =
  "The operator has taken control of the Zorai browser. Wait until they hand it back, or ask them what they need.";

const LOAD_TIMEOUT_MS = 30_000;
const ABORTED_NAVIGATION = /ERR_ABORTED|\(-3\)/;

export function describeBrowserOp(op: BrowserOp): string {
  if (op.kind === "navigate") return `Opening ${op.url}`;
  if (op.kind === "evaluate") return "Reading the page";
  if (op.kind === "screenshot") return "Taking a screenshot";
  if (op.kind === "history") return op.direction === "reload" ? "Reloading" : `Going ${op.direction}`;
  return op.action === "list" ? "Checking tabs" : `${op.action[0].toUpperCase()}${op.action.slice(1)} tab`;
}

function waitForLoadStop(webview: BrowserWebview, timeoutMs = LOAD_TIMEOUT_MS): Promise<void> {
  return new Promise((resolve) => {
    const finish = () => {
      window.clearTimeout(timer);
      webview.removeEventListener("did-stop-loading", finish);
      resolve();
    };
    const timer = window.setTimeout(finish, timeoutMs);
    webview.addEventListener("did-stop-loading", finish);
  });
}

function pageInfo(webview: BrowserWebview) {
  return { url: webview.getURL(), title: webview.getTitle() };
}

async function activeWebview(sessionKey: string): Promise<BrowserWebview> {
  const session = getBrowserSession(sessionKey);
  const tabId = session.activeTabId ?? openBrowserTab(sessionKey);
  return whenBrowserWebviewReady(tabId);
}

async function navigate(webview: BrowserWebview, url: string) {
  try {
    await Promise.race([
      webview.loadURL(url),
      new Promise((_, reject) => window.setTimeout(() => reject(new Error(`Loading ${url} took longer than ${LOAD_TIMEOUT_MS / 1000}s.`)), LOAD_TIMEOUT_MS)),
    ]);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!ABORTED_NAVIGATION.test(message)) throw error;
  }
  return pageInfo(webview);
}

export async function runBrowserOp(sessionKey: string, op: BrowserOp): Promise<unknown> {
  if (getBrowserSession(sessionKey).operatorControl) throw new Error(OPERATOR_CONTROL_MESSAGE);
  if (op.kind === "tabs") {
    if (op.action === "new") {
      const tabId = openBrowserTab(sessionKey);
      const webview = await whenBrowserWebviewReady(tabId);
      if (op.url) await navigate(webview, op.url);
    } else if (op.action === "switch" || op.action === "close") {
      const tabId = op.tab_id;
      if (!tabId || !getBrowserSession(sessionKey).tabs.some((tab) => tab.id === tabId)) {
        throw new Error(`No tab ${tabId ?? "(missing tab_id)"} in this thread's browser; call browser_tabs to list them.`);
      }
      if (op.action === "close") closeBrowserTab(sessionKey, tabId);
      else updateBrowserSession(sessionKey, (session) => ({ ...session, activeTabId: tabId }));
    }
    return browserTabsResult(getBrowserSession(sessionKey));
  }
  const webview = await activeWebview(sessionKey);
  if (op.kind === "navigate") return navigate(webview, op.url);
  if (op.kind === "evaluate") return { value: await webview.executeJavaScript(op.script, true) };
  if (op.kind === "screenshot") return { png_base64: (await webview.capturePage()).toDataURL() };
  if (op.direction === "back" && webview.canGoBack()) webview.goBack();
  else if (op.direction === "forward" && webview.canGoForward()) webview.goForward();
  else if (op.direction === "reload") webview.reload();
  else return pageInfo(webview);
  await waitForLoadStop(webview);
  return pageInfo(webview);
}

export async function handleBrowserRequestEvent(event: { thread_id?: unknown; request_id?: unknown; op?: unknown }): Promise<void> {
  const sessionKey = typeof event.thread_id === "string" ? event.thread_id : "";
  const requestId = typeof event.request_id === "string" ? event.request_id : "";
  const reply = getBridge()?.agentBrowserResult;
  if (!sessionKey || !requestId || !reply) return;
  const op = event.op as BrowserOp;
  updateBrowserSession(sessionKey, (session) => ({
    ...session,
    agentUsed: true,
    agentBusy: session.agentBusy + 1,
    lastAgentAction: describeBrowserOp(op),
  }));
  try {
    const result = await runBrowserOp(sessionKey, op);
    await reply(requestId, true, JSON.stringify(result ?? null));
  } catch (error) {
    await reply(requestId, false, JSON.stringify(error instanceof Error ? error.message : String(error)));
  } finally {
    updateBrowserSession(sessionKey, (session) => ({ ...session, agentBusy: Math.max(0, session.agentBusy - 1) }));
  }
}

export function releaseBrowserFromAgent(sessionKey: string | null): void {
  if (!sessionKey || !getBrowserSession(sessionKey).agentUsed) return;
  updateBrowserSession(sessionKey, (session) => ({ ...session, agentUsed: false, operatorControl: false, lastAgentAction: null }));
}

export function setBrowserTabLoading(sessionKey: string, tabId: string, loading: boolean): void {
  updateBrowserSession(sessionKey, (session) => withTabUpdated(session, tabId, { loading }));
}

export function syncBrowserTab(sessionKey: string, tabId: string, webview: BrowserWebview, loading?: boolean): void {
  updateBrowserSession(sessionKey, (session) => withTabUpdated(session, tabId, {
    url: webview.getURL(),
    title: webview.getTitle(),
    ...(loading === undefined ? {} : { loading }),
  }));
}
