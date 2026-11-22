import { PanelFullscreenIcon } from "@/zorai/features/threads/PanelFullscreenIcon";

export function BrowserChrome({
    address,
    setAddress,
    pageTitle,
    back,
    forward,
    reload,
    navigate,
    toggleFullscreen,
    fullscreen = false,
    showFullscreen = true,
}: {
    address: string;
    setAddress: (value: string) => void;
    pageTitle: string;
    back: () => void;
    forward: () => void;
    reload: () => void;
    navigate: (url: string) => void;
    toggleFullscreen?: () => void;
    fullscreen?: boolean;
    showFullscreen?: boolean;
}) {
    return (
        <>
            <div
                data-no-drag="true"
                style={{
                    display: "flex",
                    alignItems: "center",
                    flexShrink: 0,
                    gap: 6,
                    padding: "6px 8px",
                    borderBottom: "1px solid var(--zorai-border)",
                    background: "var(--zorai-bg-panel)",
                    fontFamily: "var(--font-ui)",
                }}
            >
                <button type="button" className="zorai-browser-panel__icon" onClick={back} title="Back" aria-label="Back">‹</button>
                <button type="button" className="zorai-browser-panel__icon" onClick={forward} title="Forward" aria-label="Forward">›</button>
                <button type="button" className="zorai-browser-panel__icon" onClick={reload} title="Reload" aria-label="Reload">↻</button>
                <input
                    className="zorai-browser-panel__address"
                    value={address}
                    placeholder="Search or enter address"
                    spellCheck={false}
                    aria-label="Address"
                    onChange={(event) => setAddress(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            navigate(address);
                        }
                    }}
                    style={{
                        borderRadius: 999,
                        background: "transparent",
                        fontSize: "var(--text-xs)",
                    }}
                />
                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => navigate(address)} title="Go">Go</button>
                {showFullscreen && toggleFullscreen ? (
                    <button
                        type="button"
                        className="zorai-browser-panel__icon"
                        onClick={toggleFullscreen}
                        title={fullscreen ? "Exit full screen" : "Full screen"}
                        aria-label={fullscreen ? "Exit full screen" : "Full screen"}
                        aria-pressed={fullscreen}
                    >
                        <PanelFullscreenIcon active={fullscreen} />
                    </button>
                ) : null}
            </div>

            <div
                style={{
                    display: "flex",
                    alignItems: "center",
                    flexShrink: 0,
                    padding: "4px 10px",
                    borderBottom: "1px solid var(--zorai-border)",
                    color: "var(--zorai-muted)",
                    fontFamily: "var(--font-ui)",
                    fontSize: "var(--text-xs)",
                    whiteSpace: "nowrap",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                }}
            >
                {pageTitle || "Browser"}
            </div>
        </>
    );
}
