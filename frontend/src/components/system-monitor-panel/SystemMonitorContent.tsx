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
        <div style={{ display: "grid", gridTemplateColumns: "420px minmax(0, 1fr)", minHeight: 0 }}>
            <div style={{ borderRight: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", overflow: "auto", padding: 16, display: "grid", gap: 12, alignContent: "start" }}>
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

            <div style={{ display: "grid", gridTemplateRows: "auto 1fr", minHeight: 0 }}>
                <div style={{ padding: "14px 16px", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", display: "flex", justifyContent: "space-between", gap: 12, alignItems: "center" }}>
                    <div>
                        <div style={{ fontSize: "var(--text-base)", fontWeight: 700 }}>Process Table</div>
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 4 }}>
                            Sorted by CPU usage from the native runtime snapshot.
                        </div>
                    </div>
                    {error ? <span style={{ fontSize: "var(--text-xs)", color: "var(--danger)" }}>{error}</span> : null}
                </div>
                <div style={{ overflow: "auto" }}>
                    {loading && !snapshot ? (
                        <EmptyState message="Sampling host telemetry..." />
                    ) : filteredProcesses.length === 0 ? (
                        <EmptyState message="No process metrics available." />
                    ) : (
                        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "var(--text-sm)" }}>
                            <thead>
                                <tr style={{ position: "sticky", top: 0, background: "color-mix(in srgb, var(--shade) 98%, transparent)", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", textAlign: "left", color: "var(--text-secondary)" }}>
                                    <th style={headerCellStyle}>PID</th>
                                    <th style={headerCellStyle}>Process</th>
                                    <th style={headerCellStyle}>CPU</th>
                                    <th style={headerCellStyle}>Memory</th>
                                    <th style={headerCellStyle}>State</th>
                                    <th style={headerCellStyle}>Command</th>
                                </tr>
                            </thead>
                            <tbody>
                                {filteredProcesses.map((processEntry) => (
                                    <tr key={`${processEntry.pid}-${processEntry.command}`} style={{ borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)" }}>
                                        <td style={bodyCellStyle}>{processEntry.pid}</td>
                                        <td style={bodyCellStyle}>
                                            <div style={{ display: "grid", gap: 4 }}>
                                                <span style={{ fontWeight: 700 }}>{processEntry.name}</span>
                                                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{processEntry.command.split(" ")[0]}</span>
                                            </div>
                                        </td>
                                        <td style={bodyCellStyle}>{processEntry.cpuPercent === null ? "n/a" : `${processEntry.cpuPercent.toFixed(1)}%`}</td>
                                        <td style={bodyCellStyle}>{formatBytes(processEntry.memoryBytes)}</td>
                                        <td style={bodyCellStyle}>{processEntry.state}</td>
                                        <td style={{ ...bodyCellStyle, maxWidth: 0 }}>
                                            <div title={processEntry.command} style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--text-secondary)", fontFamily: "var(--font-mono)" }}>
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

function ResourceCard({ title, value, subtitle, detail, meterValue }: { title: string; value: string; subtitle: string; detail: string; meterValue: number }) {
    return (
        <div style={{ display: "grid", gap: 10, borderRadius: 0, border: "1px solid color-mix(in srgb, var(--text-primary) 6%, transparent)", background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)", padding: 14 }}>
            <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "center" }}>
                <div style={{ display: "grid", gap: 4 }}>
                    <span className="zorai-panel-title">{title}</span>
                    <span style={{ fontSize: "var(--text-lg)", fontWeight: 800 }}>{value}</span>
                </div>
                <span className="zorai-chip">{subtitle}</span>
            </div>
            <div style={{ height: 8, borderRadius: 0, background: "color-mix(in srgb, var(--text-primary) 6%, transparent)", overflow: "hidden" }}>
                <div style={{ width: `${Math.max(0, Math.min(100, meterValue))}%`, height: "100%", background: "var(--bg-secondary)", borderRadius: 0 }} />
            </div>
            <span style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", lineHeight: 1.45 }}>{detail}</span>
        </div>
    );
}

function EmptyState({ message }: { message: string }) {
    return (
        <div style={{ padding: 32, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>
            {message}
        </div>
    );
}
