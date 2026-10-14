import type { TranscriptEntry } from "../../lib/types";
import { SessionVaultMemoryView } from "./SessionVaultMemoryView";
import { StaticLog } from "../StaticLog";
import {
    activeModeBtnStyle,
    formatBytes,
    hdrBtn,
    type MemoryProvenanceReport,
    isTranscriptEntry,
    miniActionBtnStyle,
    modeBtnStyle,
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
            <div style={{ padding: "12px 14px", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", background: "color-mix(in srgb, var(--text-primary) 2%, transparent)", display: "grid", gap: 12 }}>
                <div style={{ display: "flex", justifyContent: "space-between", gap: 12, alignItems: "center" }}>
                    <div>
                        <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Time Travel Timeline</div>
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 2 }}>
                            {timelineMode === "memory"
                                ? "Review memory provenance, with emphasis on uncertain facts that may need operator confirmation later."
                                : "Rewind recent commands and transcript checkpoints for the current scope."}
                        </div>
                    </div>
                    <div style={{ display: "flex", gap: 8 }}>
                        <button type="button" onClick={() => setTimelineMode("timeline")} style={{ ...modeBtnStyle, ...(timelineMode === "timeline" ? activeModeBtnStyle : null) }}>Timeline</button>
                        <button type="button" onClick={() => setTimelineMode("transcripts")} style={{ ...modeBtnStyle, ...(timelineMode === "transcripts" ? activeModeBtnStyle : null) }}>Transcripts</button>
                        <button type="button" onClick={() => { setTimelineMode("memory"); void loadMemoryProvenance(); }} style={{ ...modeBtnStyle, ...(timelineMode === "memory" ? activeModeBtnStyle : null) }}>Memory</button>
                    </div>
                </div>

                <div style={{ display: "flex", gap: 10, overflowX: "auto", paddingBottom: 2 }}>
                    {timelineMode === "memory" ? (
                        <div style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>Memory Provenance</div>
                    ) : timeline.length === 0 ? (
                        <div style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>No timeline events in this scope.</div>
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
                                        borderColor: isSelected ? "color-mix(in srgb, var(--info) 36%, transparent)" : "color-mix(in srgb, var(--text-primary) 8%, transparent)",
                                        background: isSelected ? "var(--bg-secondary)" : timelineCardStyle.background,
                                    }}
                                >
                                    <span style={timelineTypeStyle}>checkpoint</span>
                                    <strong style={{ fontSize: 12 }}>{entry.reason}</strong>
                                    <span style={timelineMetaStyle}>{new Date(entry.capturedAt).toLocaleTimeString()}</span>
                                    <span style={timelineBodyStyle}>{entry.preview || entry.filename}</span>
                                </button>
                            );
                        }

                        return (
                            <div key={entry.id} style={timelineCardStyle}>
                                <span style={timelineTypeStyle}>command</span>
                                <strong style={{ fontSize: 12 }}>{entry.exitCode === null ? "running" : entry.exitCode === 0 ? "success" : `exit ${entry.exitCode}`}</strong>
                                <span style={timelineMetaStyle}>{new Date(entry.timestamp).toLocaleTimeString()}</span>
                                <span style={timelineBodyStyle}>{entry.command}</span>
                                <div style={{ display: "flex", gap: 6, marginTop: 8 }}>
                                    <button type="button" style={miniActionBtnStyle} onClick={() => void runTimelineCommand(entry.command, false)}>Type</button>
                                    <button type="button" style={miniActionBtnStyle} onClick={() => void runTimelineCommand(entry.command, true)}>Run</button>
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>

            <div style={{ flex: 1, overflow: "auto" }}>
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
                            color: "var(--text-secondary)",
                            fontSize: "var(--text-sm)",
                        }}
                    >
                        No transcripts captured yet
                    </div>
                ) : timelineMode === "timeline" ? (
                    <div style={{ padding: 18, display: "grid", gap: 10 }}>
                        {timeline.length === 0 ? (
                            <div style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)", textAlign: "center", padding: 24 }}>
                                No timeline events for the current filters.
                            </div>
                        ) : timeline.map((entry) => {
                            if (isTranscriptEntry(entry)) {
                                return (
                                    <div key={entry.id} style={timelineRowStyle}>
                                        <div style={timelineRailStyle} />
                                        <div style={{ flex: 1 }}>
                                            <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "center" }}>
                                                <div>
                                                    <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Checkpoint · {entry.reason}</div>
                                                    <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 4 }}>{entry.filename}</div>
                                                </div>
                                                <button type="button" style={miniActionBtnStyle} onClick={() => setSelectedId(entry.id)}>Inspect</button>
                                            </div>
                                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 8, whiteSpace: "pre-wrap" }}>{entry.preview}</div>
                                        </div>
                                    </div>
                                );
                            }

                            return (
                                <div key={entry.id} style={timelineRowStyle}>
                                    <div style={{ ...timelineRailStyle, background: entry.exitCode === 0 ? "var(--success)" : entry.exitCode === null ? "var(--accent)" : "var(--danger)" }} />
                                    <div style={{ flex: 1 }}>
                                        <div style={{ display: "flex", justifyContent: "space-between", gap: 10, alignItems: "center" }}>
                                            <div>
                                                <div style={{ fontSize: "var(--text-sm)", fontWeight: 700 }}>Command</div>
                                                <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 4 }}>{entry.command}</div>
                                            </div>
                                            <div style={{ display: "flex", gap: 6 }}>
                                                <button type="button" style={miniActionBtnStyle} onClick={() => void runTimelineCommand(entry.command, false)}>Type</button>
                                                <button type="button" style={miniActionBtnStyle} onClick={() => void runTimelineCommand(entry.command, true)}>Run</button>
                                            </div>
                                        </div>
                                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 8 }}>
                                            {new Date(entry.timestamp).toLocaleString()} · {entry.cwd ?? "no cwd"}
                                            {entry.durationMs !== null ? ` · ${entry.durationMs}ms` : ""}
                                        </div>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                ) : (
                    <div style={{ display: "grid", gridTemplateColumns: "420px minmax(0, 1fr)", minHeight: 560 }}>
                        <div style={{ borderRight: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)", overflow: "auto" }}>
                            {display.map((tx) => (
                                <button
                                    key={tx.id}
                                    type="button"
                                    onClick={() => setSelectedId(tx.id)}
                                    style={{
                                        width: "100%",
                                        textAlign: "left",
                                        padding: "12px 16px",
                                        border: 0,
                                        borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)",
                                        background: selected?.id === tx.id ? "color-mix(in srgb, var(--text-primary) 4%, transparent)" : "transparent",
                                        color: "var(--text-primary)",
                                        cursor: "pointer",
                                    }}
                                >
                                    <div style={{ display: "flex", justifyContent: "space-between", gap: 8, marginBottom: 4 }}>
                                        <span style={{ fontWeight: 600, fontSize: 13, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{tx.filename}</span>
                                        <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>{formatBytes(tx.sizeBytes)}</span>
                                    </div>
                                    <div style={{ color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>
                                        {tx.reason}{tx.cwd ? ` · ${tx.cwd}` : ""}
                                    </div>
                                    <div style={{ marginTop: 6, fontSize: "var(--text-sm)", color: "var(--text-secondary)", opacity: 0.9, maxHeight: 64, overflow: "hidden", whiteSpace: "pre-wrap" }}>
                                        {tx.preview}
                                    </div>
                                </button>
                            ))}
                        </div>
                        <div style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
                            {selected ? (
                                <>
                                    <div style={{ display: "flex", justifyContent: "space-between", gap: 10, padding: "12px 16px", borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)" }}>
                                        <div>
                                            <div style={{ fontSize: "var(--text-base)", fontWeight: 600 }}>{selected.filename}</div>
                                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 3 }}>
                                                {new Date(selected.capturedAt).toLocaleString()} · {selected.reason} · {formatBytes(selected.sizeBytes)}
                                            </div>
                                            <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)", marginTop: 3 }}>
                                                {(selected.workspaceId && workspaceLabels.get(selected.workspaceId)) || "No workspace"}
                                                {selected.surfaceId ? ` / ${surfaceLabels.get(selected.surfaceId) ?? selected.surfaceId}` : ""}
                                                {selected.paneId ? ` / ${selected.paneId}` : ""}
                                            </div>
                                        </div>
                                        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                                            <button onClick={() => void sendSelectedToActivePane(false)} style={hdrBtn}>Type</button>
                                            <button onClick={() => void sendSelectedToActivePane(true)} style={hdrBtn}>Run</button>
                                            <button onClick={() => void copySelected()} style={hdrBtn}>Copy All</button>
                                            <button onClick={() => removeTranscript(selected.id)} style={hdrBtn}>Delete</button>
                                            <button onClick={() => void openSelectedFile()} style={hdrBtn}>Open File</button>
                                            <button onClick={() => void revealSelectedFile()} style={hdrBtn}>Open Folder</button>
                                            <button onClick={exportSelected} style={hdrBtn}>Export</button>
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
                <div style={{ display: "grid", gap: 8 }}>
                    <input
                        type="range"
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
                        <div style={{ fontSize: "var(--text-sm)", color: "var(--text-secondary)" }}>
                            Scrub target: {isTranscriptEntry(scrubTarget) ? `${scrubTarget.reason} checkpoint` : scrubTarget.command}
                        </div>
                    ) : null}
                </div>
            ) : null}
        </>
    );
}