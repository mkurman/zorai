import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";

export function TableFullscreen({ title, bodyClassName, closing, onCopyCsv, onPng, onClose, children }: {
  title: string;
  bodyClassName?: string;
  closing?: boolean;
  onCopyCsv: () => void;
  onPng: () => void;
  onClose: () => void;
  children: ReactNode;
}) {
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  if (typeof document === "undefined") return null;
  return createPortal(
    <div className="zorai-table-fullscreen" data-closing={closing || undefined} onPointerDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="zorai-table-fullscreen__panel" role="dialog" aria-label={title}>
        <header className="zorai-table-fullscreen__header">
          <strong>{title}</strong>
          <span className="zorai-table-fullscreen__actions">
            <button type="button" className="zorai-data-table__action" onClick={onCopyCsv}>Copy CSV</button>
            <button type="button" className="zorai-data-table__action" onClick={onPng}>PNG</button>
            <button ref={closeRef} type="button" className="zorai-table-fullscreen__close" aria-label="Close full screen table" onClick={onClose}>×</button>
          </span>
        </header>
        <div className={["zorai-table-fullscreen__body", bodyClassName].filter(Boolean).join(" ")}>{children}</div>
      </div>
    </div>,
    document.body,
  );
}
