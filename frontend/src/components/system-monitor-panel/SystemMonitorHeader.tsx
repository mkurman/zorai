import { formatUptime, panelButtonStyle } from "./shared";

export function SystemMonitorHeader({
    hostname,
    platform,
    intervalMs,
    uptimeSeconds,
    clear,
    close,
}: {
    hostname: string;
    platform: string;
    intervalMs: number;
    uptimeSeconds: number | null;
    clear: () => void;
    close: () => void;
}) {
    return (
        <div style={{ display: "grid", gap: 14, padding: "18px 20px 16px", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}>
            <div style={{ display: "flex", alignItems: "start", justifyContent: "space-between", gap: 16 }}>
                <div style={{ display: "grid", gap: 6 }}>
                    <span className="zorai-panel-title" style={{ color: "var(--agent)" }}>Host Telemetry</span>
                    <span style={{ fontSize: "var(--text-lg)", fontWeight: 800 }}>System Monitor</span>
                    <span style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", lineHeight: 1.45 }}>
                        Live CPU, memory, GPU, and process telemetry with adjustable refresh cadence and per-process inspection.
                    </span>
                </div>
                <div style={{ display: "flex", gap: 8 }}>
                    <button type="button" style={panelButtonStyle} onClick={clear}>
                        Clear
                    </button>
                    <button type="button" style={panelButtonStyle} onClick={close}>
                        ✕
                    </button>
                </div>
            </div>

            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, minmax(0, 1fr))", gap: 10 }}>
                <MetricCard label="Host" value={hostname} />
                <MetricCard label="Platform" value={platform} />
                <MetricCard label="Refresh" value={`every ${intervalMs / 1000}s`} />
                <MetricCard label="Uptime" value={uptimeSeconds === null ? "pending" : formatUptime(uptimeSeconds)} />
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
