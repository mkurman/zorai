import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { useAgentStore, type AgentThread } from "@/lib/agentStore";
import {
  EFFORT_POPOVER_WIDTH,
  effortFillRatio,
  effortNeedleAngle,
  effortPopoverPosition,
  effortTickIndex,
} from "./threadEffortModel";
import { resolveThreadOwnerRuntimeProfile } from "./threadOwnerRuntime";
import { applyThreadReasoningEffort, threadReasoningEfforts } from "./threadRuntimeActions";

const GAUGE_PIVOT = { x: 12, y: 15.15 };

function gaugePoint(radius: number, degrees: number): [number, number] {
  const radians = (degrees * Math.PI) / 180;
  return [
    GAUGE_PIVOT.x + radius * Math.cos(radians),
    GAUGE_PIVOT.y - radius * Math.sin(radians),
  ];
}

function gaugeRingSegment(startDeg: number, endDeg: number): string {
  const outer = 8.55;
  const inner = 5.35;
  const [x1, y1] = gaugePoint(outer, startDeg);
  const [x2, y2] = gaugePoint(outer, endDeg);
  const [x3, y3] = gaugePoint(inner, endDeg);
  const [x4, y4] = gaugePoint(inner, startDeg);
  const n = (value: number) => value.toFixed(2);
  return `M ${n(x1)} ${n(y1)} A ${outer} ${outer} 0 0 1 ${n(x2)} ${n(y2)} L ${n(x3)} ${n(y3)} A ${inner} ${inner} 0 0 0 ${n(x4)} ${n(y4)} Z`;
}

const EFFORT_GAUGE_FACE = [
  gaugeRingSegment(198, 138),
  gaugeRingSegment(120, 60),
  gaugeRingSegment(42, -18),
].join(" ");

export function ThreadEffortGauge({ thread }: { thread: AgentThread }) {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const conciergeConfig = useAgentStore((state) => state.conciergeConfig);
  const subAgents = useAgentStore((state) => state.subAgents);
  const profile = resolveThreadOwnerRuntimeProfile(thread, subAgents, agentSettings, conciergeConfig);
  const effort = profile.effort || "medium";
  const ticks = threadReasoningEfforts();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [popoverStyle, setPopoverStyle] = useState<CSSProperties>({});
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const angle = effortNeedleAngle(effort);
  const fill = effortFillRatio(effort);

  useLayoutEffect(() => {
    if (!open || !buttonRef.current) return;
    const update = () => {
      if (!buttonRef.current) return;
      const rect = buttonRef.current.getBoundingClientRect();
      const position = effortPopoverPosition(rect, {
        width: window.innerWidth,
        height: window.innerHeight,
      });
      setPopoverStyle({
        position: "fixed",
        left: position.left,
        right: "auto",
        top: "auto",
        bottom: position.bottom,
        zIndex: 90,
        width: EFFORT_POPOVER_WIDTH,
        minWidth: EFFORT_POPOVER_WIDTH,
      });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (rootRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const select = async (next: string) => {
    if (busy || next === effort) {
      setOpen(false);
      return;
    }
    setBusy(true);
    try {
      await applyThreadReasoningEffort(thread, next);
      setOpen(false);
    } finally {
      setBusy(false);
    }
  };

  const popover =
    open && typeof document !== "undefined"
      ? createPortal(
          <div
            ref={popoverRef}
            className="zorai-effort-gauge__popover"
            role="dialog"
            aria-label="Reasoning effort"
            style={popoverStyle}
          >
            <div className="zorai-effort-gauge__label">{effort}</div>
            <div
              className="zorai-effort-gauge__meter"
              role="slider"
              aria-valuemin={0}
              aria-valuemax={ticks.length - 1}
              aria-valuenow={effortTickIndex(effort)}
              aria-valuetext={effort}
            >
              <div className="zorai-effort-gauge__track">
                <div className="zorai-effort-gauge__fill" style={{ width: `${fill * 100}%` }} />
              </div>
              {ticks.map((tick, index) => (
                <button
                  type="button"
                  key={tick}
                  className={["zorai-effort-gauge__tick", tick === effort ? "is-active" : ""].filter(Boolean).join(" ")}
                  style={{ left: `${(index / Math.max(1, ticks.length - 1)) * 100}%` }}
                  title={tick}
                  aria-label={tick}
                  disabled={busy}
                  onClick={() => void select(tick)}
                />
              ))}
            </div>
            <div className="zorai-effort-gauge__scale">
              <span>{ticks[0]}</span>
              <span>{ticks[ticks.length - 1]}</span>
            </div>
          </div>,
          document.body,
        )
      : null;

  return (
    <div ref={rootRef} className="zorai-effort-gauge">
      <button
        ref={buttonRef}
        type="button"
        className="zorai-composer-icon-button"
        title={`Reasoning effort: ${effort}`}
        aria-label={`Reasoning effort: ${effort}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen((current) => !current)}
      >
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          <path d={EFFORT_GAUGE_FACE} fill="currentColor" />
          <g transform={`rotate(${angle} ${GAUGE_PIVOT.x} ${GAUGE_PIVOT.y})`} fill="currentColor">
            <path d="M11.4 14.25 L12.05 6.45 L12.75 14.4 Z" />
            <circle cx={GAUGE_PIVOT.x} cy={GAUGE_PIVOT.y} r="1.72" />
          </g>
        </svg>
      </button>
      {popover}
    </div>
  );
}
