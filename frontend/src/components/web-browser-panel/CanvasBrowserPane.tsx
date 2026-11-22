import { createElement, useCallback, useEffect, useRef, useState } from "react";
import { useWorkspaceStore } from "../../lib/workspaceStore";
import { registerCanvasBrowserController } from "../../lib/canvasBrowserRegistry";
import { BrowserChrome } from "./BrowserChrome";

const SHADOW_STYLE_ID = "zorai-webview-stretch-style";

export function CanvasBrowserPane({
  paneId,
  initialUrl,
}: {
  paneId: string;
  initialUrl: string;
}) {
  const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
  const updateCanvasPanelUrl = useWorkspaceStore((s) => s.updateCanvasPanelUrl);
  const updateCanvasPanelTitle = useWorkspaceStore((s) => s.updateCanvasPanelTitle);

  const webviewRef = useRef<any>(null);
  const [address, setAddress] = useState(initialUrl);
  const [currentUrl, setCurrentUrl] = useState(initialUrl);
  const [pageTitle, setPageTitle] = useState("Browser");
  const currentUrlRef = useRef(currentUrl);
  const pageTitleRef = useRef(pageTitle);
  currentUrlRef.current = currentUrl;
  pageTitleRef.current = pageTitle;

  const navigate = useCallback((url: string) => {
    const webview = webviewRef.current;
    if (!webview) return;
    const normalized = url.match(/^https?:\/\//) ? url : `https://${url}`;
    setCurrentUrl(normalized);
    setAddress(normalized);
    updateCanvasPanelUrl(paneId, normalized);
    if (typeof webview.loadURL === "function") {
      webview.loadURL(normalized);
    }
  }, [paneId, updateCanvasPanelUrl]);

  const back = useCallback(() => {
    const webview = webviewRef.current;
    if (webview && typeof webview.goBack === "function") {
      webview.goBack();
    }
  }, []);

  const forward = useCallback(() => {
    const webview = webviewRef.current;
    if (webview && typeof webview.goForward === "function") {
      webview.goForward();
    }
  }, []);

  const reload = useCallback(() => {
    const webview = webviewRef.current;
    if (webview && typeof webview.reload === "function") {
      webview.reload();
    }
  }, []);

  useEffect(() => {
    const webview = webviewRef.current;
    if (!webview) return;

    const handleDomReady = () => {
      // Apply shadow DOM stretch styles for proper sizing
      const shadowRoot = webview.shadowRoot;
      if (shadowRoot) {
        let styleTag = shadowRoot.getElementById(SHADOW_STYLE_ID) as HTMLStyleElement | null;
        if (!styleTag) {
          styleTag = document.createElement("style");
          styleTag.id = SHADOW_STYLE_ID;
          styleTag.textContent = `
            :host {
              display: flex !important;
              flex: 1 1 auto !important;
              width: 100% !important;
              height: 100% !important;
              min-height: 0 !important;
            }
            iframe {
              flex: 1 1 auto !important;
              align-self: stretch !important;
              width: 100% !important;
              height: 100% !important;
              min-height: 0 !important;
              border: 0 !important;
            }
          `;
          shadowRoot.appendChild(styleTag);
        }
      }

      if (typeof webview.setAutoResize === "function") {
        try {
          webview.setAutoResize({ width: true, height: true });
        } catch {
          // noop
        }
      }
    };

    const handleNavigate = (event: any) => {
      const nextUrl = String(event?.url || "");
      if (nextUrl) {
        setAddress(nextUrl);
        setCurrentUrl(nextUrl);
        updateCanvasPanelUrl(paneId, nextUrl);
      }
      if (typeof webview.getTitle === "function") {
        const title = String(webview.getTitle() || "").trim();
        if (title) {
          setPageTitle(title);
          updateCanvasPanelTitle(paneId, title);
        }
      }
    };

    const handlePageTitle = (event: any) => {
      const title = String(event?.title || "").trim();
      if (title) {
        setPageTitle(title);
        updateCanvasPanelTitle(paneId, title);
      }
    };

    const handleDidFailLoad = (event: any) => {
      const errorCode = Number(event?.errorCode);
      if (errorCode === -3) return; // ERR_ABORTED
      console.warn("[CanvasBrowserPane] failed to load URL", {
        errorCode,
        description: String(event?.errorDescription || "unknown error"),
        failedUrl: String(event?.validatedURL || event?.url || ""),
      });
    };

    webview.addEventListener("dom-ready", handleDomReady);
    webview.addEventListener("did-navigate", handleNavigate);
    webview.addEventListener("did-navigate-in-page", handleNavigate);
    webview.addEventListener("page-title-updated", handlePageTitle);
    webview.addEventListener("did-fail-load", handleDidFailLoad);

    return () => {
      webview.removeEventListener("dom-ready", handleDomReady);
      webview.removeEventListener("did-navigate", handleNavigate);
      webview.removeEventListener("did-navigate-in-page", handleNavigate);
      webview.removeEventListener("page-title-updated", handlePageTitle);
      webview.removeEventListener("did-fail-load", handleDidFailLoad);
    };
  }, [paneId, updateCanvasPanelUrl, updateCanvasPanelTitle]);

  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;

  useEffect(() => {
    return registerCanvasBrowserController(paneId, {
      getUrl: () => currentUrlRef.current,
      getTitle: () => pageTitleRef.current,
      navigate: (url: string) => navigateRef.current(url),
      getDomSnapshot: async () => {
        const webview = webviewRef.current;
        const url = currentUrlRef.current;
        const title = pageTitleRef.current;
        if (!webview || typeof webview.executeJavaScript !== "function") {
          return { title, url, text: "" };
        }
        try {
          const text: string = await webview.executeJavaScript("document.body.innerText");
          return { title, url, text: text ?? "" };
        } catch {
          return { title, url, text: "" };
        }
      },
      executeJavaScript: async (code: string) => {
        const webview = webviewRef.current;
        if (!webview || typeof webview.executeJavaScript !== "function") {
          throw new Error("Webview not available");
        }
        return webview.executeJavaScript(code);
      },
    });
  }, [paneId]);

  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        background: "var(--zorai-bg-panel)",
      }}
    >
      <BrowserChrome
        address={address}
        setAddress={setAddress}
        pageTitle={pageTitle}
        back={back}
        forward={forward}
        reload={reload}
        navigate={navigate}
        showFullscreen={false}
      />

      <div
        style={{
          flex: 1,
          minHeight: 0,
          height: "100%",
          minWidth: 0,
          display: "flex",
          overflow: "hidden",
          position: "relative",
        }}
      >
        {createElement("webview" as any, {
          key: `${paneId}-${activeWorkspaceId ?? "default"}`,
          ref: webviewRef,
          src: currentUrl,
          style: {
            display: "block",
            position: "absolute",
            inset: 0,
            width: "100%",
            minWidth: 0,
            minHeight: 0,
            height: "100%",
            border: "none",
            background: "#ffffff",
          },
          allowpopups: "true",
          partition: "persist:zorai-browser",
        })}
      </div>
    </div>
  );
}
