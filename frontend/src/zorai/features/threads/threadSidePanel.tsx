import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { create } from "zustand";

export type ThreadPanelId = "handoff" | "gateway" | "participants" | "operations";

type ThreadPanelState = {
  open: ThreadPanelId | null;
  toggle: (id: ThreadPanelId) => void;
  show: (id: ThreadPanelId) => void;
  close: (id?: ThreadPanelId) => void;
};

export const useThreadPanelStore = create<ThreadPanelState>((set) => ({
  open: null,
  toggle: (id) => set((state) => ({ open: state.open === id ? null : id })),
  show: (id) => set({ open: id }),
  close: (id) => set((state) => (id === undefined || state.open === id ? { open: null } : {})),
}));

export function ThreadSidePanel({ title, subtitle, onClose, closing, children }: { title: string; subtitle?: string; onClose: () => void; closing?: boolean; children: ReactNode }) {
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [top, setTop] = useState<number | null>(null);

  useLayoutEffect(() => {
    const place = () => {
      const header = [...document.querySelectorAll(".zorai-thread-header")].find((element) => element.getBoundingClientRect().height > 0);
      setTop(header ? Math.round(header.getBoundingClientRect().bottom + 8) : null);
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, []);

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
    <aside data-closing={closing || undefined} className="zorai-thread-side-panel" role="dialog" aria-label={title} style={top === null ? undefined : { top }}>
      <header>
        <div>
          <h2>{title}</h2>
          {subtitle ? <p>{subtitle}</p> : null}
        </div>
        <button ref={closeRef} type="button" className="zorai-thread-side-panel__close" aria-label={`Close ${title}`} onClick={onClose}>×</button>
      </header>
      <div className="zorai-thread-side-panel__body">{children}</div>
    </aside>,
    document.body,
  );
}
