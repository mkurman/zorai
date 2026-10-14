import { useEffect, useMemo, useRef, useState } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { BrowserChrome } from "./web-browser-panel/BrowserChrome";
import { getBrowserContainerStyle, type WebBrowserPanelProps } from "./web-browser-panel/shared";
import { useWebBrowserController } from "./web-browser-panel/useWebBrowserController";
import { WebviewFrame } from "./web-browser-panel/WebviewFrame";

const DEFAULT_BROWSER_PANEL_WIDTH = 620;
const MIN_BROWSER_PANEL_WIDTH = 420;
const MAX_BROWSER_PANEL_WIDTH = 1200;
const RESIZE_HANDLE_WIDTH = 10;

function clampBrowserWidth(width: number, viewportWidth: number): number {
    const viewportMaxWidth = Math.floor(viewportWidth * 0.8);
    const maxWidth = Math.max(MIN_BROWSER_PANEL_WIDTH, Math.min(MAX_BROWSER_PANEL_WIDTH, viewportMaxWidth));
    return Math.min(Math.max(width, MIN_BROWSER_PANEL_WIDTH), maxWidth);
}

function parseWidthValue(value: unknown): number | null {
    if (typeof value === "number" && Number.isFinite(value)) {
        return value;
    }

    if (typeof value === "string") {
        const parsed = Number.parseFloat(value);
        return Number.isFinite(parsed) ? parsed : null;
    }

    return null;
}

export function WebBrowserPanel({ style, className, enableInternalResize = false }: WebBrowserPanelProps = {}) {
    const activeWorkspaceId = useWorkspaceStore((s) => s.activeWorkspaceId);
    const webBrowserOpen = useWorkspaceStore((s) => s.webBrowserOpen);
    const webBrowserUrl = useWorkspaceStore((s) => s.webBrowserUrl);
    const webBrowserReloadToken = useWorkspaceStore((s) => s.webBrowserReloadToken);
    const webBrowserFullscreen = useWorkspaceStore((s) => s.webBrowserFullscreen);
    const setWebBrowserOpen = useWorkspaceStore((s) => s.setWebBrowserOpen);
    const navigateWebBrowser = useWorkspaceStore((s) => s.navigateWebBrowser);
    const webBrowserBack = useWorkspaceStore((s) => s.webBrowserBack);
    const webBrowserForward = useWorkspaceStore((s) => s.webBrowserForward);
    const webBrowserReload = useWorkspaceStore((s) => s.webBrowserReload);
    const toggleWebBrowserFullscreen = useWorkspaceStore((s) => s.toggleWebBrowserFullscreen);

    const webviewRef = useRef<any>(null);
    const panelRef = useRef<HTMLDivElement | null>(null);
    const [address, setAddress] = useState(webBrowserUrl);
    const [pageTitle, setPageTitle] = useState("Browser");
    const [isDomReady, setIsDomReady] = useState(false);
    const [panelWidth, setPanelWidth] = useState(() => parseWidthValue(style?.width) ?? DEFAULT_BROWSER_PANEL_WIDTH);

    useEffect(() => {
        setAddress(webBrowserUrl);
    }, [webBrowserUrl]);

    useEffect(() => {
        const configuredWidth = parseWidthValue(style?.width);
        if (configuredWidth !== null) {
            setPanelWidth(configuredWidth);
        }
    }, [style?.width]);

    useEffect(() => {
        const webview = webviewRef.current;
        if (!webview || !isDomReady || typeof webview.reload !== "function") return;
        if (webBrowserReloadToken <= 0) return;
        webview.reload();
    }, [isDomReady, webBrowserReloadToken]);

    useEffect(() => {
        const webview = webviewRef.current;
        if (!webview) return;

        const handleDomReady = () => {
            setIsDomReady(true);
        };

        const handleNavigate = (event: any) => {
            const nextUrl = String(event?.url || "");
            if (nextUrl) {
                navigateWebBrowser(nextUrl);
            }
            if (typeof webview.getTitle === "function") {
                const title = String(webview.getTitle() || "").trim();
                if (title) setPageTitle(title);
            }
        };

        const handlePageTitle = (event: any) => {
            const title = String(event?.title || "").trim();
            if (title) setPageTitle(title);
        };

        const handleDidFailLoad = (event: any) => {
            const errorCode = Number(event?.errorCode);
            if (errorCode === -3) {
                // ERR_ABORTED is expected when a navigation gets superseded.
                return;
            }

            const description = String(event?.errorDescription || "unknown error");
            const failedUrl = String(event?.validatedURL || event?.url || "");
            console.warn("[WebBrowserPanel] failed to load URL", { errorCode, description, failedUrl });
        };

        webview.addEventListener("dom-ready", handleDomReady);
        webview.addEventListener("did-navigate", handleNavigate);
        webview.addEventListener("did-navigate-in-page", handleNavigate);
        webview.addEventListener("page-title-updated", handlePageTitle);
        webview.addEventListener("did-fail-load", handleDidFailLoad);

        return () => {
            setIsDomReady(false);
            webview.removeEventListener("dom-ready", handleDomReady);
            webview.removeEventListener("did-navigate", handleNavigate);
            webview.removeEventListener("did-navigate-in-page", handleNavigate);
            webview.removeEventListener("page-title-updated", handlePageTitle);
            webview.removeEventListener("did-fail-load", handleDidFailLoad);
        };
    }, [navigateWebBrowser]);

    useWebBrowserController({
        webviewRef,
        isDomReady,
        navigateWebBrowser,
        webBrowserBack,
        webBrowserForward,
        webBrowserReload,
        webBrowserUrl,
        pageTitle,
    });

    const containerStyle = useMemo(() => {
        return getBrowserContainerStyle(webBrowserFullscreen);
    }, [webBrowserFullscreen]);

    const handleResizeStart = (event: React.PointerEvent<HTMLDivElement>) => {
        if (!enableInternalResize || webBrowserFullscreen || !panelRef.current) {
            return;
        }

        event.preventDefault();
        event.stopPropagation();

        const pointerId = event.pointerId;
        const target = event.currentTarget;
        const rect = panelRef.current.getBoundingClientRect();
        let lastClientX = event.clientX;

        setPanelWidth(clampBrowserWidth(rect.width, window.innerWidth));

        target.setPointerCapture(pointerId);
        document.body.style.userSelect = "none";
        document.body.style.cursor = "col-resize";

        const onPointerMove = (moveEvent: PointerEvent) => {
            const widthDelta = lastClientX - moveEvent.clientX;
            lastClientX = moveEvent.clientX;

            setPanelWidth((currentWidth) => clampBrowserWidth((currentWidth || rect.width) + widthDelta, window.innerWidth));
        };

        const onPointerEnd = () => {
            if (target.hasPointerCapture(pointerId)) {
                target.releasePointerCapture(pointerId);
            }
            document.body.style.userSelect = "";
            document.body.style.cursor = "";
            window.removeEventListener("pointermove", onPointerMove);
            window.removeEventListener("pointerup", onPointerEnd);
            window.removeEventListener("pointercancel", onPointerEnd);
        };

        window.addEventListener("pointermove", onPointerMove);
        window.addEventListener("pointerup", onPointerEnd);
        window.addEventListener("pointercancel", onPointerEnd);
    };

    if (!webBrowserOpen) return null;

    return (
        <div
            ref={panelRef}
            style={{
                ...containerStyle,
                ...(style ?? {}),
                ...(enableInternalResize && !webBrowserFullscreen
                    ? {
                        width: panelWidth,
                        flexBasis: `${panelWidth}px`,
                        flexGrow: 0,
                        flexShrink: 0,
                    }
                    : {}),
            }}
            className={className}
        >
            {enableInternalResize && !webBrowserFullscreen ? (
                <div
                    onPointerDown={handleResizeStart}
                    style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: RESIZE_HANDLE_WIDTH,
                        height: "100%",
                        cursor: "col-resize",
                        zIndex: 30,
                        background: "linear-gradient(90deg, color-mix(in srgb, var(--text-secondary) 28%, transparent), color-mix(in srgb, var(--text-secondary) 50%, transparent), transparent)",
                    }}
                />
            ) : null}

            <BrowserChrome
                address={address}
                setAddress={setAddress}
                pageTitle={pageTitle}
                back={webBrowserBack}
                forward={webBrowserForward}
                reload={webBrowserReload}
                navigate={navigateWebBrowser}
                toggleFullscreen={toggleWebBrowserFullscreen}
                close={() => setWebBrowserOpen(false)}
            />

            <WebviewFrame
                activeWorkspaceId={activeWorkspaceId}
                webviewRef={webviewRef}
                webBrowserUrl={webBrowserUrl}
            />
        </div>
    );
}
