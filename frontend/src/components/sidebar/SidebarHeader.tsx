import { useEffect, useRef, useState } from "react";

export function SidebarHeader({
    workspacesCount,
    approvalsCount,
    reasoningCount,
    createWorkspace,
    query,
    setQuery,
}: {
    workspacesCount: number;
    approvalsCount: number;
    reasoningCount: number;
    createWorkspace: (layoutMode?: "bsp" | "canvas") => void;
    query: string;
    setQuery: (value: string) => void;
}) {
    const [menuOpen, setMenuOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!menuOpen) return;
        const onPointerDown = (event: MouseEvent) => {
            if (!menuRef.current?.contains(event.target as Node)) {
                setMenuOpen(false);
            }
        };
        window.addEventListener("mousedown", onPointerDown);
        return () => window.removeEventListener("mousedown", onPointerDown);
    }, [menuOpen]);

    return (
        <>
            <div
                style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: "var(--space-3)",
                    padding: "var(--space-4)",
                    borderBottom: "1px solid var(--border)",
                }}
            >
                <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: "var(--space-3)" }}>
                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
                        <span className="zorai-panel-title">Runtime Environments</span>
                        <div style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>Workspace Fleet</div>
                        <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)", lineHeight: 1.5 }}>
                            Grouped environments for code, approvals, and telemetry
                        </div>
                    </div>

                    <div ref={menuRef} style={{ position: "relative" }}>
                        <button
                            onClick={() => setMenuOpen((current) => !current)}
                            style={createButtonStyle}
                            onMouseEnter={(e) => {
                                e.currentTarget.style.background = "color-mix(in srgb, var(--accent) 20%, transparent)";
                                e.currentTarget.style.borderColor = "var(--accent)";
                            }}
                            onMouseLeave={(e) => {
                                e.currentTarget.style.background = "var(--accent-soft)";
                                e.currentTarget.style.borderColor = "var(--accent-soft)";
                            }}
                            title="New workspace"
                        >
                            +
                        </button>

                        {menuOpen ? (
                            <div
                                style={{
                                    position: "absolute",
                                    top: "calc(100% + 6px)",
                                    right: 0,
                                    minWidth: 190,
                                    border: "1px solid var(--glass-border)",
                                    borderRadius: "var(--radius-md)",
                                    background: "var(--zorai-bg)",
                                    boxShadow: "var(--shadow-sm)",
                                    padding: 4,
                                    display: "grid",
                                    gap: 2,
                                    zIndex: 2000,
                                }}
                            >
                                <button
                                    type="button"
                                    onClick={() => {
                                        createWorkspace("bsp");
                                        setMenuOpen(false);
                                    }}
                                    style={menuItemStyle}
                                >
                                    New Workspace (BSP)
                                </button>
                                <button
                                    type="button"
                                    onClick={() => {
                                        createWorkspace("canvas");
                                        setMenuOpen(false);
                                    }}
                                    style={menuItemStyle}
                                >
                                    New Workspace (Infinite Canvas)
                                </button>
                            </div>
                        ) : null}
                    </div>
                </div>

                <div style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: "var(--space-2)" }}>
                    <SidebarMetric label="Workspaces" value={String(workspacesCount)} accent="var(--mission)" />
                    <SidebarMetric label="Approvals" value={String(approvalsCount)} accent="var(--approval)" />
                    <SidebarMetric label="Reasoning" value={String(reasoningCount)} accent="var(--reasoning)" />
                </div>
            </div>

            <div style={{ padding: "var(--space-3) var(--space-3) 0" }}>
                <input
                    type="text"
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder="Search workspaces..."
                    style={searchInputStyle}
                />
            </div>
        </>
    );
}

function SidebarMetric({ label, value, accent }: { label: string; value: string; accent: string }) {
    return (
        <div
            style={{
                padding: "var(--space-2)",
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                display: "flex",
                flexDirection: "column",
                gap: "var(--space-1)",
            }}
        >
            <span className="zorai-panel-title">{label}</span>
            <span style={{ color: accent, fontWeight: 700, fontSize: "var(--text-md)" }}>{value}</span>
        </div>
    );
}

const createButtonStyle: React.CSSProperties = {
    background: "var(--accent-soft)",
    border: "1px solid var(--accent-soft)",
    color: "var(--accent)",
    cursor: "pointer",
    fontSize: "var(--text-lg)",
    lineHeight: 1,
    padding: "var(--space-1) var(--space-2)",
    borderRadius: "var(--radius-md)",
    fontWeight: 600,
    transition: "all var(--transition-fast)",
};

const searchInputStyle: React.CSSProperties = {
    width: "100%",
    background: "var(--bg-secondary)",
    border: "1px solid var(--glass-border)",
    color: "var(--text-primary)",
    borderRadius: "var(--radius-md)",
    padding: "var(--space-2) var(--space-3)",
    fontSize: "var(--text-sm)",
    outline: "none",
};

const menuItemStyle: React.CSSProperties = {
    border: "none",
    background: "transparent",
    color: "var(--text-secondary)",
    cursor: "pointer",
    textAlign: "left",
    fontSize: "var(--text-xs)",
    borderRadius: "var(--radius-sm)",
    padding: "6px 8px",
};
