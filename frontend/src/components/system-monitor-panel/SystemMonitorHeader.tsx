import { formatUptime } from "./shared";

export function SystemMonitorHeader({
    hostname,
    platform,
    intervalMs,
    uptimeSeconds,
}: {
    hostname: string;
    platform: string;
    intervalMs: number;
    uptimeSeconds: number | null;
}) {
    return (
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 10, padding: "12px 12px 0" }}>
            <MetricCard label="Host" value={hostname} />
            <MetricCard label="Platform" value={platform} />
            <MetricCard label="Refresh" value={`every ${intervalMs / 1000}s`} />
            <MetricCard label="Uptime" value={uptimeSeconds === null ? "pending" : formatUptime(uptimeSeconds)} />
        </div>
    );
}

function MetricCard({ label, value }: { label: string; value: string }) {
    return (
        <div style={{ borderRadius: 10, padding: "10px 12px", border: "1px solid var(--zorai-border)", background: "var(--zorai-bg-panel)", display: "grid", gap: 4, minWidth: 0 }}>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>{label}</span>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 600, color: "var(--zorai-text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={value}>{value}</span>
        </div>
    );
}
