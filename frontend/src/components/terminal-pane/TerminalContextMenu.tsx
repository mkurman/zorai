import { createPortal } from "react-dom";
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

    return createPortal(
        <div
            className="zorai-code-context-menu"
            onPointerDown={(event) => event.stopPropagation()}
            style={{ top: y, left: x }}
        >
            <div className="zorai-code-toolbar-menu__list" role="menu" aria-label="Terminal actions">
                {items.map((item, index) => {
                    if (item.separator) return null;
                    const separatorBefore = index > 0 && Boolean(items[index - 1]?.separator);
                    return (
                        <button
                            key={item.label}
                            type="button"
                            role="menuitem"
                            disabled={item.disabled}
                            className={[item.danger ? "is-danger" : "", separatorBefore ? "has-separator" : ""].filter(Boolean).join(" ") || undefined}
                            onClick={() => {
                                hideContextMenu();
                                item.action?.();
                            }}
                        >
                            <span className="zorai-code-toolbar-menu__label">{item.label}</span>
                            {item.shortcut ? <kbd>{item.shortcut}</kbd> : null}
                        </button>
                    );
                })}
            </div>
        </div>,
        document.body,
    );
}
