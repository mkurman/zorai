import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export type ComposerPlusItem = {
  key: string;
  label: string;
  icon?: ReactNode;
  kind: "action" | "panel" | "switch";
  active?: boolean;
  hint?: string;
  disabled?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
};

export function ComposerPlusMenu({ anchor, items, onClose }: { anchor: DOMRect; items: ComposerPlusItem[]; onClose: () => void }) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState({ left: anchor.left, bottom: window.innerHeight - anchor.top + 8 });

  useLayoutEffect(() => {
    const width = rootRef.current?.offsetWidth ?? 240;
    setPosition({ left: Math.min(Math.max(8, anchor.left), window.innerWidth - width - 8), bottom: Math.max(8, window.innerHeight - anchor.top + 8) });
  }, [anchor]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) onClose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return createPortal(
    <div ref={rootRef} className="zorai-plus-menu" role="menu" aria-label="Composer tools" style={{ position: "fixed", left: position.left, bottom: position.bottom, zIndex: 95 }}>
      {items.map((item) => (
        <div key={item.key}>
          {item.separatorBefore ? <div className="zorai-plus-menu__separator" role="separator" /> : null}
          <button
            type="button"
            role={item.kind === "switch" ? "menuitemcheckbox" : "menuitem"}
            aria-checked={item.kind === "switch" ? Boolean(item.active) : undefined}
            className={["zorai-plus-menu__item", item.active ? "is-active" : ""].filter(Boolean).join(" ")}
            disabled={item.disabled}
            title={item.hint}
            onClick={() => {
              item.onSelect();
              if (item.kind !== "switch") onClose();
            }}
          >
            <span className="zorai-plus-menu__icon" aria-hidden="true">{item.icon}</span>
            <span className="zorai-plus-menu__label">{item.label}</span>
            {item.kind === "switch" ? <span className={["zorai-plus-menu__switch", item.active ? "is-on" : ""].filter(Boolean).join(" ")} aria-hidden="true"><span /></span> : null}
            {item.kind === "panel" && item.active ? <span className="zorai-plus-menu__on">Open</span> : null}
          </button>
        </div>
      ))}
    </div>,
    document.body,
  );
}
