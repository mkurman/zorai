import { hdrBtn } from "./shared";

export function SessionVaultHeader({
    visibleCount,
    totalCount,
    timelineCount,
    scopeLabel,
    captureActivePane,
    clearAll,
    close,
}: {
    visibleCount: number;
    totalCount: number;
    timelineCount: number;
    scopeLabel: string;
    captureActivePane: () => void;
    clearAll: () => void;
    close: () => void;
}) {
    return (
        <div
            style={{
                display: "grid",
                gap: 14,
                padding: "18px 20px 16px",
                borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
            }}
        >
            <div style={{ display: "flex", alignItems: "start", justifyContent: "space-between", gap: 16 }}>
                <div style={{ display: "grid", gap: 6 }}>
                    <span className="zorai-panel-title" style={{ color: "var(--timeline)" }}>Recall Archive</span>
                    <span style={{ fontSize: "var(--text-xl)", fontWeight: 800 }}>Session Vault</span>
                    <span style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", lineHeight: 1.45 }}>
                        Capture transcripts, scrub execution history, and recover terminal state from checkpoints and replayable command timelines.
                    </span>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                    <button onClick={captureActivePane} style={hdrBtn} title="Capture active pane now">
                        Capture
                    </button>
                    <button onClick={clearAll} style={hdrBtn} title="Clear all">
                        Purge
                    </button>
                    <button onClick={close} style={hdrBtn}>
                        ✕
                    </button>
                </div>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 10 }}>
                <MetricCard label="Visible" value={String(visibleCount)} />
                <MetricCard label="Total" value={String(totalCount)} />
                <MetricCard label="Timeline" value={String(timelineCount)} />
                <MetricCard label="Scope" value={scopeLabel} />
            </div>
        </div>
    );
}

function MetricCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ borderRadius: 0, padding: "10px 12px", border: "1px solid color-mix(in srgb, var(--text-primary) 6%, transparent)", background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)", display: "grid", gap: 4 }}>
            <span className="zorai-panel-title">{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 700, wordBreak: "break-word" }}>{value}</span>
        </div>
    );
}