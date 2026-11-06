import { create } from "zustand";

export const BROWSER_HOME_URL = "about:blank";

export function normalizeBrowserAddress(input: string): string {
  const value = input.trim();
  if (!value) return BROWSER_HOME_URL;
  if (/^(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/i.test(value)) return `http://${value}`;
  if (/^[a-z][a-z0-9+.-]*:/i.test(value)) return value;
  if (!/\s/.test(value) && /\.[a-z]{2,}(:\d+)?(\/|$)/i.test(value)) return `https://${value}`;
  return `https://duckduckgo.com/?q=${encodeURIComponent(value)}`;
}

export type ThreadBrowserTab = {
  id: string;
  url: string;
  title: string;
  loading: boolean;
};

export type ThreadBrowserSession = {
  tabs: ThreadBrowserTab[];
  activeTabId: string | null;
  panelOpen: boolean;
  agentUsed: boolean;
  operatorControl: boolean;
  agentBusy: number;
  lastAgentAction: string | null;
};

export function emptyBrowserSession(): ThreadBrowserSession {
  return {
    tabs: [],
    activeTabId: null,
    panelOpen: false,
    agentUsed: false,
    operatorControl: false,
    agentBusy: 0,
    lastAgentAction: null,
  };
}

export function withTabAdded(session: ThreadBrowserSession, tab: ThreadBrowserTab): ThreadBrowserSession {
  return { ...session, tabs: [...session.tabs, tab], activeTabId: tab.id };
}

export function withTabRemoved(session: ThreadBrowserSession, tabId: string): ThreadBrowserSession {
  const index = session.tabs.findIndex((tab) => tab.id === tabId);
  if (index < 0) return session;
  const tabs = session.tabs.filter((tab) => tab.id !== tabId);
  const activeTabId = session.activeTabId === tabId
    ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? null)
    : session.activeTabId;
  return { ...session, tabs, activeTabId };
}

export function withTabUpdated(
  session: ThreadBrowserSession,
  tabId: string,
  patch: Partial<Omit<ThreadBrowserTab, "id">>,
): ThreadBrowserSession {
  if (!session.tabs.some((tab) => tab.id === tabId)) return session;
  return { ...session, tabs: session.tabs.map((tab) => (tab.id === tabId ? { ...tab, ...patch } : tab)) };
}

export function browserTabsResult(session: ThreadBrowserSession) {
  return {
    tabs: session.tabs.map((tab) => ({
      id: tab.id,
      url: tab.url,
      title: tab.title,
      active: tab.id === session.activeTabId,
    })),
  };
}

let tabSerial = 0;

export function newBrowserTabId(): string {
  tabSerial += 1;
  return `tab-${Date.now().toString(36)}-${tabSerial}`;
}

type ThreadBrowserState = {
  sessions: Record<string, ThreadBrowserSession>;
  update: (sessionKey: string, change: (session: ThreadBrowserSession) => ThreadBrowserSession) => void;
};

export const useThreadBrowserStore = create<ThreadBrowserState>((set) => ({
  sessions: {},
  update: (sessionKey, change) => set((state) => ({
    sessions: { ...state.sessions, [sessionKey]: change(state.sessions[sessionKey] ?? emptyBrowserSession()) },
  })),
}));

export function getBrowserSession(sessionKey: string): ThreadBrowserSession {
  return useThreadBrowserStore.getState().sessions[sessionKey] ?? emptyBrowserSession();
}

export function updateBrowserSession(
  sessionKey: string,
  change: (session: ThreadBrowserSession) => ThreadBrowserSession,
): void {
  useThreadBrowserStore.getState().update(sessionKey, change);
}

export function openBrowserTab(sessionKey: string, url = BROWSER_HOME_URL): string {
  const id = newBrowserTabId();
  updateBrowserSession(sessionKey, (session) => withTabAdded(session, { id, url, title: "", loading: url !== BROWSER_HOME_URL }));
  return id;
}

export function closeBrowserTab(sessionKey: string, tabId: string): void {
  updateBrowserSession(sessionKey, (session) => withTabRemoved(session, tabId));
}

export function setBrowserPanelOpen(sessionKey: string, panelOpen: boolean): void {
  updateBrowserSession(sessionKey, (session) => {
    const opened = { ...session, panelOpen };
    return panelOpen && opened.tabs.length === 0
      ? withTabAdded(opened, { id: newBrowserTabId(), url: BROWSER_HOME_URL, title: "", loading: false })
      : opened;
  });
}

export function setBrowserOperatorControl(sessionKey: string, operatorControl: boolean): void {
  updateBrowserSession(sessionKey, (session) => ({ ...session, operatorControl }));
}
