import type { CSSProperties } from "react";
import type { CommandLogEntry, TranscriptEntry } from "../../lib/types";

export type SessionVaultMode = "timeline" | "transcripts" | "memory";

export type MemoryProvenanceEntry = {
    id: string;
    target: string;
    mode: string;
    source_kind: string;
    content: string;
    fact_keys: string[];
    thread_id: string | null;
    task_id: string | null;
    goal_run_id: string | null;
    created_at: number;
    age_days: number;
    confidence: number;
    status: string;
    relationships: Array<{
        related_entry_id: string;
        relation_type: string;
        fact_key?: string | null;
    }>;
};

export type MemoryProvenanceReport = {
    total_entries: number;
    target_filter?: string | null;
    summary_by_target: Record<string, number>;
    summary_by_source: Record<string, number>;
    summary_by_status: Record<string, number>;
    entries: MemoryProvenanceEntry[];
};

export type SessionVaultFilters = {
    query: string;
    workspaceFilter: string;
    surfaceFilter: string;
    paneFilter: string;
    reasonFilter: string;
    dateFilter: string;
};

export type TimelineEntry = CommandLogEntry | TranscriptEntry;

export function isTranscriptEntry(entry: TimelineEntry): entry is TranscriptEntry {
    return "capturedAt" in entry;
}

export function filterTranscripts(transcripts: TranscriptEntry[], searchResults: TranscriptEntry[], filters: SessionVaultFilters): TranscriptEntry[] {
    const base = filters.query.trim() ? searchResults : transcripts;
    return base.filter((tx) => {
        const matchesWorkspace = filters.workspaceFilter === "all" || tx.workspaceId === filters.workspaceFilter;
        const matchesSurface = filters.surfaceFilter === "all" || tx.surfaceId === filters.surfaceFilter;
        const matchesPane = filters.paneFilter === "all" || tx.paneId === filters.paneFilter;
        const matchesReason = filters.reasonFilter === "all" || tx.reason === filters.reasonFilter;

        let matchesDate = true;
        if (filters.dateFilter !== "all") {
            const ageMs = Date.now() - tx.capturedAt;
            if (filters.dateFilter === "today") {
                matchesDate = new Date(tx.capturedAt).toDateString() === new Date().toDateString();
            } else if (filters.dateFilter === "7d") {
                matchesDate = ageMs <= 7 * 24 * 60 * 60 * 1000;
            } else if (filters.dateFilter === "30d") {
                matchesDate = ageMs <= 30 * 24 * 60 * 60 * 1000;
            }
        }

        return matchesWorkspace && matchesSurface && matchesPane && matchesReason && matchesDate;
    });
}

export function buildTimeline(commandEntries: CommandLogEntry[], display: TranscriptEntry[], filters: SessionVaultFilters): TimelineEntry[] {
    return [...commandEntries, ...display].filter((entry) => {
        const timestamp = isTranscriptEntry(entry) ? entry.capturedAt : entry.timestamp;
        if (filters.workspaceFilter !== "all" && entry.workspaceId !== filters.workspaceFilter) return false;
        if (filters.surfaceFilter !== "all" && entry.surfaceId !== filters.surfaceFilter) return false;
        if (filters.paneFilter !== "all" && entry.paneId !== filters.paneFilter) return false;

        if (filters.dateFilter !== "all") {
            const ageMs = Date.now() - timestamp;
            if (filters.dateFilter === "today" && new Date(timestamp).toDateString() !== new Date().toDateString()) return false;
            if (filters.dateFilter === "7d" && ageMs > 7 * 24 * 60 * 60 * 1000) return false;
            if (filters.dateFilter === "30d" && ageMs > 30 * 24 * 60 * 60 * 1000) return false;
        }

        const lower = filters.query.trim().toLowerCase();
        if (!lower) return true;

        if (isTranscriptEntry(entry)) {
            return entry.filename.toLowerCase().includes(lower)
                || entry.preview.toLowerCase().includes(lower)
                || (entry.cwd ?? "").toLowerCase().includes(lower);
        }

        return entry.command.toLowerCase().includes(lower)
            || (entry.cwd ?? "").toLowerCase().includes(lower);
    }).sort((a, b) => {
        const aTime = isTranscriptEntry(a) ? a.capturedAt : a.timestamp;
        const bTime = isTranscriptEntry(b) ? b.capturedAt : b.timestamp;
        return bTime - aTime;
    }).slice(0, 48);
}

export function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
}

export const hdrBtn: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 13,
    padding: "6px 10px",
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

export const modeBtnStyle: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 4%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-secondary)",
    borderRadius: 0,
    padding: "7px 12px",
    fontSize: 11,
    cursor: "pointer",
};

export const activeModeBtnStyle: CSSProperties = {
    color: "var(--text-primary)",
    borderColor: "color-mix(in srgb, var(--info) 36%, transparent)",
    background: "var(--bg-secondary)",
};

export const timelineCardStyle: CSSProperties = {
    minWidth: 220,
    maxWidth: 260,
    padding: 12,
    display: "grid",
    gap: 6,
    textAlign: "left",
    borderRadius: 0,
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    background: "var(--bg-secondary)",
    color: "var(--text-primary)",
    flexShrink: 0,
};

export const timelineTypeStyle: CSSProperties = {
    fontSize: 10,
    color: "var(--text-secondary)",
    textTransform: "uppercase",
    letterSpacing: "0.08em",
};

export const timelineMetaStyle: CSSProperties = {
    fontSize: 10,
    color: "var(--text-secondary)",
};

export const timelineBodyStyle: CSSProperties = {
    fontSize: 11,
    color: "var(--text-primary)",
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
};

export const miniActionBtnStyle: CSSProperties = {
    background: "color-mix(in srgb, var(--text-primary) 5%, transparent)",
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    color: "var(--text-primary)",
    borderRadius: 0,
    padding: "6px 9px",
    fontSize: 11,
    cursor: "pointer",
};

export const timelineRowStyle: CSSProperties = {
    display: "flex",
    gap: 12,
    padding: 14,
    borderRadius: 0,
    border: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
    background: "color-mix(in srgb, var(--text-primary) 3%, transparent)",
    alignItems: "stretch",
};

export const timelineRailStyle: CSSProperties = {
    width: 4,
    borderRadius: 0,
    background: "var(--warning)",
    flexShrink: 0,
};
