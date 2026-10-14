export interface TerminalContextMenuItem {
    label?: string;
    shortcut?: string;
    disabled?: boolean;
    danger?: boolean;
    separator?: boolean;
    action?: () => void;
}

export function TerminalContextMenu({
    visible,
    x,
    y,
    items,
    hideContextMenu,
}: {
    visible: boolean;
    x: number;
    y: number;
    items: TerminalContextMenuItem[];
    hideContextMenu: () => void;
}) {
    if (!visible) {
        return null;
    }

    return (
        <div
            onPointerDown={(event) => event.stopPropagation()}
            style={{
                position: "absolute",
                top: y,
                left: x,
                width: 220,
                background: "var(--bg-secondary)",
                border: "1px solid var(--border)",
                boxShadow: "var(--shadow-sm)",
                borderRadius: 0,
                padding: 6,
                zIndex: 4000,
            }}
        >
            {items.map((item, index) => {
                if (item.separator) {
                    return (
                        <div
                            key={`separator-${index}`}
                            style={{
                                height: 1,
                                margin: "6px 4px",
                                background: "var(--bg-secondary)",
                            }}
                        />
                    );
                }

                return (
                    <button
                        key={item.label}
                        type="button"
                        disabled={item.disabled}
                        onClick={() => {
                            hideContextMenu();
                            item.action?.();
                        }}
                        style={{
                            width: "100%",
                            border: 0,
                            background: "transparent",
                            color: item.danger ? "var(--danger)" : item.disabled ? "var(--text-muted)" : "var(--text-primary)",
                            display: "flex",
                            justifyContent: "space-between",
                            alignItems: "center",
                            borderRadius: 0,
                            padding: "8px 10px",
                            fontSize: "var(--text-xs)",
                            cursor: item.disabled ? "not-allowed" : "pointer",
                        }}
                    >
                        <span>{item.label}</span>
                        <span style={{ color: "var(--text-secondary)", fontSize: "var(--text-xs)" }}>{item.shortcut ?? ""}</span>
                    </button>
                );
            })}
        </div>
    );
}
