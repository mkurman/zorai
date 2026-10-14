import type { CSSProperties } from "react";
import type { CommandLogEntry } from "../../lib/types";

export type CommandLogFilters = {
    query: string;
    workspaceFilter: string;
    surfaceFilter: string;
    paneFilter: string;
    dateFilter: string;
    statusFilter: string;
};

export function filterCommandEntries(entries: CommandLogEntry[], filters: CommandLogFilters): CommandLogEntry[] {
    return entries.filter((entry) => {
        const lower = filters.query.trim().toLowerCase();
        const dateText = new Date(entry.timestamp).toLocaleString().toLowerCase();
        const matchesQuery = !lower
            || entry.command.toLowerCase().includes(lower)
            || (entry.path ?? "").toLowerCase().includes(lower)
            || (entry.cwd ?? "").toLowerCase().includes(lower)
            || dateText.includes(lower);
        const matchesWorkspace = filters.workspaceFilter === "all" || entry.workspaceId === filters.workspaceFilter;
        const matchesSurface = filters.surfaceFilter === "all" || entry.surfaceId === filters.surfaceFilter;
        const matchesPane = filters.paneFilter === "all" || entry.paneId === filters.paneFilter;
        const matchesStatus = filters.statusFilter === "all"
            || (filters.statusFilter === "running" && entry.exitCode === null)
            || (filters.statusFilter === "success" && entry.exitCode === 0)
            || (filters.statusFilter === "failed" && entry.exitCode !== null && entry.exitCode !== 0);

        let matchesDate = true;
        if (filters.dateFilter !== "all") {
            const ageMs = Date.now() - entry.timestamp;
            if (filters.dateFilter === "today") {
                matchesDate = new Date(entry.timestamp).toDateString() === new Date().toDateString();
            } else if (filters.dateFilter === "7d") {
                matchesDate = ageMs <= 7 * 24 * 60 * 60 * 1000;
            } else if (filters.dateFilter === "30d") {
                matchesDate = ageMs <= 30 * 24 * 60 * 60 * 1000;
            }
        }

        return matchesQuery && matchesWorkspace && matchesSurface && matchesPane && matchesStatus && matchesDate;
    });
}

export const hdrBtn: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 13,
    padding: "8px 10px",
    borderRadius: 0,
};

export const filterInputStyle: CSSProperties = {
    width: "100%",
    padding: "10px 12px",
    background: "color-mix(in srgb, var(--bg-surface) 80%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-primary)",
    fontSize: 13,
    fontFamily: "inherit",
    outline: "none",
    borderRadius: 0,
};

export const actionBtn: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    borderRadius: 0,
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 12,
    padding: "6px 10px",
};
