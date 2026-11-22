import type { CSSProperties } from "react";
import type { CommandLogEntry } from "../../lib/types";

const headerCellStyle: CSSProperties = {
    padding: "6px 10px",
    fontSize: "var(--text-xs)",
    fontWeight: 500,
    color: "var(--zorai-muted)",
    textAlign: "left",
    whiteSpace: "nowrap",
    position: "sticky",
    top: 0,
    zIndex: 1,
    background: "var(--zorai-bg-panel)",
    boxShadow: "inset 0 -1px 0 var(--zorai-border)",
};

const cellStyle: CSSProperties = {
    padding: "6px 10px",
    fontSize: "var(--text-sm)",
    whiteSpace: "nowrap",
};

const mutedCellStyle: CSSProperties = {
    ...cellStyle,
    color: "var(--zorai-muted)",
};

const numericCellStyle: CSSProperties = {
    ...cellStyle,
    textAlign: "right",
    fontVariantNumeric: "tabular-nums",
};

const ellipsisStyle: CSSProperties = {
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
};

export function CommandLogTable({
    entries,
    workspaceLabels,
    surfaceLabels,
    copyCommand,
    sendToActivePane,
    removeEntry,
}: {
    entries: CommandLogEntry[];
    workspaceLabels: Map<string, string>;
    surfaceLabels: Map<string, string>;
    copyCommand: (command: string) => Promise<void>;
    sendToActivePane: (command: string, execute: boolean) => Promise<void>;
    removeEntry: (id: string) => void;
}) {
    if (entries.length === 0) {
        return (
            <div
                style={{
                    padding: 32,
                    textAlign: "center",
                    color: "var(--zorai-muted)",
                    fontSize: "var(--text-sm)",
                }}
            >
                No commands logged
            </div>
        );
    }

    return (
        <table
            style={{
                width: "100%",
                borderCollapse: "collapse",
                fontSize: "var(--text-sm)",
                color: "var(--zorai-text)",
            }}
        >
            <thead>
                <tr>
                    <th style={headerCellStyle}>Workspace</th>
                    <th style={headerCellStyle}>Surface</th>
                    <th style={headerCellStyle}>Command</th>
                    <th style={headerCellStyle}>Path</th>
                    <th style={{ ...headerCellStyle, textAlign: "right" }}>Exit</th>
                    <th style={{ ...headerCellStyle, textAlign: "right" }}>Duration</th>
                    <th style={headerCellStyle}>CWD</th>
                    <th style={headerCellStyle}>Time</th>
                    <th style={headerCellStyle}>Actions</th>
                </tr>
            </thead>
            <tbody>
                {entries.map((entry) => (
                    <tr
                        key={entry.id}
                        style={{ borderBottom: "1px solid var(--zorai-border)" }}
                        onMouseEnter={(ev) =>
                            (ev.currentTarget.style.background = "var(--zorai-bg-elevated)")
                        }
                        onMouseLeave={(ev) =>
                            (ev.currentTarget.style.background = "transparent")
                        }
                    >
                        <td style={mutedCellStyle}>
                            {entry.workspaceId ? (workspaceLabels.get(entry.workspaceId) ?? entry.workspaceId) : "-"}
                        </td>
                        <td style={mutedCellStyle}>
                            {entry.surfaceId ? (surfaceLabels.get(entry.surfaceId) ?? entry.surfaceId) : "-"}
                        </td>
                        <td style={{ ...cellStyle, maxWidth: 520 }}>
                            <div
                                title={entry.command}
                                style={{
                                    ...ellipsisStyle,
                                    fontFamily: "var(--font-mono)",
                                    fontSize: "var(--text-xs)",
                                    color: "var(--zorai-muted)",
                                }}
                            >
                                {entry.command}
                            </div>
                        </td>
                        <td style={mutedCellStyle}>
                            {entry.path ?? "-"}
                        </td>
                        <td
                            style={{
                                ...numericCellStyle,
                                color:
                                    entry.exitCode === 0
                                        ? "var(--success)"
                                        : entry.exitCode !== null
                                            ? "var(--danger)"
                                            : "var(--zorai-muted)",
                            }}
                        >
                            {entry.exitCode ?? "–"}
                        </td>
                        <td style={numericCellStyle}>
                            {entry.durationMs !== null
                                ? entry.durationMs < 1000
                                    ? `${entry.durationMs}ms`
                                    : `${(entry.durationMs / 1000).toFixed(1)}s`
                                : "–"}
                        </td>
                        <td style={{ ...mutedCellStyle, maxWidth: 150 }}>
                            <div title={entry.cwd ?? undefined} style={ellipsisStyle}>
                                {entry.cwd ?? "–"}
                            </div>
                        </td>
                        <td style={{ ...mutedCellStyle, fontVariantNumeric: "tabular-nums" }}>
                            {new Date(entry.timestamp).toLocaleString()}
                        </td>
                        <td style={cellStyle}>
                            <div style={{ display: "flex", gap: 6 }}>
                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void copyCommand(entry.command)}>Copy</button>
                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void sendToActivePane(entry.command, false)}>Insert</button>
                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => void sendToActivePane(entry.command, true)}>Run</button>
                                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={() => removeEntry(entry.id)}>Delete</button>
                            </div>
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>
    );
}
