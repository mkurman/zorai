import { useEffect, useRef, useState } from "react";

const PETAL_TURNS = [0, 60, 120, 180, 240, 300];
const PETAL = "M 50 45.82 A 37.67 37.67 0 0 1 50 11.32 A 37.67 37.67 0 0 1 50 45.82";

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
    <svg className={className} viewBox="0 0 100 100" aria-hidden="true">
      <ToolMarkPaths layer="base" />
      <ToolMarkPaths layer="trace" />
    </svg>
  );
}

function ToolMarkPaths({ layer }: { layer: "base" | "trace" }) {
  return (
    <g className={`acp-tool-mark__${layer}`}>
      <circle className="acp-tool-mark__line" cx="50" cy="50" r="42.86" pathLength="1" />
      {PETAL_TURNS.map((turn) => (
        <path
          key={`${layer}-${turn}`}
          className="acp-tool-mark__line"
          d={PETAL}
          transform={`rotate(${turn} 50 50)`}
          pathLength="1"
        />
      ))}
    </g>
  );
}

function prefersReducedMotion(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
