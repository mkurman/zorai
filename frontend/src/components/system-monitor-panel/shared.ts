import type { CSSProperties } from "react";

export type MonitorSnapshot = {
    timestamp: number;
    platform: string;
    hostname: string;
    uptimeSeconds: number;
    cpu: {
        usagePercent: number;
        coreCount: number;
        model: string;
        loadAverage: number[];
    };
    memory: {
        totalBytes: number;
        usedBytes: number;
        freeBytes: number;
        swapTotalBytes: number | null;
        swapUsedBytes: number | null;
        swapFreeBytes: number | null;
    };
    gpus: Array<{
        id: string;
        name: string;
        memoryUsedMB: number;
        memoryTotalMB: number;
        utilizationPercent: number;
    }>;
    processes: Array<{
        pid: number;
        name: string;
        cpuPercent: number | null;
        memoryBytes: number;
        state: string;
        command: string;
    }>;
};

export const INTERVAL_OPTIONS = [500, 1000, 2000, 5000, 10000];

export function percentage(used: number, total: number) {
    if (!Number.isFinite(total) || total <= 0) return 0;
    return (used / total) * 100;
}

export function formatBytes(bytes: number) {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
    return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

export function formatMegabytes(value: number) {
    if (!Number.isFinite(value) || value <= 0) return "0 MB";
    if (value < 1024) return `${value.toFixed(0)} MB`;
    return `${(value / 1024).toFixed(1)} GB`;
}

export function formatUptime(totalSeconds: number) {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    if (hours >= 24) {
        const days = Math.floor(hours / 24);
        return `${days}d ${hours % 24}h`;
    }
    return `${hours}h ${minutes}m`;
}

export const fieldStyle: CSSProperties = {
    height: 28,
    padding: "4px 8px",
    background: "transparent",
    border: "1px solid var(--zorai-border)",
    color: "var(--zorai-text)",
    fontSize: "var(--text-sm)",
    fontFamily: "inherit",
    outline: "none",
    borderRadius: 6,
};

export const headerCellStyle: CSSProperties = {
    padding: "6px 10px",
    fontSize: "var(--text-xs)",
    fontWeight: 500,
    color: "var(--zorai-muted)",
    textAlign: "left",
    whiteSpace: "nowrap",
};

export const bodyCellStyle: CSSProperties = {
    padding: "6px 10px",
    fontSize: "var(--text-sm)",
    verticalAlign: "middle",
};
