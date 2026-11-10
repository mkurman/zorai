import { useEffect, useRef, useState } from "react";

const PETAL_TURNS = [0, 60, 120, 180, 240, 300];
const PETAL = "M12 2.2C13.9 4.8 13.9 7.6 12 9.8C10.1 7.6 10.1 4.8 12 2.2Z";

export const TOOL_MARK_ENTER_DELAY_MS = 70;
export const TOOL_MARK_LEAVE_DELAY_MS = 180;
export const TOOL_MARK_FADE_MS = 240;
export const TOOL_MARK_LEAVE_MS = TOOL_MARK_LEAVE_DELAY_MS + TOOL_MARK_FADE_MS;

export type ToolMarkPresence = {
  present: boolean;
  shown: boolean;
  showAfterMs: number | null;
  hideAfterMs: number | null;
};

export function toolMarkPresence(active: boolean, wasPresent: boolean, reduceMotion: boolean): ToolMarkPresence {
  if (!active && !wasPresent) {
    return { present: false, shown: false, showAfterMs: null, hideAfterMs: null };
  }
  if (reduceMotion) {
    return { present: active, shown: active, showAfterMs: null, hideAfterMs: null };
  }
  if (active) {
    return { present: true, shown: false, showAfterMs: TOOL_MARK_ENTER_DELAY_MS, hideAfterMs: null };
  }
  return { present: true, shown: false, showAfterMs: null, hideAfterMs: TOOL_MARK_LEAVE_MS };
}

export function ZoraiToolMark({ active }: { active: boolean }) {
  const presentRef = useRef(active);
  const [present, setPresent] = useState(active);
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const next = toolMarkPresence(active, presentRef.current, prefersReducedMotion());
    presentRef.current = next.present;
    setPresent(next.present);
    if (next.showAfterMs == null) setShown(next.shown);
    const timer = next.showAfterMs != null
      ? window.setTimeout(() => setShown(true), next.showAfterMs)
      : next.hideAfterMs != null
        ? window.setTimeout(() => {
          presentRef.current = false;
          setPresent(false);
          setShown(false);
        }, next.hideAfterMs)
        : 0;
    return () => {
      if (timer) window.clearTimeout(timer);
    };
  }, [active]);

  if (!present) return null;

  const className = shown
    ? "acp-tool-mark acp-tool-mark--shown"
    : active
      ? "acp-tool-mark"
      : "acp-tool-mark acp-tool-mark--leave";

  return (
    <svg className={className} viewBox="0 0 24 24" aria-hidden="true">
      {PETAL_TURNS.map((turn) => (
        <g key={turn} transform={`rotate(${turn} 12 12)`}>
          <path className="acp-tool-mark__petal" d={PETAL} />
        </g>
      ))}
    </svg>
  );
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
