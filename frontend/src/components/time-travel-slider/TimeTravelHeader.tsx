import { RefreshButton } from "@/zorai/shell/RefreshButton";
import { closeBtnStyle } from "./shared";

export function TimeTravelHeader({
    snapshotCount,
    onRefresh,
    toggle,
}: {
    snapshotCount: number;
    onRefresh: () => void;
    toggle: () => void;
}) {
    return (
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
            <div style={{ display: "grid", gap: 2 }}>
                <span className="zorai-panel-title" style={{ color: "var(--timeline)" }}>Time Travel</span>
                <span style={{ fontSize: "var(--text-base)", fontWeight: 700 }}>Filesystem Checkpoints</span>
            </div>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
                    {snapshotCount} snapshot{snapshotCount !== 1 ? "s" : ""}
                </span>
                <RefreshButton onClick={onRefresh} label="Refresh snapshots" />
                <button onClick={toggle} style={closeBtnStyle} title="Close (Esc)">
                    ✕
                </button>
            </div>
        </div>
    );
}
