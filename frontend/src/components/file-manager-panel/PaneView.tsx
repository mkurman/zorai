import { useState } from "react";
import { formatBytes, getParentPath, inputStyle, type FsEntry, type PaneState } from "./shared";

export function PaneView({
    title,
    pane,
    active,
    inputPath,
    onPathInputChange,
    onGo,
    onSelect,
    onOpen,
    onParent,
}: {
    title: string;
    pane: PaneState;
    active: boolean;
    inputPath: string;
    onPathInputChange: (value: string) => void;
    onGo: () => void;
    onSelect: (path: string | null) => void;
    onOpen: (entry: FsEntry) => void;
    onParent: () => void;
}) {
    const parentPath = getParentPath(pane.path);
    const [hoveredPath, setHoveredPath] = useState<string | null>(null);

    return (
        <div
            style={{
                minWidth: 0,
                minHeight: 0,
                border: "1px solid",
                borderColor: active ? "var(--zorai-accent)" : "var(--zorai-border)",
                borderRadius: 10,
                overflow: "hidden",
                background: "var(--zorai-bg-panel)",
                display: "flex",
                flexDirection: "column",
            }}
            onClick={() => onSelect(pane.selectedPath)}
        >
            <div style={{ padding: "8px 10px", borderBottom: "1px solid var(--zorai-border)", display: "flex", gap: 8, alignItems: "center" }}>
                <span className="zorai-section-label">{title}</span>
                <input
                    value={inputPath}
                    onChange={(event) => onPathInputChange(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            event.preventDefault();
                            onGo();
                        }
                    }}
                    style={{ ...inputStyle, flex: 1, minWidth: 0, fontFamily: "var(--font-mono)", fontSize: "var(--text-xs)" }}
                />
                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={onGo}>Go</button>
                <button type="button" className="zorai-ghost-button zorai-ghost-button--compact" onClick={onParent} disabled={!parentPath}>Up</button>
                {pane.loading ? <span style={{ color: "var(--zorai-muted)", fontSize: "var(--text-xs)" }}>Loading…</span> : null}
            </div>

            <div style={{ flex: 1, overflowY: "auto", minHeight: 0 }}>
                {pane.error ? (
                    <div style={{ color: "var(--danger)", padding: "4px 10px", fontSize: "var(--text-xs)" }}>
                        {pane.error}
                    </div>
                ) : null}

                <table style={{ width: "100%", borderCollapse: "collapse", tableLayout: "fixed" }}>
                    <thead>
                        <tr>
                            <th style={headerCellStyle}>Name</th>
                            <th style={{ ...headerCellStyle, ...metaColumnStyle, width: 84 }}>Size</th>
                            <th style={{ ...headerCellStyle, ...metaColumnStyle, width: 150 }}>Modified</th>
                        </tr>
                    </thead>
                    <tbody>
                        {parentPath ? (
                            <tr
                                onClick={() => onSelect(parentPath)}
                                onDoubleClick={onParent}
                                onMouseEnter={() => setHoveredPath(parentPath)}
                                onMouseLeave={() => setHoveredPath(null)}
                                style={rowStyle(pane.selectedPath === parentPath, hoveredPath === parentPath)}
                            >
                                <td style={cellStyle}>..</td>
                                <td style={{ ...cellStyle, ...metaColumnStyle }}>-</td>
                                <td style={{ ...cellStyle, ...metaColumnStyle }}>parent</td>
                            </tr>
                        ) : null}

                        {pane.entries.map((entry) => (
                            <tr
                                key={entry.path}
                                onClick={() => onSelect(entry.path)}
                                onDoubleClick={() => onOpen(entry)}
                                onMouseEnter={() => setHoveredPath(entry.path)}
                                onMouseLeave={() => setHoveredPath(null)}
                                style={rowStyle(pane.selectedPath === entry.path, hoveredPath === entry.path)}
                            >
                                <td style={cellStyle} title={entry.path}>
                                    {entry.isDirectory ? "[DIR] " : ""}
                                    {entry.name}
                                </td>
                                <td style={{ ...cellStyle, ...metaColumnStyle }}>{entry.isDirectory ? "-" : formatBytes(entry.sizeBytes)}</td>
                                <td style={{ ...cellStyle, ...metaColumnStyle }}>
                                    {entry.modifiedAt ? new Date(entry.modifiedAt).toLocaleString() : "-"}
                                </td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

const headerCellStyle: React.CSSProperties = {
    position: "sticky",
    top: 0,
    zIndex: 1,
    textAlign: "left",
    padding: "4px 10px",
    fontSize: "var(--text-xs)",
    fontWeight: 500,
    color: "var(--zorai-muted)",
    background: "var(--zorai-bg-panel)",
    borderBottom: "1px solid var(--zorai-border)",
};

const cellStyle: React.CSSProperties = {
    height: 28,
    padding: "4px 10px",
    fontSize: "var(--text-sm)",
    color: "var(--zorai-text)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
};

const metaColumnStyle: React.CSSProperties = {
    textAlign: "right",
    fontSize: "var(--text-xs)",
    color: "var(--zorai-muted)",
    fontVariantNumeric: "tabular-nums",
};

function rowStyle(selected: boolean, hovered: boolean): React.CSSProperties {
    return {
        cursor: "pointer",
        background: selected ? "var(--zorai-bg-active)" : hovered ? "var(--zorai-bg-elevated)" : "transparent",
    };
}