import type { CommandLogEntry } from "../../lib/types";
import { actionBtn } from "./shared";

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
                    color: "var(--text-secondary)",
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
            }}
        >
            <thead>
                <tr
                    style={{
                        color: "var(--text-secondary)",
                        textAlign: "left",
                        borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 8%, transparent)",
                        position: "sticky",
                        top: 0,
                        background: "color-mix(in srgb, var(--shade) 98%, transparent)",
                    }}
                >
                    <th style={{ padding: "8px 12px" }}>Workspace</th>
                    <th style={{ padding: "8px 12px" }}>Surface</th>
                    <th style={{ padding: "8px 16px" }}>Command</th>
                    <th style={{ padding: "8px 12px" }}>Path</th>
                    <th style={{ padding: "8px 12px" }}>Exit</th>
                    <th style={{ padding: "8px 12px" }}>Duration</th>
                    <th style={{ padding: "8px 12px" }}>CWD</th>
                    <th style={{ padding: "8px 12px" }}>Time</th>
                    <th style={{ padding: "8px 12px" }}>Actions</th>
                </tr>
            </thead>
            <tbody>
                {entries.map((entry) => (
                    <tr
                        key={entry.id}
                        style={{ borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 3%, transparent)" }}
                        onMouseEnter={(ev) =>
                            (ev.currentTarget.style.background = "color-mix(in srgb, var(--text-primary) 3%, transparent)")
                        }
                        onMouseLeave={(ev) =>
                            (ev.currentTarget.style.background = "transparent")
                        }
                    >
                        <td style={{ padding: "8px 12px", color: "var(--text-secondary)", whiteSpace: "nowrap" }}>
                            {entry.workspaceId ? (workspaceLabels.get(entry.workspaceId) ?? entry.workspaceId) : "-"}
                        </td>
                        <td style={{ padding: "8px 12px", color: "var(--text-secondary)", whiteSpace: "nowrap" }}>
                            {entry.surfaceId ? (surfaceLabels.get(entry.surfaceId) ?? entry.surfaceId) : "-"}
                        </td>
                        <td
                            style={{
                                padding: "8px 16px",
                                fontFamily: "var(--font-mono)",
                                maxWidth: 520,
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                                whiteSpace: "nowrap",
                            }}
                        >
                            {entry.command}
                        </td>
                        <td style={{ padding: "8px 12px", color: "var(--text-secondary)", whiteSpace: "nowrap" }}>
                            {entry.path ?? "-"}
                        </td>
                        <td
                            style={{
                                padding: "8px 12px",
                                color:
                                    entry.exitCode === 0
                                        ? "var(--success)"
                                        : entry.exitCode !== null
                                            ? "var(--danger)"
                                            : "var(--text-secondary)",
                            }}
                        >
                            {entry.exitCode ?? "–"}
                        </td>
                        <td style={{ padding: "8px 12px" }}>
                            {entry.durationMs !== null
                                ? entry.durationMs < 1000
                                    ? `${entry.durationMs}ms`
                                    : `${(entry.durationMs / 1000).toFixed(1)}s`
                                : "–"}
                        </td>
                        <td
                            style={{
                                padding: "8px 12px",
                                maxWidth: 150,
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                                whiteSpace: "nowrap",
                                color: "var(--text-secondary)",
                            }}
                        >
                            {entry.cwd ?? "–"}
                        </td>
                        <td
                            style={{
                                padding: "8px 12px",
                                color: "var(--text-secondary)",
                                whiteSpace: "nowrap",
                            }}
                        >
                            {new Date(entry.timestamp).toLocaleString()}
                        </td>
                        <td style={{ padding: "8px 12px", whiteSpace: "nowrap" }}>
                            <div style={{ display: "flex", gap: 6 }}>
                                <button type="button" style={actionBtn} onClick={() => void copyCommand(entry.command)}>Copy</button>
                                <button type="button" style={actionBtn} onClick={() => void sendToActivePane(entry.command, false)}>Insert</button>
                                <button type="button" style={actionBtn} onClick={() => void sendToActivePane(entry.command, true)}>Run</button>
                                <button type="button" style={actionBtn} onClick={() => removeEntry(entry.id)}>Delete</button>
                            </div>
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>
    );
}