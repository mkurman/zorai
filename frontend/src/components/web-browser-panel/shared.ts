import type { CSSProperties } from "react";

export type WebBrowserPanelProps = {
    style?: CSSProperties;
    className?: string;
    enableInternalResize?: boolean;
};

export function getBrowserContainerStyle(webBrowserFullscreen: boolean): CSSProperties {
    if (webBrowserFullscreen) {
        return {
            position: "fixed",
            inset: 0,
            zIndex: 3500,
            background: "var(--zorai-bg-panel)",
            display: "flex",
            flexDirection: "column",
            minWidth: 0,
            minHeight: 0,
            overflow: "hidden",
        };
    }

    return {
        minWidth: 420,
        maxWidth: 1200,
        height: "100%",
        alignSelf: "stretch",
        display: "flex",
        flexDirection: "column",
        flexShrink: 0,
        minHeight: 0,
        position: "relative",
        background: "var(--zorai-bg-panel)",
        border: "1px solid var(--zorai-border)",
        borderRadius: 10,
        overflow: "hidden",
    };
}
