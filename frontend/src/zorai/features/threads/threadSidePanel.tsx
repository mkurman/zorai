import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { create } from "zustand";
import { useExclusivePopup } from "@/lib/exclusivePopup";

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

export const ThreadPanelHostContext = createContext<{ active: boolean; element: () => HTMLElement | null } | null>(null);

export const THREAD_PANEL_KEEP_OPEN_SELECTOR = ".zorai-thread-header button, .zorai-thread-participants, [role=\"menu\"], [role=\"listbox\"], [role=\"dialog\"], .zorai-code-context-menu";

export function keepsThreadPanelOpen(target: Pick<Element, "closest">): boolean {
  return Boolean(target.closest(THREAD_PANEL_KEEP_OPEN_SELECTOR));
}

export function ThreadSidePanel({ title, subtitle, onClose, closing, children }: { title: string; subtitle?: string; onClose: () => void; closing?: boolean; children: ReactNode }) {
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useExclusivePopup(onClose, "panel");
  const host = useContext(ThreadPanelHostContext);
  const [top, setTop] = useState<number | null>(null);
  const [right, setRight] = useState<number | null>(null);

  useLayoutEffect(() => {
    const place = () => {
      const hostElement = host?.element() ?? null;
      const header = hostElement?.querySelector(".zorai-thread-header")
        ?? [...document.querySelectorAll(".zorai-thread-header")].find((element) => element.getBoundingClientRect().height > 0);
      setTop(header ? Math.round(header.getBoundingClientRect().bottom + 8) : null);
      setRight(hostElement ? Math.max(16, Math.round(window.innerWidth - hostElement.getBoundingClientRect().right + 8)) : null);
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [host]);

  const panelRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (!target || panelRef.current?.contains(target) || keepsThreadPanelOpen(target)) return;
      onCloseRef.current();
    };
    window.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown, true);
    };
  }, []);

  if (typeof document === "undefined" || host?.active === false) return null;
  return createPortal(
    <aside ref={panelRef} data-closing={closing || undefined} className="zorai-thread-side-panel" role="dialog" aria-label={title} style={top === null ? undefined : { top, maxHeight: `calc(100vh - ${top + 16}px)`, ...(right === null ? {} : { right }) }}>
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
