import { useEffect, useRef, useState, type CSSProperties } from "react";

export function SurfaceCreateButton({
    layoutMode,
    createBspTerminal,
    createCanvasSurface,
    createCanvasTerminal,
    createCanvasBrowser,
}: {
    layoutMode: "bsp" | "canvas";
    createBspTerminal: () => void;
    createCanvasSurface: () => void;
    createCanvasTerminal: () => void;
    createCanvasBrowser: () => void;
}) {
    const [menuOpen, setMenuOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement | null>(null);
    const anchorRect = menuRef.current?.getBoundingClientRect();
    const menuWidth = 210;
    const menuLeft = Math.max(8, (anchorRect?.right ?? 8) - menuWidth);
    const menuTop = Math.max(8, (anchorRect?.bottom ?? 8) + 6);

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
        <div ref={menuRef} style={{ position: "relative" }}>
            <button
                onClick={() => setMenuOpen((current) => !current)}
                style={{
                    background: "var(--accent-soft)",
                    border: "1px solid var(--accent-soft)",
                    color: "var(--accent)",
                    cursor: "pointer",
                    fontSize: "var(--text-md)",
                    padding: "0 var(--space-2)",
                    height: 26,
                    borderRadius: "var(--radius-md)",
                    fontWeight: 600,
                    transition: "all var(--transition-fast)",
                }}
                onMouseEnter={(event) => {
                    event.currentTarget.style.background = "color-mix(in srgb, var(--accent) 20%, transparent)";
                    event.currentTarget.style.borderColor = "var(--accent)";
                }}
                onMouseLeave={(event) => {
                    event.currentTarget.style.background = "var(--accent-soft)";
                    event.currentTarget.style.borderColor = "var(--accent-soft)";
                }}
                title="Add Terminal or Canvas"
            >
                +
            </button>

            {menuOpen ? (
                <div
                    style={{
                        position: "fixed",
                        left: menuLeft,
                        top: menuTop,
                        minWidth: menuWidth,
                        border: "1px solid var(--glass-border)",
                        background: "var(--zorai-bg)",
                        borderRadius: "var(--radius-md)",
                        boxShadow: "var(--shadow-sm), 0 12px 28px color-mix(in srgb, var(--shade) 28%, transparent)",
                        zIndex: 2600,
                        overflow: "hidden",
                    }}
                >
                    <button
                        type="button"
                        onClick={() => {
                            createCanvasSurface();
                            setMenuOpen(false);
                        }}
                        style={menuItemStyle}
                    >
                        New Infinite Canvas
                    </button>
                    {layoutMode === "canvas" ? (
                        <>
                            <button
                                type="button"
                                onClick={() => {
                                    createCanvasTerminal();
                                    setMenuOpen(false);
                                }}
                                style={menuItemStyle}
                            >
                                New Canvas Terminal
                            </button>
                            <button
                                type="button"
                                onClick={() => {
                                    createCanvasBrowser();
                                    setMenuOpen(false);
                                }}
                                style={menuItemStyle}
                            >
                                New Canvas Browser
                            </button>
                        </>
                    ) : null}
                    <button
                        type="button"
                        onClick={() => {
                            createBspTerminal();
                            setMenuOpen(false);
                        }}
                        style={menuItemStyle}
                    >
                        {layoutMode === "bsp" ? "New Terminal (BSP Pane)" : "New Terminal Surface (BSP)"}
                    </button>
                </div>
            ) : null}
        </div>
    );
}

const menuItemStyle: CSSProperties = {
    width: "100%",
    border: "none",
    borderBottom: "1px solid var(--border)",
    background: "transparent",
    color: "var(--text-secondary)",
    textAlign: "left",
    cursor: "pointer",
    padding: "8px 10px",
    fontSize: "var(--text-xs)",
};
