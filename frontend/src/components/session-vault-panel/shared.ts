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
    maxWidth: 180,
};

export const activeModeBtnStyle: CSSProperties = {
    background: "var(--zorai-bg-active)",
    borderColor: "var(--zorai-accent)",
};

export const timelineCardStyle: CSSProperties = {
    minWidth: 200,
    maxWidth: 240,
    padding: "10px 12px",
    display: "grid",
    gap: 4,
    alignContent: "start",
    textAlign: "left",
    borderRadius: 10,
    border: "1px solid var(--zorai-border)",
    background: "var(--zorai-bg-panel)",
    color: "var(--zorai-text)",
    fontFamily: "inherit",
    flexShrink: 0,
};

export const timelineTypeStyle: CSSProperties = {
    fontSize: "var(--text-xs)",
    color: "var(--zorai-muted)",
};

export const timelineMetaStyle: CSSProperties = {
    fontSize: "var(--text-xs)",
    color: "var(--zorai-muted)",
    fontVariantNumeric: "tabular-nums",
};

export const timelineBodyStyle: CSSProperties = {
    fontFamily: "var(--font-mono)",
    fontSize: "var(--text-xs)",
    lineHeight: 1.5,
    color: "var(--zorai-text)",
    whiteSpace: "pre-wrap",
    wordBreak: "break-word",
    maxHeight: 54,
    overflow: "hidden",
};

export const timelineRowStyle: CSSProperties = {
    display: "flex",
    gap: 10,
    padding: "10px 12px",
    borderRadius: 10,
    border: "1px solid var(--zorai-border)",
    background: "var(--zorai-bg-panel)",
    alignItems: "stretch",
};

export const timelineRailStyle: CSSProperties = {
    width: 3,
    borderRadius: 2,
    background: "var(--warning)",
    flexShrink: 0,
};
