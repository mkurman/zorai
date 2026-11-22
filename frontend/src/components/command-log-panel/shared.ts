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

export const filterInputStyle: CSSProperties = {
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
