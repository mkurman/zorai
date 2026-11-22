import { INTERVAL_OPTIONS, fieldStyle } from "./shared";

const metaStyle = { fontSize: "var(--text-xs)", color: "var(--zorai-muted)", fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" } as const;

export function SystemMonitorControls({
    processQuery,
    setProcessQuery,
    intervalMs,
    setIntervalMs,
    processLimit,
    setProcessLimit,
    visibleProcesses,
    timestampLabel,
    clear,
}: {
    processQuery: string;
    setProcessQuery: (value: string) => void;
    intervalMs: number;
    setIntervalMs: (value: number) => void;
    processLimit: number;
    setProcessLimit: (value: number) => void;
    visibleProcesses: number;
    timestampLabel: string;
    clear: () => void;
}) {
    return (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--zorai-border)", alignItems: "center" }}>
            <input
                type="text"
                value={processQuery}
                onChange={(event) => setProcessQuery(event.target.value)}
                placeholder="Filter processes by pid, name, or command..."
                style={{ ...fieldStyle, flex: "1 1 220px", minWidth: 160 }}
            />
            <select value={intervalMs} onChange={(event) => setIntervalMs(Number(event.target.value))} style={{ ...fieldStyle, flex: "none", minWidth: 120 }}>
                {INTERVAL_OPTIONS.map((value) => (
                    <option key={value} value={value}>{value < 1000 ? `${value} ms` : `${value / 1000} s`}</option>
                ))}
            </select>
            <select value={processLimit} onChange={(event) => setProcessLimit(Number(event.target.value))} style={{ ...fieldStyle, flex: "none", minWidth: 120 }}>
                {[12, 24, 36, 48, 64].map((value) => (
                    <option key={value} value={value}>Top {value} processes</option>
                ))}
            </select>
            <span style={metaStyle}>{visibleProcesses} visible</span>
            <span style={metaStyle}>{timestampLabel}</span>
            <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={clear}>
                Clear
            </button>
        </div>
    );
}
