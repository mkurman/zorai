import { useEffect, useRef, useState } from "react";

export type CodeRailTab = "explorer" | "search" | "scm" | "debug" | "testing" | "agent";

const STORAGE_KEY = "zorai.code.railTab";

const TABS: ReadonlyArray<{ id: CodeRailTab; label: string; icon: string }> = [
  { id: "explorer", label: "Explorer", icon: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M9 13h6M9 17h4" },
  { id: "search", label: "Search", icon: "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13M15.5 15.5l5 5" },
  { id: "scm", label: "Source Control", icon: "M6 3v12M6 15a3 3 0 1 0 0 6 3 3 0 0 0 0-6M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6M18 9a9 9 0 0 1-9 9" },
  { id: "debug", label: "Run and Debug", icon: "M8 5v14l11-7zM4 4v4M4 16v4" },
  { id: "testing", label: "Testing", icon: "M9 3h6M10 3v6L4.5 18.5A1.7 1.7 0 0 0 6 21h12a1.7 1.7 0 0 0 1.5-2.5L14 9V3M7 15h10" },
  { id: "agent", label: "Agent", icon: "M12 3v3M7 6h10a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM9.5 11.5v1M14.5 11.5v1M9.5 15.5h5M3 11v3M21 11v3" },
];

export function readStoredCodeRailTab(storage: Pick<Storage, "getItem"> | undefined): CodeRailTab {
  const stored = storage?.getItem(STORAGE_KEY);
  return TABS.some((tab) => tab.id === stored) ? stored as CodeRailTab : "explorer";
}

export function useCodeRailTab(): [CodeRailTab, (tab: CodeRailTab) => void] {
  const storage = typeof window === "undefined" ? undefined : window.localStorage;
  const [tab, setTab] = useState<CodeRailTab>(() => readStoredCodeRailTab(storage));
  return [tab, (next) => {
    setTab(next);
    storage?.setItem(STORAGE_KEY, next);
  }];
}

export function CodeRailTabs({ value, onChange, badges }: {
  value: CodeRailTab;
  onChange: (tab: CodeRailTab) => void;
  badges: Partial<Record<CodeRailTab, { count: number; tone?: "danger" }>>;
}) {
  const drag = useRef<{ x: number; scrollLeft: number; moved: boolean } | null>(null);
  const stripRef = useRef<HTMLDivElement | null>(null);
  const [edges, setEdges] = useState({ left: false, right: false });
  const updateEdges = () => {
    const strip = stripRef.current;
    if (!strip) return;
    const left = strip.scrollLeft > 1;
    const right = strip.scrollLeft + strip.clientWidth < strip.scrollWidth - 1;
    setEdges((current) => (current.left === left && current.right === right ? current : { left, right }));
  };
  useEffect(() => {
    const strip = stripRef.current;
    if (!strip) return;
    updateEdges();
    const observer = new ResizeObserver(updateEdges);
    observer.observe(strip);
    return () => observer.disconnect();
  }, []);
  return (
    <div
      ref={stripRef}
      className={["zorai-code-rail-tabs", edges.left ? "has-more-left" : "", edges.right ? "has-more-right" : ""].filter(Boolean).join(" ")}
      onScroll={updateEdges}
      role="tablist"
      aria-label="Code sidebar"
      onWheel={(event) => {
        const strip = event.currentTarget;
        if (strip.scrollWidth <= strip.clientWidth || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
        strip.scrollLeft += event.deltaY;
      }}
      onPointerDown={(event) => {
        if (event.button !== 0 || event.currentTarget.scrollWidth <= event.currentTarget.clientWidth) return;
        drag.current = { x: event.clientX, scrollLeft: event.currentTarget.scrollLeft, moved: false };
      }}
      onPointerMove={(event) => {
        const state = drag.current;
        if (!state) return;
        const delta = event.clientX - state.x;
        if (!state.moved && Math.abs(delta) < 4) return;
        if (!state.moved) {
          state.moved = true;
          event.currentTarget.setPointerCapture(event.pointerId);
        }
        event.currentTarget.scrollLeft = state.scrollLeft - delta;
      }}
      onPointerUp={() => {
        if (!drag.current?.moved) drag.current = null;
        else window.setTimeout(() => { drag.current = null; }, 0);
      }}
      onPointerCancel={() => { drag.current = null; }}
      onClickCapture={(event) => {
        if (drag.current?.moved) {
          event.preventDefault();
          event.stopPropagation();
        }
        drag.current = null;
      }}
    >
      {TABS.map((tab) => {
        const badge = badges[tab.id];
        return (
          <button
            key={tab.id}
            type="button"
            role="tab"
            aria-selected={value === tab.id}
            className={value === tab.id ? "is-active" : undefined}
            aria-label={tab.label}
            title={tab.label}
            onClick={() => onChange(tab.id)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={tab.icon} /></svg>
            {badge && badge.count > 0 ? (
              <span className={`zorai-code-rail-tabs__badge${badge.tone === "danger" ? " is-danger" : ""}`}>{badge.count > 99 ? "99+" : badge.count}</span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
