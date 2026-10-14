import { formatBytes, getParentPath, inputStyle, secondaryButtonStyle, type FsEntry, type PaneState } from "./shared";

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

    return (
        <div
            style={{
                minWidth: 0,
                minHeight: 0,
                border: "1px solid",
                borderColor: active ? "var(--accent)" : "var(--border)",
                background: "var(--bg-secondary)",
                display: "flex",
                flexDirection: "column",
            }}
            onClick={() => onSelect(pane.selectedPath)}
        >
            <div style={{ padding: "var(--space-2)", borderBottom: "1px solid var(--border)", display: "grid", gap: "var(--space-1)" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <strong style={{ fontSize: "var(--text-sm)" }}>{title} Pane</strong>
                    {pane.loading ? <span className="zorai-chip">loading</span> : null}
                </div>

                <div style={{ display: "flex", gap: "var(--space-1)" }}>
                    <input
                        value={inputPath}
                        onChange={(event) => onPathInputChange(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                event.preventDefault();
                                onGo();
                            }
                        }}
                        style={inputStyle}
                    />
                    <button type="button" style={secondaryButtonStyle} onClick={onGo}>Go</button>
                    <button type="button" style={secondaryButtonStyle} onClick={onParent} disabled={!parentPath}>Up</button>
                </div>
            </div>

            <div style={{ flex: 1, overflowY: "auto", minHeight: 0 }}>
                {pane.error ? (
                    <div style={{ color: "var(--danger)", padding: "var(--space-2)", fontSize: "var(--text-xs)" }}>
                        {pane.error}
                    </div>
                ) : null}

                <table style={{ width: "100%", borderCollapse: "collapse", tableLayout: "fixed" }}>
                    <thead>
                        <tr style={{ position: "sticky", top: 0, background: "var(--bg-tertiary)", zIndex: 1 }}>
                            <th style={headerCellStyle}>Name</th>
                            <th style={headerCellStyle}>Size</th>
                            <th style={headerCellStyle}>Modified</th>
                        </tr>
                    </thead>
                    <tbody>
                        {parentPath ? (
                            <tr
                                onClick={() => onSelect(parentPath)}
                                onDoubleClick={onParent}
                                style={rowStyle(pane.selectedPath === parentPath)}
                            >
                                <td style={cellStyle}>..</td>
                                <td style={cellStyle}>-</td>
                                <td style={cellStyle}>parent</td>
                            </tr>
                        ) : null}

                        {pane.entries.map((entry) => (
                            <tr
                                key={entry.path}
                                onClick={() => onSelect(entry.path)}
                                onDoubleClick={() => onOpen(entry)}
                                style={rowStyle(pane.selectedPath === entry.path)}
                            >
                                <td style={cellStyle} title={entry.path}>
                                    {entry.isDirectory ? "[DIR] " : ""}
                                    {entry.name}
                                </td>
                                <td style={cellStyle}>{entry.isDirectory ? "-" : formatBytes(entry.sizeBytes)}</td>
                                <td style={cellStyle}>
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
    textAlign: "left",
    padding: "6px 8px",
    fontSize: "var(--text-xs)",
    color: "var(--text-secondary)",
    borderBottom: "1px solid var(--border)",
};

const cellStyle: React.CSSProperties = {
    padding: "6px 8px",
    fontSize: "var(--text-xs)",
    borderBottom: "1px solid color-mix(in srgb, var(--text-primary) 5%, transparent)",
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
};

function rowStyle(selected: boolean): React.CSSProperties {
    return {
        cursor: "pointer",
        background: selected ? "var(--accent-soft)" : "transparent",
    };
}