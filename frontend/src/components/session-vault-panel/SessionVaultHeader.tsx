export function SessionVaultHeader({
    visibleCount,
    totalCount,
    timelineCount,
    scopeLabel,
    captureActivePane,
    clearAll,
}: {
    visibleCount: number;
    totalCount: number;
    timelineCount: number;
    scopeLabel: string;
    captureActivePane: () => void;
    clearAll: () => void;
}) {
    return (
        <>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", minWidth: 0 }}>
                {visibleCount}/{totalCount} transcripts · {timelineCount} events · {scopeLabel}
            </span>
            <span style={{ display: "flex", gap: 8, marginLeft: "auto" }}>
                <button type="button" onClick={captureActivePane} className="zorai-primary-button" title="Capture active pane now">
                    Capture
                </button>
                <button type="button" onClick={clearAll} className="zorai-ghost-button zorai-ghost-button--compact" title="Clear all">
                    Purge
                </button>
            </span>
        </>
    );
}
