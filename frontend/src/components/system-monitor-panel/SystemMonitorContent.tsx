import type { MonitorSnapshot } from "./shared";
import { bodyCellStyle, formatBytes, formatMegabytes, headerCellStyle, percentage } from "./shared";

export function SystemMonitorContent({
    snapshot,
    loading,
    error,
    filteredProcesses,
}: {
    snapshot: MonitorSnapshot | null;
    loading: boolean;
    error: string | null;
    filteredProcesses: MonitorSnapshot["processes"];
}) {
    const memoryUsagePercent = snapshot
        ? percentage(snapshot.memory.usedBytes, snapshot.memory.totalBytes)
        : 0;
    const swapUsagePercent = snapshot && snapshot.memory.swapTotalBytes
        ? percentage(snapshot.memory.swapUsedBytes ?? 0, snapshot.memory.swapTotalBytes)
        : 0;

    return (
        <div style={{ display: "grid", gridTemplateColumns: "minmax(260px, 340px) minmax(0, 1fr)", gap: 12, padding: 12, minHeight: 0 }}>
            <div style={{ overflow: "auto", display: "grid", gap: 10, alignContent: "start", minHeight: 0 }}>
                <ResourceCard
                    title="CPU"
                    value={snapshot ? `${snapshot.cpu.usagePercent.toFixed(1)}%` : loading ? "sampling" : "n/a"}
                    subtitle={snapshot ? `${snapshot.cpu.coreCount} cores` : ""}
                    meterValue={snapshot?.cpu.usagePercent ?? 0}
                    detail={snapshot ? `${snapshot.cpu.model} · load ${snapshot.cpu.loadAverage.join(" / ")}` : error ?? "Waiting for metrics..."}
                />
                <ResourceCard
                    title="Memory"
                    value={snapshot ? `${memoryUsagePercent.toFixed(1)}%` : loading ? "sampling" : "n/a"}
                    subtitle={snapshot ? `${formatBytes(snapshot.memory.usedBytes)} / ${formatBytes(snapshot.memory.totalBytes)}` : ""}
                    meterValue={memoryUsagePercent}
                    detail={snapshot ? `${formatBytes(snapshot.memory.freeBytes)} free` : error ?? "Waiting for metrics..."}
                />
                {snapshot?.memory.swapTotalBytes ? (
                    <ResourceCard
                        title="Swap"
                        value={`${swapUsagePercent.toFixed(1)}%`}
                        subtitle={`${formatBytes(snapshot.memory.swapUsedBytes ?? 0)} / ${formatBytes(snapshot.memory.swapTotalBytes)}`}
                        meterValue={swapUsagePercent}
                        detail={`${formatBytes(snapshot.memory.swapFreeBytes ?? 0)} free`}
                    />
                ) : null}
                {snapshot?.gpus.length ? snapshot.gpus.map((gpu) => (
                    <ResourceCard
                        key={gpu.id}
                        title={gpu.name}
                        value={`${gpu.utilizationPercent.toFixed(0)}%`}
                        subtitle={`${formatMegabytes(gpu.memoryUsedMB)} / ${formatMegabytes(gpu.memoryTotalMB)} VRAM`}
                        meterValue={percentage(gpu.memoryUsedMB, gpu.memoryTotalMB)}
                        detail="GPU utilization and VRAM usage"
                    />
                )) : (
                    <ResourceCard
                        title="GPU"
                        value="n/a"
                        subtitle="No GPU telemetry"
                        meterValue={0}
                        detail="nvidia-smi was not available or no supported discrete GPU was detected."
                    />
                )}
            </div>

            <div style={{ display: "grid", gridTemplateRows: "auto 1fr", gap: 8, minHeight: 0 }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: 12, alignItems: "center" }}>
                    <span className="zorai-section-label">Processes</span>
                    {error ? <span style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>{error}</span> : null}
                </div>
                <div style={{ overflow: "auto", minHeight: 0, border: "1px solid var(--zorai-border)", borderRadius: 10, background: "var(--zorai-bg-panel)" }}>
                    {loading && !snapshot ? (
                        <EmptyState message="Sampling host telemetry..." />
                    ) : filteredProcesses.length === 0 ? (
                        <EmptyState message="No process metrics available." />
                    ) : (
                        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)", color: "var(--zorai-text)" }}>
                            <thead>
                                <tr>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle, textAlign: "right" }}>PID</th>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle }}>Process</th>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle, textAlign: "right" }}>CPU</th>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle, textAlign: "right" }}>Memory</th>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle }}>State</th>
                                    <th style={{ ...headerCellStyle, ...stickyHeaderStyle }}>Command</th>
                                </tr>
                            </thead>
                            <tbody>
                                {filteredProcesses.map((processEntry) => (
                                    <tr
                                        key={`${processEntry.pid}-${processEntry.command}`}
                                        style={{ borderBottom: "1px solid var(--zorai-border)" }}
                                        onMouseEnter={(event) => (event.currentTarget.style.background = "var(--zorai-bg-elevated)")}
                                        onMouseLeave={(event) => (event.currentTarget.style.background = "transparent")}
                                    >
                                        <td style={{ ...bodyCellStyle, ...numericCellStyle }}>{processEntry.pid}</td>
                                        <td style={{ ...bodyCellStyle, whiteSpace: "nowrap" }}>
                                            <div style={{ display: "grid", gap: 2 }}>
                                                <span style={{ fontWeight: 500 }}>{processEntry.name}</span>
                                                <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>{processEntry.command.split(" ")[0]}</span>
                                            </div>
                                        </td>
                                        <td style={{ ...bodyCellStyle, ...numericCellStyle }}>{processEntry.cpuPercent === null ? "n/a" : `${processEntry.cpuPercent.toFixed(1)}%`}</td>
                                        <td style={{ ...bodyCellStyle, ...numericCellStyle }}>{formatBytes(processEntry.memoryBytes)}</td>
                                        <td style={{ ...bodyCellStyle, color: "var(--zorai-muted)" }}>{processEntry.state}</td>
                                        <td style={{ ...bodyCellStyle, maxWidth: 0, width: "40%" }}>
                                            <div title={processEntry.command} style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--zorai-muted)", fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}>
                                                {processEntry.command}
                                            </div>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    )}
                </div>
            </div>
        </div>
    );
}

const stickyHeaderStyle = {
    position: "sticky",
    top: 0,
    zIndex: 1,
    background: "var(--zorai-bg-panel)",
    boxShadow: "inset 0 -1px 0 var(--zorai-border)",
} as const;

const numericCellStyle = {
    textAlign: "right",
    fontVariantNumeric: "tabular-nums",
    whiteSpace: "nowrap",
} as const;

function ResourceCard({ title, value, subtitle, detail, meterValue }: { title: string; value: string; subtitle: string; detail: string; meterValue: number }) {
    return (
        <div style={{ display: "grid", gap: 4, borderRadius: 10, border: "1px solid var(--zorai-border)", background: "var(--zorai-bg-panel)", padding: "10px 12px", minWidth: 0 }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "baseline" }}>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{title}</span>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" }}>{subtitle}</span>
            </div>
            <span style={{ fontSize: "var(--text-base)", fontWeight: 600, color: "var(--zorai-text)", fontVariantNumeric: "tabular-nums" }}>{value}</span>
            <div style={{ height: 4, borderRadius: 999, background: "color-mix(in srgb, var(--zorai-text) 10%, transparent)", overflow: "hidden", margin: "2px 0" }}>
                <div style={{ width: `${Math.max(0, Math.min(100, meterValue))}%`, height: "100%", background: "var(--zorai-accent)", borderRadius: 999 }} />
            </div>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", lineHeight: 1.45 }}>{detail}</span>
        </div>
    );
}

function EmptyState({ message }: { message: string }) {
    return (
        <div style={{ padding: 32, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--zorai-muted)", fontSize: "var(--text-sm)" }}>
            {message}
        </div>
    );
}
