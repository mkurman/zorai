import { INTERVAL_OPTIONS, fieldStyle } from "./shared";

export function SystemMonitorControls({
    processQuery,
    setProcessQuery,
    intervalMs,
    setIntervalMs,
    processLimit,
    setProcessLimit,
    visibleProcesses,
    timestampLabel,
}: {
    processQuery: string;
    setProcessQuery: (value: string) => void;
    intervalMs: number;
    setIntervalMs: (value: number) => void;
    processLimit: number;
    setProcessLimit: (value: number) => void;
    visibleProcesses: number;
    timestampLabel: string;
}) {
    return (
        <div style={{ display: "grid", gridTemplateColumns: "1.2fr 1.2fr 1fr auto auto", gap: 10, padding: 14, borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", background: "color-mix(in srgb, var(--text-primary) 2%, transparent)", alignItems: "center" }}>
            <input
                type="text"
                value={processQuery}
                onChange={(event) => setProcessQuery(event.target.value)}
                placeholder="Filter processes by pid, name, or command..."
                style={fieldStyle}
            />
            <select value={intervalMs} onChange={(event) => setIntervalMs(Number(event.target.value))} style={fieldStyle}>
                {INTERVAL_OPTIONS.map((value) => (
                    <option key={value} value={value}>{value < 1000 ? `${value} ms` : `${value / 1000} s`}</option>
                ))}
            </select>
            <select value={processLimit} onChange={(event) => setProcessLimit(Number(event.target.value))} style={fieldStyle}>
                {[12, 24, 36, 48, 64].map((value) => (
                    <option key={value} value={value}>Top {value} processes</option>
                ))}
            </select>
            <span className="zorai-chip">{visibleProcesses} visible</span>
            <span className="zorai-chip">{timestampLabel}</span>
        </div>
    );
}
