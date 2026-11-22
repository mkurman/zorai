import type { TranscriptEntry } from "../../lib/types";
import { SessionVaultMemoryView } from "./SessionVaultMemoryView";
import { StaticLog } from "../StaticLog";
import {
    activeModeBtnStyle,
    formatBytes,
    type MemoryProvenanceReport,
    isTranscriptEntry,
    type SessionVaultMode,
    timelineBodyStyle,
    timelineCardStyle,
    timelineMetaStyle,
    timelineRailStyle,
    timelineRowStyle,
    timelineTypeStyle,
    type TimelineEntry,
} from "./shared";

export function SessionVaultContent({
    timeline,
    timelineMode,
    setTimelineMode,
    memoryReport,
    memoryStatus,
    loadingMemory,
    loadMemoryProvenance,
    confirmMemoryEntry,
    retractMemoryEntry,
    selected,
    setSelectedId,
    display,
    workspaceLabels,
    surfaceLabels,
    runTimelineCommand,
    sendSelectedToActivePane,
    copySelected,
    removeTranscript,
    openSelectedFile,
    revealSelectedFile,
    exportSelected,
    timelineIndex,
    setTimelineIndex,
}: {
    timeline: TimelineEntry[];
    timelineMode: SessionVaultMode;
    setTimelineMode: (mode: SessionVaultMode) => void;
    memoryReport: MemoryProvenanceReport | null;
    memoryStatus: string | null;
    loadingMemory: boolean;
    loadMemoryProvenance: () => Promise<void>;
    confirmMemoryEntry: (entryId: string) => Promise<void>;
    retractMemoryEntry: (entryId: string) => Promise<void>;
    selected: TranscriptEntry | null;
    setSelectedId: (id: string | null) => void;
    display: TranscriptEntry[];
    workspaceLabels: Map<string, string>;
    surfaceLabels: Map<string, string>;
    runTimelineCommand: (command: string, execute: boolean) => Promise<void>;
    sendSelectedToActivePane: (execute?: boolean) => Promise<void>;
    copySelected: () => Promise<void>;
    removeTranscript: (id: string) => void;
    openSelectedFile: () => Promise<void>;
    revealSelectedFile: () => Promise<void>;
    exportSelected: () => void;
    timelineIndex: number;
    setTimelineIndex: (value: number) => void;
}) {
    const scrubTarget = timeline[timelineIndex] ?? timeline[0] ?? null;

    return (
        <>
            <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--zorai-border)", display: "grid", gap: 10 }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                    <div style={{ display: "grid", gap: 2, minWidth: 0 }}>
                        <div className="zorai-section-label">Time Travel Timeline</div>
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)" }}>
                            {timelineMode === "memory"
                                ? "Review memory provenance, with emphasis on uncertain facts that may need operator confirmation later."
                                : "Rewind recent commands and transcript checkpoints for the current scope."}
                        </div>
                    </div>
                    <div style={{ display: "flex", gap: 6 }}>
                        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => setTimelineMode("timeline")} style={timelineMode === "timeline" ? activeModeBtnStyle : undefined}>Timeline</button>
                        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => setTimelineMode("transcripts")} style={timelineMode === "transcripts" ? activeModeBtnStyle : undefined}>Transcripts</button>
                        <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => { setTimelineMode("memory"); void loadMemoryProvenance(); }} style={timelineMode === "memory" ? activeModeBtnStyle : undefined}>Memory</button>
                    </div>
                </div>

                <div style={{ display: "flex", gap: 8, overflowX: "auto", paddingBottom: 2 }}>
                    {timelineMode === "memory" ? (
                        <div style={{ color: "var(--zorai-muted)", fontSize: "var(--text-xs)" }}>Memory Provenance</div>
                    ) : timeline.length === 0 ? (
                        <div style={{ color: "var(--zorai-muted)", fontSize: "var(--text-xs)" }}>No timeline events in this scope.</div>
                    ) : timeline.map((entry) => {
                        if (isTranscriptEntry(entry)) {
                            const isSelected = selected?.id === entry.id;
                            return (
                                <button
                                    key={entry.id}
                                    type="button"
                                    onClick={() => {
                                        setTimelineMode("transcripts");
                                        setSelectedId(entry.id);
                                    }}
                                    style={{
                                        ...timelineCardStyle,
                                        cursor: "pointer",
                                        borderColor: isSelected ? "var(--zorai-accent)" : "var(--zorai-border)",
                                        background: isSelected ? "var(--zorai-bg-active)" : timelineCardStyle.background,
                                    }}
                                >
                                    <span style={timelineTypeStyle}>checkpoint</span>
                                    <strong style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{entry.reason}</strong>
                                    <span style={timelineMetaStyle}>{new Date(entry.capturedAt).toLocaleTimeString()}</span>
                                    <span style={timelineBodyStyle}>{entry.preview || entry.filename}</span>
                                </button>
                            );
                        }

                        return (
                            <div key={entry.id} style={timelineCardStyle}>
                                <span style={timelineTypeStyle}>command</span>
                                <strong style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{entry.exitCode === null ? "running" : entry.exitCode === 0 ? "success" : `exit ${entry.exitCode}`}</strong>
                                <span style={timelineMetaStyle}>{new Date(entry.timestamp).toLocaleTimeString()}</span>
                                <span style={timelineBodyStyle}>{entry.command}</span>
                                <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
                                    <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void runTimelineCommand(entry.command, false)}>Type</button>
                                    <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void runTimelineCommand(entry.command, true)}>Run</button>
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>

            <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
                {timelineMode === "memory" ? (
                    <SessionVaultMemoryView
                        report={memoryReport}
                        status={memoryStatus}
                        loading={loadingMemory}
                        confirmMemoryEntry={confirmMemoryEntry}
                        retractMemoryEntry={retractMemoryEntry}
                    />
                ) : timelineMode === "transcripts" && display.length === 0 ? (
                    <div
                        style={{
                            padding: 32,
                            textAlign: "center",
                            color: "var(--zorai-muted)",
                            fontSize: "var(--text-sm)",
                        }}
                    >
                        No transcripts captured yet
                    </div>
                ) : timelineMode === "timeline" ? (
                    <div style={{ padding: 12, display: "grid", gap: 8 }}>
                        {timeline.length === 0 ? (
                            <div style={{ color: "var(--zorai-muted)", fontSize: "var(--text-sm)", textAlign: "center", padding: 24 }}>
                                No timeline events for the current filters.
                            </div>
                        ) : timeline.map((entry) => {
                            if (isTranscriptEntry(entry)) {
                                return (
                                    <div key={entry.id} style={timelineRowStyle}>
                                        <div style={timelineRailStyle} />
                                        <div style={{ flex: 1, minWidth: 0 }}>
                                            <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "center" }}>
                                                <div style={{ minWidth: 0 }}>
                                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>Checkpoint · {entry.reason}</div>
                                                    <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{entry.filename}</div>
                                                </div>
                                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => setSelectedId(entry.id)}>Inspect</button>
                                            </div>
                                            <div style={{ ...timelineBodyStyle, color: "var(--zorai-muted)", marginTop: 6, maxHeight: "none" }}>{entry.preview}</div>
                                        </div>
                                    </div>
                                );
                            }

                            return (
                                <div key={entry.id} style={timelineRowStyle}>
                                    <div style={{ ...timelineRailStyle, background: entry.exitCode === 0 ? "var(--success)" : entry.exitCode === null ? "var(--accent)" : "var(--danger)" }} />
                                    <div style={{ flex: 1, minWidth: 0 }}>
                                        <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "center" }}>
                                            <div style={{ minWidth: 0 }}>
                                                <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>Command</div>
                                                <div style={{ ...timelineBodyStyle, marginTop: 2, maxHeight: "none" }}>{entry.command}</div>
                                            </div>
                                            <div style={{ display: "flex", gap: 6 }}>
                                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void runTimelineCommand(entry.command, false)}>Type</button>
                                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void runTimelineCommand(entry.command, true)}>Run</button>
                                            </div>
                                        </div>
                                        <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", marginTop: 6, fontVariantNumeric: "tabular-nums" }}>
                                            {new Date(entry.timestamp).toLocaleString()} · {entry.cwd ?? "no cwd"}
                                            {entry.durationMs !== null ? ` · ${entry.durationMs}ms` : ""}
                                        </div>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                ) : (
                    <div style={{ display: "grid", gridTemplateColumns: "minmax(220px, 320px) minmax(0, 1fr)", height: "100%", minHeight: 360 }}>
                        <div style={{ borderRight: "1px solid var(--zorai-border)", overflow: "auto", padding: 6, display: "grid", gap: 2, alignContent: "start" }}>
                            {display.map((tx) => (
                                <button
                                    key={tx.id}
                                    type="button"
                                    onClick={() => setSelectedId(tx.id)}
                                    style={{
                                        width: "100%",
                                        minWidth: 0,
                                        textAlign: "left",
                                        padding: "6px 10px",
                                        border: 0,
                                        borderRadius: 6,
                                        background: selected?.id === tx.id ? "var(--zorai-bg-active)" : "transparent",
                                        color: "var(--zorai-text)",
                                        fontFamily: "inherit",
                                        cursor: "pointer",
                                        display: "grid",
                                        gap: 2,
                                    }}
                                >
                                    <span style={{ fontSize: "var(--text-sm)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{tx.filename}</span>
                                    <span style={{ color: "var(--zorai-muted)", fontSize: "var(--text-xs)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontVariantNumeric: "tabular-nums" }}>
                                        {tx.reason} · {formatBytes(tx.sizeBytes)}{tx.cwd ? ` · ${tx.cwd}` : ""}
                                    </span>
                                </button>
                            ))}
                        </div>
                        <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
                            {selected ? (
                                <>
                                    <div style={{ display: "flex", justifyContent: "space-between", gap: 8, padding: "10px 12px", borderBottom: "1px solid var(--zorai-border)", flexWrap: "wrap", alignItems: "center" }}>
                                        <div style={{ minWidth: 0 }}>
                                            <div style={{ fontSize: "var(--text-base)", fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{selected.filename}</div>
                                            <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", marginTop: 2, fontVariantNumeric: "tabular-nums" }}>
                                                {new Date(selected.capturedAt).toLocaleString()} · {selected.reason} · {formatBytes(selected.sizeBytes)}
                                            </div>
                                            <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", marginTop: 2 }}>
                                                {(selected.workspaceId && workspaceLabels.get(selected.workspaceId)) || "No workspace"}
                                                {selected.surfaceId ? ` / ${surfaceLabels.get(selected.surfaceId) ?? selected.surfaceId}` : ""}
                                                {selected.paneId ? ` / ${selected.paneId}` : ""}
                                            </div>
                                        </div>
                                        <div style={{ display: "flex", gap: 6, alignItems: "center", flexWrap: "wrap" }}>
                                            <button onClick={() => void sendSelectedToActivePane(false)} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Type</button>
                                            <button onClick={() => void sendSelectedToActivePane(true)} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Run</button>
                                            <button onClick={() => void copySelected()} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Copy All</button>
                                            <button onClick={() => removeTranscript(selected.id)} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Delete</button>
                                            <button onClick={() => void openSelectedFile()} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Open File</button>
                                            <button onClick={() => void revealSelectedFile()} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Open Folder</button>
                                            <button onClick={exportSelected} type="button" className="zorai-ghost-button zorai-ghost-button--compact">Export</button>
                                        </div>
                                    </div>
                                    <div style={{ flex: 1, minHeight: 0 }}>
                                        <StaticLog content={selected.content} maxHeight="100%" />
                                    </div>
                                </>
                            ) : null}
                        </div>
                    </div>
                )}
            </div>

            {timeline.length > 0 ? (
                <div style={{ display: "grid", gap: 6, padding: "10px 12px", borderTop: "1px solid var(--zorai-border)" }}>
                    <input
                        type="range"
                        style={{ width: "100%", accentColor: "var(--zorai-accent)" }}
                        min={0}
                        max={Math.max(0, timeline.length - 1)}
                        value={Math.min(timelineIndex, Math.max(0, timeline.length - 1))}
                        onChange={(event) => {
                            const nextIndex = Number(event.target.value);
                            setTimelineIndex(nextIndex);
                            const target = timeline[nextIndex];
                            if (target && isTranscriptEntry(target)) {
                                setSelectedId(target.id);
                            }
                        }}
                    />
                    {scrubTarget ? (
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--zorai-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            Scrub target: {isTranscriptEntry(scrubTarget) ? `${scrubTarget.reason} checkpoint` : scrubTarget.command}
                        </div>
                    ) : null}
                </div>
            ) : null}
        </>
    );
}