import { actionBtnStyle, chipStyle, type SnapshotEntry } from "./shared";

export function TimeTravelContent({
    snapshots,
    selectedIndex,
    setSelectedIndex,
    confirmRestore,
    setConfirmRestore,
    isRestoring,
    handleRestore,
}: {
    snapshots: SnapshotEntry[];
    selectedIndex: number;
    setSelectedIndex: (value: number) => void;
    confirmRestore: boolean;
    setConfirmRestore: (value: boolean) => void;
    isRestoring: boolean;
    handleRestore: () => void;
}) {
    const selected = snapshots[selectedIndex] ?? null;

    if (snapshots.length === 0) {
        return (
            <div style={{ padding: "16px 0", textAlign: "center", color: "var(--text-secondary)", fontSize: "var(--text-sm)" }}>
                No snapshots recorded yet. Snapshots are created before managed command execution.
            </div>
        );
    }

    return (
        <>
            <div style={{ display: "flex", alignItems: "center", gap: 2, padding: "0 4px", marginBottom: 8 }}>
                {snapshots.map((snapshot, index) => (
                    <button
                        key={snapshot.snapshot_id}
                        onClick={() => {
                            setSelectedIndex(index);
                            setConfirmRestore(false);
                        }}
                        title={`${snapshot.label} — ${new Date(snapshot.created_at).toLocaleTimeString()}`}
                        style={{
                            width: index === selectedIndex ? 12 : 8,
                            height: index === selectedIndex ? 12 : 8,
                            borderRadius: "50%",
                            border: "none",
                            cursor: "pointer",
                            flexShrink: 0,
                            transition: "all 0.15s ease",
                            background: index === selectedIndex
                                ? "var(--timeline)"
                                : snapshot.status === "ready"
                                    ? "var(--accent)"
                                    : "var(--text-secondary)",
                            boxShadow: "none",
                        }}
                    />
                ))}
                <div style={{ flex: 1, height: 2, background: "var(--glass-border)", margin: "0 4px" }} />
            </div>

            <input
                type="range"
                min={0}
                max={Math.max(0, snapshots.length - 1)}
                value={selectedIndex}
                onChange={(event) => {
                    setSelectedIndex(Number(event.target.value));
                    setConfirmRestore(false);
                }}
                style={{ width: "100%", accentColor: "var(--timeline)", marginBottom: 10 }}
            />

            {selected ? (
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-end", gap: 12 }}>
                    <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}>{selected.label}</div>
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)", marginTop: 2 }}>
                            {new Date(selected.created_at).toLocaleString()}
                            {selected.command ? (
                                <span style={{ marginLeft: 8, fontFamily: "var(--font-mono)", opacity: 0.8 }}>
                                    {selected.command.length > 60 ? `${selected.command.slice(0, 60)}...` : selected.command}
                                </span>
                            ) : null}
                        </div>
                        <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
                            <span
                                style={{
                                    ...chipStyle,
                                    color: selected.status === "ready" ? "var(--success)" : "var(--warning)",
                                    borderColor: selected.status === "ready" ? "color-mix(in srgb, var(--success) 30%, transparent)" : "color-mix(in srgb, var(--warning) 30%, transparent)",
                                }}
                            >
                                {selected.status}
                            </span>
                            <span style={{ ...chipStyle, color: "var(--text-secondary)", borderColor: "color-mix(in srgb, var(--text-primary) 8%, transparent)" }}>
                                {selectedIndex + 1} / {snapshots.length}
                            </span>
                        </div>
                    </div>
                    <div style={{ display: "flex", gap: 8 }}>
                        {confirmRestore ? (
                            <>
                                <span style={{ fontSize: "var(--text-xs)", color: "var(--warning)", alignSelf: "center" }}>
                                    Overwrite workspace?
                                </span>
                                <button onClick={handleRestore} style={{ ...actionBtnStyle, background: "color-mix(in srgb, var(--danger) 18%, transparent)", borderColor: "color-mix(in srgb, var(--danger) 40%, transparent)", color: "var(--danger)" }}>
                                    {isRestoring ? "Restoring..." : "Confirm"}
                                </button>
                                <button onClick={() => setConfirmRestore(false)} style={actionBtnStyle}>
                                    Cancel
                                </button>
                            </>
                        ) : (
                            <button
                                onClick={handleRestore}
                                disabled={isRestoring || selected.status !== "ready"}
                                style={{
                                    ...actionBtnStyle,
                                    opacity: selected.status !== "ready" ? 0.5 : 1,
                                    cursor: selected.status !== "ready" ? "not-allowed" : "pointer",
                                }}
                            >
                                Restore
                            </button>
                        )}
                    </div>
                </div>
            ) : null}
        </>
    );
}
