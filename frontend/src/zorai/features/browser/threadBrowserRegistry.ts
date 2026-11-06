export type BrowserWebview = HTMLElement & {
  loadURL: (url: string) => Promise<void>;
  getURL: () => string;
  getTitle: () => string;
  executeJavaScript: (code: string, userGesture?: boolean) => Promise<unknown>;
  capturePage: () => Promise<{ toDataURL: () => string; isEmpty?: () => boolean }>;
  canGoBack: () => boolean;
  canGoForward: () => boolean;
  goBack: () => void;
  goForward: () => void;
  reload: () => void;
  isLoading: () => boolean;
};

type WebviewEntry = {
  webview: BrowserWebview | null;
  ready: Promise<BrowserWebview>;
  resolve: (webview: BrowserWebview) => void;
};

const webviews = new Map<string, WebviewEntry>();
const slots = new Map<string, HTMLElement>();
const slotListeners = new Set<() => void>();

function entryFor(tabId: string): WebviewEntry {
  let entry = webviews.get(tabId);
  if (!entry) {
    let resolve!: (webview: BrowserWebview) => void;
    const ready = new Promise<BrowserWebview>((done) => {
      resolve = done;
    });
    entry = { webview: null, ready, resolve };
    webviews.set(tabId, entry);
  }
  return entry;
}

export function markBrowserWebviewReady(tabId: string, webview: BrowserWebview): void {
  const entry = entryFor(tabId);
  entry.webview = webview;
  entry.resolve(webview);
}

export function forgetBrowserWebview(tabId: string): void {
  webviews.delete(tabId);
}

export function browserWebview(tabId: string): BrowserWebview | null {
  return webviews.get(tabId)?.webview ?? null;
}

export function whenBrowserWebviewReady(tabId: string, timeoutMs = 15_000): Promise<BrowserWebview> {
  const entry = entryFor(tabId);
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error("The Zorai browser tab did not start in time.")), timeoutMs);
    void entry.ready.then((webview) => {
      window.clearTimeout(timer);
      resolve(webview);
    });
  });
}

export function registerBrowserSlot(sessionKey: string, element: HTMLElement): () => void {
  slots.set(sessionKey, element);
  for (const listener of slotListeners) listener();
  return () => {
    if (slots.get(sessionKey) === element) slots.delete(sessionKey);
    for (const listener of slotListeners) listener();
  };
}

export function browserSlot(sessionKey: string): HTMLElement | null {
  return slots.get(sessionKey) ?? null;
}

export function browserSlotKeys(): string[] {
  return [...slots.keys()];
}

export function subscribeBrowserSlots(listener: () => void): () => void {
  slotListeners.add(listener);
  return () => {
    slotListeners.delete(listener);
  };
}
